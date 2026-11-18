//! Cloud storage, against an in-process mock [`CloudBackend`] and the real `craft_ui::cloud`
//! contract (`storytold/craft-libs#160`, `#665`). No cloud service exists in these tests, or in
//! a shipped DeckCraft unless `DECKCRAFT_CLOUD_DIR` is set; see
//! `docs/cloud-pages-remade-notes.md`.
//!
//! Several of these started as adversarial probes against the branch's earlier, locally defined
//! contract (before this crate adopted `craft_ui::cloud` directly): a rejected account call that
//! resubmitted forever, a stale reply from a signed-out session landing on the next one, `save`
//! and `reload` silently skipping the availability check every other arm had, an `.md` entry
//! that opened but could never be saved back, a save that landed on a switched window leaving
//! its own document dirty forever, and a "Shared with Me" listing that filled a field nothing
//! ever drew. Each is kept here as a regression, adapted to the real types; each still fails if
//! its guard is removed (checked by hand against this crate before landing).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use deckcraft_engine::Session;
use egui_kittest::Harness;
use serde_json::json;

use super::*;
use crate::{Services, SlideApp};

/// Answers every job the moment it is submitted, synchronously (no thread, unlike a real
/// backend), with scripted results, and counts the account jobs it was asked. A test mutates
/// the scripted results through the `Arc` it keeps, then connects the rest.
struct Mock {
    account: Mutex<Result<Account, Fault>>,
    entries: Mutex<Result<Vec<Entry>, Fault>>,
    bytes: Mutex<Result<Vec<u8>, Fault>>,
    saved: Mutex<Result<Entry, Fault>>,
    account_jobs: AtomicUsize,
}

impl Mock {
    fn new() -> Arc<Mock> {
        Arc::new(Mock {
            account: Mutex::new(Ok(Account { display: "Dana".to_owned(), capabilities: vec![], provider_linked: true })),
            entries: Mutex::new(Ok(vec![])),
            bytes: Mutex::new(Err(Fault::Refused("no bytes were scripted".to_owned()))),
            saved: Mutex::new(Err(Fault::Refused("no save was scripted".to_owned()))),
            account_jobs: AtomicUsize::new(0),
        })
    }
}

impl Mock {
    fn handle(&self, job: Job, post: Post) {
        #[allow(clippy::unwrap_used)]
        match job {
            Job::Account | Job::SignIn => {
                self.account_jobs.fetch_add(1, Ordering::Relaxed);
                post.send(Event::Account(self.account.lock().unwrap().clone()));
            }
            Job::SignOut | Job::LinkStart | Job::LinkPoll(_) | Job::Conversation(_) => {}
            Job::List(path) => post.send(Event::Listed { path, entries: self.entries.lock().unwrap().clone() }),
            Job::SharedWithMe => post.send(Event::Shared(self.entries.lock().unwrap().clone())),
            Job::Read { entry, into } => post.send(Event::Read { entry, into, bytes: self.bytes.lock().unwrap().clone() }),
            Job::Write { uid, .. } => post.send(Event::Saved { uid, entry: self.saved.lock().unwrap().clone() }),
            Job::Create { uid, name, .. } => {
                let entry = self.saved.lock().unwrap().clone().map(|mut e| {
                    e.name = name;
                    e
                });
                post.send(Event::Saved { uid, entry });
            }
        }
    }
}

/// `CloudBackend` is `craft_ui`'s and `Arc` is `std`'s; the orphan rules refuse `impl
/// CloudBackend for Arc<Mock>` directly, even though `Mock` is local. This newtype is the usual
/// way round that: wholly local, so the impl is unambiguously ours.
struct Backend(Arc<Mock>);

impl CloudBackend for Backend {
    fn submit(&self, job: Job, post: Post) {
        self.0.handle(job, post);
    }
}

fn entry(id: &str, name: &str) -> Entry {
    Entry {
        id: id.to_owned(),
        room: id.to_owned(),
        name: name.to_owned(),
        path: format!("/{name}"),
        kind: EntryKind::File,
        size: None,
        modified: None,
        mime: None,
        etag: Some("v1".to_owned()),
        writable: true,
        shared_by: None,
    }
}

/// A fresh presentation's own `.deckcraft` bytes, for scripting a cloud Read.
fn presentation_bytes(app: &mut SlideApp) -> Vec<u8> {
    app.run("file.new", json!({})).expect("file.new");
    let doc = app.session.active().expect("a new presentation is open").doc.clone();
    let bytes = deckcraft_engine::cmd::file::save_bytes(&doc, "deckcraft").expect("encode");
    app.close_presentation();
    bytes
}

