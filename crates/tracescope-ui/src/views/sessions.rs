//! Saved session browser and recording controls.

use eframe::egui;

use crate::app::TraceScopeApp;

/// Renders the sessions view.
pub fn render(ui: &mut egui::Ui, app: &mut TraceScopeApp) {
    ui.heading("Sessions");
    ui.label("Browse saved sessions and control recording.");
    ui.add_space(8.0);
    ui.label(match app.recording_blocked_reason() {
        Some(reason) if app.recording.is_none() => format!("Recording unavailable: {reason}"),
        _ if app.recording.is_some() => {
            String::from("Recording a live snapshot for the current connection.")
        }
        _ => String::from("Recording is available while connected to live telemetry."),
    });
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        let is_recording = app.recording.is_some();
        let button_label = if is_recording {
            "Stop Recording"
        } else {
            "Start Recording"
        };
        ui.add_enabled_ui(
            app.recording.is_some() || app.recording_blocked_reason().is_none(),
            |ui| {
                if ui.button(button_label).clicked() {
                    if is_recording {
                        app.stop_recording();
                    } else {
                        app.start_recording();
                    }
                }
            },
        );

        if ui.button("Refresh").clicked() {
            app.refresh_sessions();
        }
        ui.add_enabled_ui(app.recording.is_none(), |ui| {
            if ui.button("Load Selected").clicked() {
                app.load_selected_session();
            }
            if ui.button("Delete Selected").clicked() {
                app.delete_selected_session();
            }
        });
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Filter");
        ui.text_edit_singleline(&mut app.session_filter);
    });

    ui.add_space(8.0);
    let filtered_sessions = app.filtered_sessions();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for session in filtered_sessions {
            let selected = app.selected_session_id == Some(session.id);
            if ui
                .selectable_label(
                    selected,
                    format!(
                        "{}  |  {}  |  {}",
                        session.name,
                        session.started_at.format("%Y-%m-%d %H:%M:%S"),
                        session.target_address
                    ),
                )
                .clicked()
            {
                app.selected_session_id = Some(session.id);
            }
        }

        if app.sessions.is_empty() {
            ui.label("No saved sessions yet.");
        } else if !app.session_filter.trim().is_empty() {
            ui.label("No sessions match the current filter.");
        }
    });

    if let Some(message) = &app.last_message {
        ui.add_space(8.0);
        ui.label(message);
    }
}
