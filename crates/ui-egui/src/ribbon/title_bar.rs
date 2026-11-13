use super::{Align2, Icon, Rect, Sense, SlideApp, Tokens, Ui, icon_toggle, json, pos2, theme, vec2};

pub const TITLE_H: f32 = 38.0;

/// macOS draws the window into the title bar (`with_fullsize_content_view`) and AppKit paints the
/// traffic lights itself, 16 pt from the top of the window on macOS 27. Moving them would take
/// AppKit calls (`unsafe`, forbidden here), so on macOS the title bar is the band they are centred
/// in, with the controls beside them (#71). Up to macOS 15 the lights are smaller and centred 14 pt
/// from the top, about 2 pt above this bar's centre.
pub const MAC_TITLE_BAR: f32 = 32.0;

/// The traffic lights as AppKit lays them out, measured on macOS 27: button diameter,
/// centre-to-centre pitch and the first button's left edge.
pub(super) const MAC_LIGHT: f32 = 14.0;

pub(super) const MAC_LIGHT_PITCH: f32 = 23.0;

pub(super) const MAC_LIGHTS_LEFT: f32 = 9.0;

/// Where title-bar content starts on macOS: the end of the three lights plus the gap the lights
/// keep from the top of the window.
pub const MAC_CONTENT_LEFT: f32 = MAC_LIGHTS_LEFT + 2.0 * MAC_LIGHT_PITCH + MAC_LIGHT + (MAC_TITLE_BAR - MAC_LIGHT) / 2.0;

/// The title bar's height, the left edge of its content and the height of its tallest control.
/// With the integrated title bar (macOS) the bar is shorter, so the controls shrink to keep a
/// margin above and below them.
pub(super) fn title_geometry(integrated: bool) -> (f32, f32, f32) {
    if integrated { (MAC_TITLE_BAR, MAC_CONTENT_LEFT, 22.0) } else { (TITLE_H, 10.0, 26.0) }
}

/// The window's title row: traffic-light space, quick access, centred title, actions.
pub fn title_bar(app: &mut SlideApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let (height, left_offset, control_h) = title_geometry(app.integrated_titlebar);
    egui::Panel::top("title_bar").exact_size(height).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let rect = ui.max_rect();
        // Dragging the empty title bar moves the window.
        let drag = ui.interact(rect, ui.scope_id().with("titledrag"), Sense::click_and_drag());
        // The drag surface is not a button: a full-width Button would be announced as one.
        drag.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Window, true, "Window title bar"));
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            let max = ui.input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
        }
        let total_w = rect.width();

        // Responsive panel geometry via the shared helper (also called by the layout tests).
        // Each panel is capped at its natural maximum (420 px left, 560 px right) but must not
        // cross the centre. At narrow viewports the centre acts as a hard stop so the two
        // panels never overlap. At desktop widths both caps kick in so the layout is
        // identical to the original.
        let (left_max_x_rel, right_min_x_rel) = title_bar_panel_bounds(total_w, left_offset);
        let left_max_x = rect.min.x + left_max_x_rel;
        let right_min_x = rect.min.x + right_min_x_rel;

        // Narrow-layout breakpoints. Essential controls (Home, Save, Play, Present, cloud
        // account and Share) stay reachable. Secondary controls are hidden below these
        // thresholds to keep the two panels within their respective halves.
        let is_narrow = total_w < 720.0; // hide Undo/Redo, Comments, Dark toggle
        let is_compact = total_w < 500.0; // additionally hide Search (its shortcut remains)

        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(rect.min.x + left_offset, rect.min.y), pos2(left_max_x, rect.max.y)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        child.spacing_mut().item_spacing.x = 2.0;
        let has_doc = app.session.active().is_some();
        let dirty = app.session.active().is_some_and(|d| d.is_dirty());
        crate::qat::show(app, &mut child);
        let title =
            app.session.active().map(|d| format!("{}{}", d.title(), if dirty { " — Edited" } else { "" })).unwrap_or_else(|| "DeckCraft".into());
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, title, theme::font(13.0), t.text);
        // Right side.
        let mut right = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(pos2(right_min_x, rect.min.y), pos2(rect.max.x - 10.0, rect.max.y)))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        right.spacing_mut().item_spacing.x = 6.0;
        // Who is signed in and who else is here, at the far right, where every office suite puts
        // them. Drawn first because this layout runs right to left.
        crate::cloud::who(app, &mut right);
        if !is_compact && icon_toggle(&mut right, Icon::Search, "Search commands (⇧⌘P)", true, false).clicked() {
            app.palette = Some((String::new(), 0));
        }
        let present = right.add(
            egui::Button::new(egui::RichText::new("  Present  ").color(t.accent_text).font(theme::bold(13.0)))
                .fill(t.accent)
                .corner_radius(control_h / 2.0)
                .min_size(vec2(84.0, control_h)),
        );
        if present.clicked() && has_doc {
            let from = app.session.active().map(|d| d.selection.slide).unwrap_or(0);
            app.start_show(from, false);
        }
        // Share means share when there is somewhere to share to. The dialog itself lives in
        // craft-ui, so every Craft app offers the same one. Without a service that reports
        // sharing nothing is drawn here, rather than a button that fails on use.
        if crate::cloud::shares(app) {
            // A split button, drawn by craft-ui so every Craft app offers the same one. Only the
            // Save a Copy formats under it are DeckCraft's own.
            let copies = crate::cloud::copies();
            if let Some(fmt) = crate::cloud::share_button(app, &mut right, &copies) {
                app.export_as(&fmt, json!({}));
            }
        }
        if !is_narrow {
            if icon_toggle(&mut right, Icon::CommentsBubble, "Comments", has_doc, app.ui.pane.as_deref() == Some("comments")).clicked() {
                let _ = app.run("view.pane", json!({"pane": "comments", "toggle": true}));
            }
            if icon_toggle(
                &mut right,
                if app.ui.brightness == theme::Brightness::Dark { Icon::Sparkle } else { Icon::Palette },
                "Light/Dark appearance",
                true,
                false,
            )
            .clicked()
            {
                let _ = app.run("view.dark", json!({}));
            }
        }
    });
}

/// Computes the non-overlapping title-bar panel bounds for a given total bar width.
///
/// `total_w` is the full bar width; `left_offset` is the space reserved at the left edge
/// (traffic-light / macOS: 84 px; standard: 10 px). Returns `(left_max_x, right_min_x)` as
/// offsets from the bar's own origin (`rect.min.x = 0`), matching the coordinate system used
/// to build the child panel `Rect`s.
///
/// Called by both `title_bar` and the layout tests so the geometry is tested against the same
/// formula that ships.
pub(crate) fn title_bar_panel_bounds(total_w: f32, left_offset: f32) -> (f32, f32) {
    // Maximum reach positions, measured from rect.min.x = 0, preserving the original
    // fixed values (left: 420, right-start: total_w - 560).
    const LEFT_REACH: f32 = 420.0;
    const RIGHT_REACH: f32 = 560.0;
    const CENTRE_GAP: f32 = 4.0;
    let center_x = total_w / 2.0;
    let left_max_x = LEFT_REACH.min(center_x - CENTRE_GAP).max(left_offset);
    let right_min_x = (total_w - RIGHT_REACH).max(center_x + CENTRE_GAP).min(total_w - 10.0);
    (left_max_x, right_min_x)
}

#[cfg(test)]
#[path = "../ribbon_tests.rs"]
mod layout_tests;
