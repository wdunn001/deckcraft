# Cloud Open/Save: branch notes

This branch (`cloud-pages-remade`) rebuilds `storytold/deckcraft#711`. That PR's title was
"Cloud: Open, Save and Share against cloud storage." This rebuild follows the checklist in review
`5764297647` on `storytold/designcraft#878`. A second, adversarial pass followed, against the
first wiring. This file records both rounds truthfully.

## The real contract is now wired

`storytold/craft-libs#160` added the vendor-neutral contracts. It also added the craft-ui cloud
and share pages, plus a local reference adapter. It merged on 2026-11-17. `storytold/craft-libs#665`
added a provider-neutral suite account list and a keychain token store. It merged on 2026-11-18.
Both land on `storytold/craft-libs` `main` at `52fc6b5b3d2179239664136e84543231f8576f8d`. This
branch pins `craft-ui` (feature `cloud`) and `craft-cloud` to that single rev. The workspace's
other `craft-libs` crates stay on their own, earlier revs. That multi-rev state predates this
branch. It is not this change's job to reconcile it.

An earlier version of this branch defined its own local `CloudBackend`/`Job`/`Event` types.
`#160` had not merged yet at the time. That module is gone now.
`crates/ui-egui/src/cloud.rs` implements the real `craft_ui::cloud::CloudApp` trait for
`SlideApp`. All 23 methods are there, including `sharing() -> &mut craft_ui::share::ShareDialog`
and `presence() -> &mut craft_ui::presence::PresenceBar`. The first pass treated both as
optional. Both are mandatory on the real trait. DeckCraft owns both starting with this branch's
first commit.

## The backend is a local folder

`apps/deckcraft/src/cloud.rs` connects `craft-cloud-local`'s `LocalBackend`. It is `#160`'s own
reference `CloudBackend`. It treats one folder on disk as "the cloud." It reaches no service and
no personal fork. It connects only when `DECKCRAFT_CLOUD_DIR` (or `CRAFT_CLOUD_DIR`) names a
folder. Every cloud menu item, page and badge stays hidden otherwise. That is the same behaviour
as with no backend at all. `apps/deckcraft-web` connects nothing. `craft-cloud-local` needs real
filesystem access. A wasm build has none. No other backend is wired for the web build either. A
future storytold-hosted service can replace `LocalBackend` with a call to its own client crate.
That call would go through the same `CloudFiles::connect` seam.

## Checklist items, against the wired version

Item 3 asked for no collab-craft crates. That still holds. `craft-cloud-client` and
`collabcraft-share` are not referenced anywhere on this branch. The backend is
`craft-cloud-local`, a `storytold` crate.

Item 4 asked for an app that talks only to the contract. It asked that the app name no provider
type. It asked that every cloud surface stay hidden without a backend. All three hold. They hold
more fully than the first pass managed. `CloudApp` is the only seam DeckCraft's own code touches.
The account state lives inside `craft_ui::cloud::CloudFiles`. The share state lives inside
`craft_ui::share::ShareDialog`. The presence state lives inside `craft_ui::presence::PresenceBar`.
DeckCraft invented none of that state itself. `grep -r wdunn001 Cargo.lock` is empty.

Item 5 asked for builds and tests that pass with no cloud service. They do.
`crates/ui-egui/src/cloud_tests.rs` exercises the real contract against an in-process mock
backend.

Item 6 asked for no workflow changes. None were made. That was true before this rewrite too.

Items 1 and 2 asked for a pin to a `storytold/craft-libs` `main` rev, with no `wdunn001`
revision anywhere. That now holds for real. `craft-ui` and `craft-cloud` are pinned to
`52fc6b5b3d2179239664136e84543231f8576f8d`.

## What the adversarial review found

An adversarial review of the first wiring (the local-module version) ran nine probes. It
confirmed eleven findings. The review's own cost analysis called for wiring the real contract
first. Several findings are architectural. Replacing the module outright fixes those by
construction. Everything else needed a direct fix on top. This branch took that order.

**Fixed by construction, by adopting `craft_ui::cloud` in place of the local module:**

- **Finding 1**: a rejected account call resubmitted every frame, forever.
  `CloudFiles::asked_account` is set once, before the first job goes out. Only `sign_in`/`sign_out`
  clear it. A failure does not.
