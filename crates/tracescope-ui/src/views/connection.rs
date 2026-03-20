//! Connection view.

use eframe::egui;

use crate::app::TraceScopeApp;

/// Renders the connection panel.
pub fn render(ui: &mut egui::Ui, app: &mut TraceScopeApp) {
    ui.heading("Connection");
    ui.label("Connect TraceScope to a Tokio console endpoint.");
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.label("Target");
        ui.text_edit_singleline(&mut app.connection_target);
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Connect").clicked() {
            app.connect();
        }

        if ui.button("Disconnect").clicked() {
            app.disconnect();
        }
    });

    ui.add_space(12.0);
    ui.group(|ui| {
        ui.label(app.connection_label());
        if let Some(updated_at) = app.snapshot.updated_at {
            ui.label(format!("Last update: {}", updated_at.to_rfc3339()));
        }
        ui.label(format!(
            "Current counts: {} tasks, {} spans, {} resources",
            app.snapshot.tasks.len(),
            app.snapshot.spans.len(),
            app.snapshot.resources.len()
        ));
    });

    if let Some(message) = &app.last_message {
        ui.add_space(8.0);
        ui.label(message);
    }
}
