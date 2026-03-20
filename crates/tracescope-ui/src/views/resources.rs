//! Resource table view.

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use tracescope_core::query::ResourceSortColumn;

use crate::app::TraceScopeApp;

/// Renders the resources view.
pub fn render(ui: &mut egui::Ui, app: &mut TraceScopeApp) {
    ui.heading("Resources");
    ui.label("Observed runtime resources and poll activity.");
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.label("Filter");
        ui.text_edit_singleline(&mut app.resource_query.filter);
    });

    let resources = app.queried_resources();
    ui.add_space(8.0);

    TableBuilder::new(ui)
        .striped(true)
        .column(Column::initial(70.0))
        .column(Column::remainder())
        .column(Column::initial(120.0))
        .column(Column::initial(80.0))
        .column(Column::initial(70.0))
        .column(Column::initial(80.0))
        .header(24.0, |mut header| {
            sort_header(&mut header, "ID", ResourceSortColumn::Id, app);
            sort_header(&mut header, "Name", ResourceSortColumn::Name, app);
            sort_header(&mut header, "Kind", ResourceSortColumn::Kind, app);
            sort_header(&mut header, "Poll Ops", ResourceSortColumn::PollOps, app);
            sort_header(&mut header, "Ready", ResourceSortColumn::Ready, app);
            sort_header(&mut header, "Pending", ResourceSortColumn::Pending, app);
        })
        .body(|body| {
            body.rows(22.0, resources.len(), |mut row| {
                let resource = &resources[row.index()];
                row.col(|ui| {
                    ui.label(resource.id.0.to_string());
                });
                row.col(|ui| {
                    ui.label(&resource.name);
                });
                row.col(|ui| {
                    ui.label(&resource.kind);
                });
                row.col(|ui| {
                    ui.label(resource.stats.poll_op_count.to_string());
                });
                row.col(|ui| {
                    ui.label(resource.stats.ready_count.to_string());
                });
                row.col(|ui| {
                    ui.label(resource.stats.pending_count.to_string());
                });
            });
        });
}

fn sort_header(
    header: &mut egui_extras::TableRow<'_, '_>,
    label: &str,
    column: ResourceSortColumn,
    app: &mut TraceScopeApp,
) {
    header.col(|ui| {
        if ui.small_button(label).clicked() {
            app.set_resource_sort(column);
        }
    });
}
