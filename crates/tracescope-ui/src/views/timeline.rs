//! Simplified timeline view.

use eframe::egui;

use crate::app::TraceScopeApp;

/// Renders the Phase 1 simplified timeline.
pub fn render(ui: &mut egui::Ui, app: &mut TraceScopeApp) {
    ui.heading("Timeline");
    ui.label("Simplified span timeline with proportional bars.");
    ui.add_space(8.0);

    let spans = &app.snapshot.spans;
    let max_duration = spans
        .iter()
        .map(|span| span.busy_duration.as_millis())
        .max()
        .unwrap_or(1)
        .max(1);

    egui::ScrollArea::vertical().show(ui, |ui| {
        for span in spans {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(220.0, 16.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.label(format!("{} ({})", span.name, span.level));
                    },
                );

                let width = ((span.busy_duration.as_millis() as f32 / max_duration as f32) * 320.0)
                    .clamp(4.0, 320.0);
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(width, 16.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, 4.0, egui::Color32::from_rgb(86, 144, 255));
                ui.label(format!("{} ms", span.busy_duration.as_millis()));
            });
            ui.add_space(6.0);
        }

        if spans.is_empty() {
            ui.label("No span data received yet.");
        }
    });
}