fn app() -> SlideApp {
    SlideApp::new(Session::new(), Services::default())
}

// ---------------------------------------------------------------- availability

#[test]
fn unavailable_until_a_backend_is_connected() {
    let app = app();
    assert!(!available(&app));
    assert_eq!(status(&app)["available"], json!(false));
}

#[test]
fn connecting_a_backend_makes_it_available() {
    let mut app = app();
    app.cloud.connect(Backend(Mock::new()));
    assert!(available(&app));
}

/// Regression (finding 10): `cloud.save` and `cloud.reload` must answer `UNAVAILABLE` exactly
/// like every other `cloud.*` command when no backend is connected, not some other error that
/// happens to fall out of looking for a cloud origin that can never exist. Dispatched through
/// `SlideApp::run`, the same path the ribbon and the control channel use, so this fails if the
/// guard in `app/commands.rs` is ever dropped from one of these two arms specifically.
#[test]
fn cloud_save_and_reload_commands_report_unavailable_like_every_other_arm() {
    let mut app = app();
    app.run("file.new", json!({})).expect("file.new");
    assert_eq!(app.run("cloud.save", json!({})).unwrap_err(), UNAVAILABLE);
    assert_eq!(app.run("cloud.reload", json!({})).unwrap_err(), UNAVAILABLE);
    assert_eq!(app.run("cloud.browse", json!({})).unwrap_err(), UNAVAILABLE);
}

// ---------------------------------------------------------------- sign-in state (findings 1-3)

/// Regression (PROBE 1, adapted): a rejected account call must not make `start` resubmit it on
/// every frame, forever. `CloudFiles::asked_account` is what stops this, set before the first
/// job goes out and cleared only by `sign_in`/`sign_out`, never by a failure. Counts the actual
/// jobs sent, not just the state after: a resubmit-forever bug reaches the same final state
/// (signed out, a fault recorded) that the fix does, so only the count tells them apart.
#[test]
fn a_rejected_account_call_is_asked_for_only_once_across_many_frames() {
    let mut app = app();
    let mock = Mock::new();
    *mock.account.lock().unwrap() = Err(Fault::Unauthenticated);
    app.cloud.connect(Backend(mock.clone()));
    let ctx = egui::Context::default();
    for _ in 0..30 {
        start(&mut app);
        pump(&mut app, &ctx);
    }
    assert_eq!(mock.account_jobs.load(Ordering::Relaxed), 1, "start must not resubmit the account job once it was refused");
    assert!(app.cloud.account.is_none());
    assert!(app.cloud.account_fault.is_some());
}

/// Regression (PROBE 2, adapted): signing out, then one more frame of the Open page, must not
/// sign the user straight back in from a reply that was already in flight. `Post::send` drops
/// an answer whose generation does not match the one current when it was issued; `sign_out`
/// bumps it. The fresh `Account` job this frame's own `start` asks for still lands normally.
#[test]
fn sign_out_is_not_undone_by_a_stale_reply_on_the_next_frame() {
    let mut app = app();
    let mock = Mock::new();
    app.cloud.connect(Backend(mock.clone()));
    let ctx = egui::Context::default();
    start(&mut app);
    pump(&mut app, &ctx);
    assert!(app.cloud.account.is_some(), "signed in");
    sign_out(&mut app);
    pump(&mut app, &ctx);
    assert!(app.cloud.account.is_none(), "signed out");
    start(&mut app);
    pump(&mut app, &ctx);
    assert!(app.cloud.account.is_some(), "the fresh sign-in this frame's own `start` asked for still lands");
    assert_eq!(mock.account_jobs.load(Ordering::Relaxed), 2, "exactly one job for each of the two sign-ins, no stale extra");
}

// ---------------------------------------------------------------- document kind (finding 9)

#[test]
fn opens_matches_by_extension_and_by_content_type_and_never_a_folder() {
    assert!(opens(&entry("1", "Talk.deckcraft"), PRESENTATION));
    assert!(opens(&entry("1", "Talk.pptx"), PRESENTATION));
    assert!(opens(&Entry { mime: Some("text/plain; charset=utf-8".to_owned()), ..entry("1", "Notes") }, PRESENTATION));
    assert!(!opens(&entry("1", "Movie.mp4"), PRESENTATION));
    assert!(!opens(&Entry { kind: EntryKind::Folder, ..entry("1", "Talk.deckcraft") }, PRESENTATION));
}

