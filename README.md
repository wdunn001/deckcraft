# Cloud Open, Save and Share, as rendered

Review material for the `cloud-pages` pull request against `storytold/deckcraft`.
Every file here is written by `cloud::tests::renders_for_review` in
`crates/ui-egui/src/cloud_tests.rs`, with `DECKCRAFT_CLOUD_RENDERS` pointing at a
directory. The window is 1440 by 900 points at 1.4 device pixels per point, drawn
offscreen by a software renderer against a cloud service that lives inside the
test.

| File | What it shows |
|---|---|
| `deckcraft-open-page.png` | File › Open with a Cloud section: your own files beside what other people shared with you, a folder to walk into, and a `…` on every presentation |
| `deckcraft-open-page-overflow.png` | That `…` open, holding Open, Share and Work together |
| `deckcraft-save-page.png` | File › Save As with This Computer and the cloud account, a folder to pick, a file name, and a line saying which format it writes |
| `deckcraft-title-bar-presence.png` | The title bar with Share, the presence row and the account badge |
| `deckcraft-share-split-menu.png` | The Share chevron, holding Work together and DeckCraft's own Save a Copy formats |
| `deckcraft-share-dialog.png` | The dialog the Share label opens, with real access on it |

The "Inter isn't installed" notice in two of these is the build container
saying it has no system fonts. It is not part of the feature.

Everything visible is DeckCraft's own interface, its own icons and openly
licensed fonts. These screenshots are therefore contributor-original under
`MIT OR Apache-2.0` (see the repository's `AGENTS.md` §1.2).