- **Finding 2**: signing out, then one more frame, signed the user back in from a stale reply.
  `Post::send` drops any answer whose generation does not match the one current when it was
  issued. `sign_out` and `sign_in` both bump that generation.
- **Finding 3**: the generation guard itself. It is the mechanism behind finding 2.
- **Finding 4**: "Shared with Me" filled a field nothing drew. `craft_ui::cloud::open_page` draws
  `CloudFiles::shared` itself, in its own column, through `Source`. The local module's `open_page`
  drew `mine` only.
- **Finding 5**: faults surfaced as ad hoc strings. `Fault` is now the contract's own enum.
  `NotLinked` draws as a state with an obvious next step. It never draws as a red error.
  `Conflict` routes to a real conflict modal, `craft_ui::cloud::conflict_modal`. A generic
  message no longer stands in for it.
- **Finding 6/7**: a save landing on a switched window never cleared its own document's dirty
  flag. `craft_ui::cloud::apply` calls `CloudApp::mark_saved(uid)` by the document's own uid.
  Which window happens to be active plays no part in that call. `Session::document_mut` (new, in
  `crates/engine/src/lib.rs`) is what makes the call possible from outside the engine crate. The
  local module had no such accessor. It silently skipped the update for a document that was not
  active when the answer arrived.
- **Finding 8**: `save_as(...).is_ok()` discarded the error on a refused save. The real
  `craft_ui::cloud::save_page` always calls `app.say(error)` on a refusal. This branch calls that
  function directly now. The hand-rolled one that dropped the `Err` is gone.
- **Finding 12**: the Save As destination folder was invisible, or only implicit. `save_page`
  draws its own "Folder" heading, with the current path and Up/folder navigation. This was
  checked by reading the function itself.

**Fixed directly; the contract leaves these to the host:**

- **Finding 9**: `.md` could be opened but never saved back. A typed `.pdf` name also got
  mangled into `.pdf.deckcraft`. `crate::cloud::PRESENTATION`'s extension list now includes
  `pdf`. DeckCraft can write one. `crate::cloud::writes_as` maps `.md` to the same plain-text
  outline writer `.txt` already used. Open and Save now agree on both formats. One known
  residual remains. A `.pdf` row in a cloud listing shows as open-enabled. `opens` and
  `document_name` share one extension list in `craft_ui::cloud`. DeckCraft cannot read a PDF back
  as an editable presentation. Clicking one fails with an ordinary "could not open" message. It
  never crashes. Closing this gap fully would need a capability split the contract does not
  offer.
- **Finding 10**: `cloud.save` and `cloud.reload` skipped the availability check every other
  `cloud.*` command had. Both arms in `crates/ui-egui/src/app/commands.rs` now check
  `craft_ui::cloud::available` first. Both answer `UNAVAILABLE`, like every other arm, when it
  answers false. The library's own `save`/`reload_active` carry no availability check of their
  own. An origin can only exist with a backend connected. Calling either with none connected
  therefore already failed, just through a different message. This gap is why the fix had to
  land on the host side.
- **Finding 11**: two vacuous tests. `who_and_open_page_ask_nothing_and_draw_nothing_without_a_backend`
  and its connected counterpart now drive `who`/`open_page` through a real `egui_kittest::Harness`.
  The earlier version called `start`/`pump` directly. Both functions went untested as a result.
  The save test (`a_save_landing_while_a_different_window_is_active_still_clears_its_own_dirty_flag`)
  asserts `is_dirty()` directly, before and after, on the document that was saved. It never reads
  the dirty flag of whichever document happens to be active when the assertion runs.

## Host obligations, checked against the fleet standard

Every `cloud.*` entry in `UI_COMMANDS` is dispatched through `SlideApp::run`. Every arm in
`crates/ui-egui/src/app/commands.rs` checks `craft_ui::cloud::available` first. None carries a
keyboard shortcut. The command palette lists them by label, like any other command. The same
guard applies once one is chosen. That covers the fleet standard's command, search and shortcut
surfaces.

`crates/ui-egui/src/backstage.rs` calls `cancel_save_as` whenever the Save As page is left
without submitting. It also calls it on the way to an unrelated page. A stale typed name can no
longer carry into the next Save As. Neither can a conflict's "Save as a copy" target.

