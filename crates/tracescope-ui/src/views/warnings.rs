//! Warning list view.

use eframe::egui;
use egui_extras::{Column, TableBuilder};

use crate::app::TraceScopeApp;

/// Renders the warning list.
pub fn render(ui: &mut egui::Ui, app: &mut TraceScopeApp) {
    ui.heading("Warnings");
    ui.label("Derived async lint-style warnings for observed tasks.");
    ui.add_space(8.0);

    let warnings = app.warnings();

    TableBuilder::new(ui)
        .striped(true)
        .column(Column::initial(70.0))
        .column(Column::initial(160.0))
        .column(Column::initial(100.0))
        .column(Column::remainder())
        .header(24.0, |mut header| {
            header.col(|ui| {
                ui.label("Task ID");
            });
            header.col(|ui| {
                ui.label("Task");
            });
            header.col(|ui| {
                ui.label("Kind");
            });
            header.col(|ui| {
                ui.label("Message");
            });
        })
        .body(|body| {
            body.rows(22.0, warnings.len(), |mut row| {
                let warning = &warnings[row.index()];
                row.col(|ui| {
                    ui.label(warning.task_id.0.to_string());
                });
                row.col(|ui| {
                    ui.label(&warning.task_name);
                });
                row.col(|ui| {
                    ui.label(format!("{:?}", warning.kind));
                });
                row.col(|ui| {
                    ui.label(&warning.message);
                });
            });
        });

    if warnings.is_empty() {
        ui.add_space(8.0);
        ui.label("No warnings detected.");
    }
}