/// Regression (PROBE 4, adapted): a typed `.pdf` name must survive Save As rather than become
/// `Deck.pdf.deckcraft`, since DeckCraft can write a PDF and `document_name`/`opens` share one
/// extension list in `craft_ui::cloud`. `.md` is in that list too, since it is now a writable
/// destination (see the next test).
#[test]
fn document_name_keeps_extensions_deckcraft_can_actually_write() {
    assert_eq!(document_name("Deck.pdf", PRESENTATION), "Deck.pdf", "a .pdf name must survive: DeckCraft can write one");
    assert_eq!(document_name("Notes.md", PRESENTATION), "Notes.md");
    assert_eq!(document_name("Q3 Review", PRESENTATION), "Q3 Review.deckcraft");
}

// ---------------------------------------------------------------- opening and saving

#[test]
fn opening_a_cloud_entry_loads_it_and_remembers_where_it_came_from() {
    let mut app = app();
    let bytes = presentation_bytes(&mut app);
    let mock = Mock::new();
    *mock.bytes.lock().unwrap() = Ok(bytes);
    app.cloud.connect(Backend(mock));
    let ctx = egui::Context::default();

    open_entry(&mut app, entry("42", "Keynote.deckcraft"), None);
    pump(&mut app, &ctx);

    assert!(app.session.active().is_some(), "the cloud entry opened a document");
    assert_eq!(origin(&app).map(|o| o.entry.id.clone()), Some("42".to_owned()));
    assert!(has_target(&app));
}

/// Regression (PROBE 5, adapted): an `.md` entry opened from the cloud must be savable back to
/// its origin. `writes_as` (`crate::cloud`) maps `.md` to the plain-text outline writer instead
/// of refusing it, so Save no longer dead-ends on a format Open itself offered.
#[test]
fn an_md_entry_opened_from_the_cloud_can_be_saved_back() {
    let mut app = app();
    let bytes = presentation_bytes(&mut app);
    let mock = Mock::new();
    *mock.bytes.lock().unwrap() = Ok(bytes);
    *mock.saved.lock().unwrap() = Ok(entry("7", "Notes.md"));
    app.cloud.connect(Backend(mock));
    let ctx = egui::Context::default();

    open_entry(&mut app, entry("7", "Notes.md"), None);
    pump(&mut app, &ctx);
    assert!(app.session.active().is_some(), "the .md entry opened");
    assert!(has_target(&app), "it has a cloud origin");

    save(&mut app).expect("an .md presentation must be savable back to its origin");
    pump(&mut app, &ctx);
    assert!(app.cloud.status.as_deref().is_some_and(|s| s.contains("Saved")));
}

/// Regression (PROBE 6, adapted): a save that lands while a different presentation is active
/// must still clear its own document's dirty flag. `CloudApp::mark_saved` is dispatched by uid
/// in `craft_ui::cloud`'s own event handling, never gated on which window happens to be active
/// when the answer arrives.
#[test]
fn a_save_landing_while_a_different_window_is_active_still_clears_its_own_dirty_flag() {
    let mut app = app();
    let bytes = presentation_bytes(&mut app);
    let mock = Mock::new();
    *mock.bytes.lock().unwrap() = Ok(bytes);
    *mock.saved.lock().unwrap() = Ok(entry("42", "Keynote.deckcraft"));
    app.cloud.connect(Backend(mock));
    let ctx = egui::Context::default();

    open_entry(&mut app, entry("42", "Keynote.deckcraft"), None);
    pump(&mut app, &ctx);
    let uid = app.session.active().expect("open").uid;
    app.run("slide.new", json!({"layout": "blank"})).expect("slide.new");
    assert!(app.session.documents().iter().any(|d| d.uid == uid && d.is_dirty()), "the edit made it dirty");

    save(&mut app).expect("save submits");
    // A second presentation, made active while the first save's bytes are "on the wire".
    app.run("file.new", json!({})).expect("file.new");
    assert_ne!(app.session.active().map(|d| d.uid), Some(uid), "a different window is active when the answer lands");
    pump(&mut app, &ctx);

    assert!(
        !app.session.documents().iter().any(|d| d.uid == uid && d.is_dirty()),
        "a save that reached the service must clear its own document's dirty flag, even on a switched window"
    );
}