`crates/ui-egui/src/app.rs`'s `ui_inner` calls `ShareDialog::show` unconditionally, every frame.
The dialog keeps collecting its own answers while it stays closed. It is not limited to the
frames where it happens to be open.

The quit-confirmation check in `app.rs` reads `is_dirty()` across every open document. A cloud
save in flight has not cleared that flag yet. `mark_saved` only runs once the `Saved` event
lands. The existing unsaved-changes prompt already catches a quit attempted mid-save. No
separate `busy` check needed adding on top of it.

`CloudApp::open_bytes` opens a cloud document with `path: None`. `file.openBytes` never receives
a path for one. `SlideApp::save()`'s routing (`crates/ui-egui/src/app/files.rs`) keeps sending
that document back to the cloud once it has a uid. It never starts treating the document as a
local file on its own.

DeckCraft has no translation catalog. The i18n obligation does not apply here as a result.
`CloudApp::tr` is left at its default. It passes the English text through unchanged, the same
treatment every other string in this crate gets.

## Where the pieces are

`crates/ui-egui/src/cloud.rs` holds the `CloudApp` impl for `SlideApp`. `PRESENTATION`
(`DocumentKind`) lives there too. So do `writes_as`, `copies`, `service_host`, `Sent` and
`status`. `crates/ui-egui/src/cloud_tests.rs` holds the mock-backed regressions. The nine
adversarial probes live there too, adapted to the real types. `crates/engine/src/lib.rs` adds
`Session::document_mut`. `mark_saved` needs that accessor.

`crates/ui-egui/src/lib.rs` adds four fields to `SlideApp`. `cloud: craft_ui::cloud::CloudFiles`
is one. `cloud_sent: cloud::Sent` is another. `share: craft_ui::share::ShareDialog` and
`presence: craft_ui::presence::PresenceBar` round out the four. The file also moves
`pending_urls` behind a `RefCell`. `CloudApp::open_url` takes `&self`. The old `&mut self` field
could not satisfy that signature. `crates/ui-egui/src/app.rs` adds the field init. It calls
`craft_ui::cloud::pump` on every `logic()` pass. `ShareDialog::show` runs on every `ui()` pass
too. So do `conflict_modal` and the `share_save` handshake. `crates/ui-egui/src/app/commands.rs` adds
the `cloud.*` command dispatch. Every arm there checks availability first.
`crates/ui-egui/src/app/files.rs` makes `SlideApp::save()` redirect to `craft_ui::cloud::save`,
for a presentation with no local path and a cloud target.

`crates/ui-egui/src/backstage/files.rs` and `crates/ui-egui/src/backstage/info.rs` add the Open
page's Cloud section and the Save As page's destination choice. Both call the real
`open_page`/`save_page`. Both gate on `craft_ui::cloud::available`. `crates/ui-egui/src/backstage.rs`
adds the `Backstage::save_to_cloud` field and the `cancel_save_as` call on leaving the page.
`crates/ui-egui/src/ribbon/title_bar.rs` adds the `craft_ui::cloud::who` account badge and the
`share_button` split button.

`apps/deckcraft/src/cloud.rs` connects `craft-cloud-local`'s `LocalBackend` when
`DECKCRAFT_CLOUD_DIR` or `CRAFT_CLOUD_DIR` is set.

## What's left before this reopens as a PR

1. A maintainer decides whether the bundled local-folder backend belongs in a shipped DeckCraft.
   The alternative is waiting for a real storytold-hosted service first. Only
   `apps/deckcraft/src/cloud.rs` would change either way. Nothing in `crates/ui-egui` depends on
   that choice.
2. No backend is connected for sharing today. `ShareDialog::connect` is never called.
   `shares(app)` answers false as a result. The Share button stays hidden even with the
   local-folder backend connected. `craft-cloud-local` is a file backend only. A `ShareBackend`
   implementation is a separate, later piece of work, for whichever real service ends up behind
   this.
3. Someone verifies the Open/Save/account-badge/Share layout by hand. That check needs the
   local-folder backend, real folders and real account names. None of that is tested beyond the
   mock and the kittest harness in `cloud_tests.rs`.
4. The maintainers give the go-ahead to open the PR. This branch stands ready for that. It has
   not been submitted.
