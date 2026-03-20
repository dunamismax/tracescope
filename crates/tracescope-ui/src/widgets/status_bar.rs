//! Status bar widget.

use eframe::egui;

use crate::app::TraceScopeApp;

/// Renders the bottom status bar.
pub fn render(ui: &mut egui::Ui, app: &TraceScopeApp) {
    ui.horizontal_wrapped(|ui| {
        ui.label(app.connection_label());
        ui.separator();
        ui.label(if app.recording.is_some() {
            "Recording: on"
        } else {
            "Recording: off"
        });
        ui.separator();
        ui.label(format!(
            "Tasks: {}  Spans: {}  Resources: {}",
            app.snapshot.tasks.len(),
            app.snapshot.spans.len(),
            app.snapshot.resources.len()
        ));
        if let Some(name) = &app.loaded_session_name {
            ui.separator();
            ui.label(format!("Loaded session: {name}"));
        }
        if let Some(error) = &app.store_error {
            ui.separator();
            ui.label(format!("Store unavailable: {error}"));
        }
    });
}
