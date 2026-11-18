//! DeckCraft's cloud backend: a local folder, through craft-libs' own reference `CloudBackend`.
//!
//! No CollabCraft, no storytold-hosted service and no personal fork of either.
//! `craft-cloud-local` (`storytold/craft-libs#160`) treats one folder on disk as "the cloud,"
//! proving the contract needs no specific provider. Connected only when `DECKCRAFT_CLOUD_DIR`
//! (or `CRAFT_CLOUD_DIR`) names a folder; left unconnected otherwise, so every cloud menu item,
//! page and badge stays hidden exactly as when no backend exists at all. A future
//! storytold-hosted service replaces this with a call to its own client crate, through the same
//! `CloudFiles::connect` seam; nothing above that seam would need to change.

use deckcraft_ui_egui::SlideApp;

/// Where the local "cloud" folder is, from the environment.
///
/// Read from the environment and compiled in nowhere: a desktop binary must not carry a path
/// that belongs to one machine.
fn root() -> Option<std::path::PathBuf> {
    ["DECKCRAFT_CLOUD_DIR", "CRAFT_CLOUD_DIR"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from)
        .find(|path| !path.as_os_str().is_empty())
}

/// Connects the local-folder backend, when one is configured. Without it, `app.cloud` stays as
/// `SlideApp::new` left it: no backend, nothing cloud-shaped offered anywhere.
pub fn connect(app: &mut SlideApp) {
    let Some(path) = root() else { return };
    log::info!("cloud storage: local folder {}", path.display());
    app.cloud.connect(craft_cloud_local::LocalBackend::new(path).named("Local Folder"));
}
