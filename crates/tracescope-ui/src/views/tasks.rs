//! Task table view.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use tracescope_core::query::TaskSortColumn;

use crate::app::TraceScopeApp;

/// Renders the tasks view.
pub fn render(ui: &mut egui::Ui, app: &mut TraceScopeApp) {
    ui.heading("Tasks");
    ui.label("Live and recorded task data with sorting and filtering.");
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.label("Filter");
        ui.text_edit_singleline(&mut app.task_query.filter);
    });

    let tasks = app.queried_tasks();
    ui.add_space(8.0);

    TableBuilder::new(ui)
        .striped(true)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(70.0))
        .column(Column::remainder())
        .column(Column::initial(90.0))
        .column(Column::initial(70.0))
        .column(Column::initial(70.0))
        .column(Column::initial(90.0))
        .column(Column::initial(70.0))
        .column(Column::initial(60.0))
        .column(Column::initial(80.0))
        .header(24.0, |mut header| {
            sort_header(&mut header, "ID", TaskSortColumn::Id, app);
            sort_header(&mut header, "Name", TaskSortColumn::Name, app);
            sort_header(&mut header, "State", TaskSortColumn::State, app);
            sort_header(&mut header, "Total", TaskSortColumn::Total, app);
            sort_header(&mut header, "Busy", TaskSortColumn::Busy, app);
            sort_header(&mut header, "Scheduled", TaskSortColumn::Scheduled, app);
            sort_header(&mut header, "Idle", TaskSortColumn::Idle, app);
            sort_header(&mut header, "Polls", TaskSortColumn::Polls, app);
            sort_header(&mut header, "Warnings", TaskSortColumn::Warnings, app);
        })
        .body(|body| {
            body.rows(22.0, tasks.len(), |mut row| {
                let task = &tasks[row.index()];
                row.col(|ui| {
                    ui.label(task.id.0.to_string());
                });
                row.col(|ui| {
                    ui.label(&task.name);
                });
                row.col(|ui| {
                    ui.label(task.state.to_string());
                });
                row.col(|ui| {
                    ui.label(format!("{} ms", task.stats.total_duration.as_millis()));
                });
                row.col(|ui| {
                    ui.label(format!("{} ms", task.stats.busy_duration.as_millis()));
                });
                row.col(|ui| {
                    ui.label(format!("{} ms", task.stats.scheduled_duration.as_millis()));
                });
                row.col(|ui| {
                    ui.label(format!("{} ms", task.stats.idle_duration.as_millis()));
                });
                row.col(|ui| {
                    ui.label(task.stats.poll_count.to_string());
                });
                row.col(|ui| {
                    ui.label(task.warnings.len().to_string());
                });
            });
        });
}

fn sort_header(
    header: &mut egui_extras::TableRow<'_, '_>,
    label: &str,
    column: TaskSortColumn,
    app: &mut TraceScopeApp,
) {
    header.col(|ui| {
        if ui.small_button(label).clicked() {
            app.set_task_sort(column);
        }
    });
}