/// Regression (PROBE 7, adapted): "Shared with Me" fires a job whose answer must land in the
/// listing `craft_ui::cloud::open_page` actually draws for that side (`cloud.shared`), not in
/// `cloud.mine`, which is a different `Folder` entirely.
#[test]
fn shared_with_me_entries_land_in_the_shared_listing_not_mine() {
    let mut app = app();
    let mock = Mock::new();
    *mock.entries.lock().unwrap() = Ok(vec![entry("5", "Theirs.deckcraft")]);
    app.cloud.connect(Backend(mock));
    let ctx = egui::Context::default();
    load_shared(&mut app);
    pump(&mut app, &ctx);
    assert_eq!(app.cloud.shared.entries.len(), 1);
    assert_eq!(app.cloud.mine.entries.len(), 0, "a shared listing must never land in `mine`");
}

#[test]
fn save_as_creates_a_new_cloud_file_and_gives_the_presentation_a_target() {
    let mut app = app();
    app.run("file.new", json!({})).expect("file.new");
    let uid = app.session.active().expect("a document is open").uid;
    let mock = Mock::new();
    *mock.saved.lock().unwrap() = Ok(entry("99", "New Deck.deckcraft"));
    app.cloud.connect(Backend(mock));
    let ctx = egui::Context::default();

    assert!(!has_target(&app));
    save_as(&mut app, "", "New Deck").expect("save_as submits");
    pump(&mut app, &ctx);

    assert_eq!(origin(&app).map(|o| o.entry.id.clone()), Some("99".to_owned()));
    assert_eq!(app.session.active().map(|d| d.uid), Some(uid));
    assert!(has_target(&app));
}

/// Regression: a save refused with a conflict must put up the conflict, never be discarded the
/// way a bare `save(...).is_ok()` would (finding 8). Fixed by delegating to the real
/// `craft_ui::cloud` event handling, which always either calls `app.say` or sets
/// `CloudFiles::conflict`.
#[test]
fn a_conflicted_save_is_reported_as_a_conflict_not_silently_dropped() {
    let mut app = app();
    let bytes = presentation_bytes(&mut app);
    let mock = Mock::new();
    *mock.bytes.lock().unwrap() = Ok(bytes);
    *mock.saved.lock().unwrap() = Err(Fault::Conflict);
    app.cloud.connect(Backend(mock));
    let ctx = egui::Context::default();

    open_entry(&mut app, entry("42", "Keynote.deckcraft"), None);
    pump(&mut app, &ctx);
    save(&mut app).expect("save submits");
    pump(&mut app, &ctx);

    assert!(app.cloud.conflict.is_some(), "a conflicted save must be surfaced, not silently dropped");
}

// ---------------------------------------------------------------- who()/open_page() themselves

fn draw_cloud_ui(app: &mut SlideApp, ui: &mut egui::Ui) {
    // `who`/`open_page` paint text directly, outside the usual `SlideApp::ui` pass that installs
    // the app's fonts; a bare `Harness` otherwise starts with none at all.
    ui.ctx().set_fonts(crate::theme::font_definitions(&[]));
    pump(app, ui.ctx());
    who(app, ui);
    ui.separator();
    open_page(app, ui);
}

/// Regression (finding 11): the earlier version of this test asserted on `start`/`pump` called
/// directly, never on `who`/`open_page` themselves, so it would have kept passing even if both
/// functions were deleted. This one drives them through a real `Ui` and fails if either stops
/// doing its job.
#[test]
fn who_and_open_page_ask_nothing_and_draw_nothing_without_a_backend() {
    let app = app();
    let mut h = Harness::builder().build_ui_state(|ui, app: &mut SlideApp| draw_cloud_ui(app, ui), app);
    h.run_steps(3);
    assert!(h.state().cloud.account.is_none());
    assert!(!h.state().cloud.mine.loaded, "open_page must never call start without a connected backend");
}

#[test]
fn who_and_open_page_actually_sign_in_and_list_when_a_backend_is_connected() {
    let mut app = app();
    let mock = Mock::new();
    *mock.entries.lock().unwrap() = Ok(vec![entry("1", "Talk.deckcraft")]);
    app.cloud.connect(Backend(mock));
    let mut h = Harness::builder().build_ui_state(|ui, app: &mut SlideApp| draw_cloud_ui(app, ui), app);
    h.run_steps(4);
    assert_eq!(h.state().cloud.account.as_ref().map(|a| a.display.as_str()), Some("Dana"));
    assert_eq!(h.state().cloud.mine.entries.len(), 1);
}
