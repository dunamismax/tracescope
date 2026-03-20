//! Main `eframe` application state for TraceScope.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};

use chrono::{Local, Utc};
use eframe::egui;
use tracescope_core::{
    query::{
        filter_sessions, query_resources, query_tasks, ResourceQuery, ResourceSortColumn,
        TaskQuery, TaskSortColumn,
    },
    CollectorCommand, CollectorEvent, CollectorSnapshot, ConnectionState, LoadedSession, Session,
    SessionDraft, SessionId, SessionStore, StoreError, WarningRecord,
};

use crate::{views, widgets};

/// Construction-time configuration for the TraceScope desktop app.
pub struct TraceScopeAppConfig {
    /// Initial target address displayed in the connection view.
    pub initial_target_address: String,
    /// Data directory used for local persistence.
    pub data_dir: PathBuf,
    /// Command channel used to control the collector manager.
    pub command_tx: Sender<CollectorCommand>,
    /// Event channel used to receive collector updates.
    pub event_rx: Receiver<CollectorEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavigationView {
    Connection,
    Tasks,
    Timeline,
    Resources,
    Sessions,
    Warnings,
}

impl NavigationView {
    fn label(self) -> &'static str {
        match self {
            Self::Connection => "Connection",
            Self::Tasks => "Tasks",
            Self::Timeline => "Timeline",
            Self::Resources => "Resources",
            Self::Sessions => "Sessions",
            Self::Warnings => "Warnings",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RecordingState {
    pub(crate) started_at: chrono::DateTime<Utc>,
}

/// Main desktop application state.
pub struct TraceScopeApp {
    pub(crate) connection_target: String,
    pub(crate) connection_state: ConnectionState,
    pub(crate) command_tx: Sender<CollectorCommand>,
    pub(crate) event_rx: Receiver<CollectorEvent>,
    pub(crate) snapshot: CollectorSnapshot,
    pub(crate) sessions: Vec<Session>,
    pub(crate) selected_session_id: Option<SessionId>,
    pub(crate) loaded_session_name: Option<String>,
    pub(crate) recording: Option<RecordingState>,
    pub(crate) task_query: TaskQuery,
    pub(crate) resource_query: ResourceQuery,
    pub(crate) session_filter: String,
    pub(crate) current_view: NavigationView,
    pub(crate) last_message: Option<String>,
    pub(crate) store: Option<SessionStore>,
    pub(crate) store_error: Option<String>,
}

impl TraceScopeApp {
    /// Creates a new app instance.
    #[must_use]
    pub fn new(config: TraceScopeAppConfig) -> Self {
        let mut store_error = None;
        let store = match SessionStore::open_in_dir(&config.data_dir) {
            Ok(store) => Some(store),
            Err(error) => {
                store_error = Some(error.to_string());
                None
            }
        };

        let mut app = Self {
            connection_target: config.initial_target_address.clone(),
            connection_state: ConnectionState::Disconnected,
            command_tx: config.command_tx,
            event_rx: config.event_rx,
            snapshot: CollectorSnapshot::empty(config.initial_target_address),
            sessions: Vec::new(),
            selected_session_id: None,
            loaded_session_name: None,
            recording: None,
            task_query: TaskQuery::default(),
            resource_query: ResourceQuery::default(),
            session_filter: String::new(),
            current_view: NavigationView::Connection,
            last_message: None,
            store,
            store_error,
        };

        app.refresh_sessions();
        app
    }

    pub(crate) fn poll_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                CollectorEvent::Status(status) => {
                    self.connection_state = status;
                    if matches!(self.connection_state, ConnectionState::Disconnected) {
                        self.loaded_session_name = None;
                    }
                }
                CollectorEvent::Snapshot(snapshot) => {
                    self.snapshot = snapshot;
                    self.loaded_session_name = None;
                }
            }
        }
    }

    pub(crate) fn connect(&mut self) {
        let target_address = self.connection_target.trim().to_string();
        if target_address.is_empty() {
            self.last_message = Some(String::from("enter a target address before connecting"));
            return;
        }

        if self
            .command_tx
            .send(CollectorCommand::Connect {
                target_address: target_address.clone(),
            })
            .is_err()
        {
            self.last_message = Some(String::from("collector manager is unavailable"));
            self.connection_state = ConnectionState::Error {
                target_address,
                message: String::from("collector manager disconnected"),
            };
            return;
        }

        self.connection_state = ConnectionState::Connecting { target_address };
    }

    pub(crate) fn disconnect(&mut self) {
        if self.command_tx.send(CollectorCommand::Disconnect).is_err() {
            self.last_message = Some(String::from("collector manager is unavailable"));
        }
    }

    pub(crate) fn refresh_sessions(&mut self) {
        match self.store.as_ref().map(SessionStore::list_sessions) {
            Some(Ok(sessions)) => self.sessions = sessions,
            Some(Err(error)) => self.last_message = Some(error.to_string()),
            None => {}
        }
    }

    pub(crate) fn start_recording(&mut self) {
        self.recording = Some(RecordingState {
            started_at: Utc::now(),
        });
        self.last_message = Some(String::from("recording started"));
    }

    pub(crate) fn stop_recording(&mut self) {
        let Some(recording) = self.recording.take() else {
            return;
        };

        match self.persist_recording(recording.started_at) {
            Ok(session_id) => {
                self.refresh_sessions();
                self.selected_session_id = Some(session_id);
                self.last_message = Some(format!("saved recording as session {}", session_id.0));
            }
            Err(error) => {
                self.last_message = Some(error.to_string());
            }
        }
    }

    pub(crate) fn load_selected_session(&mut self) {
        let Some(session_id) = self.selected_session_id else {
            return;
        };

        match self
            .store
            .as_ref()
            .ok_or_else(|| StoreError::HomeDirUnavailable)
            .and_then(|store| store.load_session(session_id))
        {
            Ok(session) => {
                self.snapshot = snapshot_from_loaded_session(&session);
                self.loaded_session_name = Some(session.session.name);
                self.connection_state = ConnectionState::Disconnected;
            }
            Err(error) => self.last_message = Some(error.to_string()),
        }
    }

    pub(crate) fn delete_selected_session(&mut self) {
        let Some(session_id) = self.selected_session_id else {
            return;
        };

        match self
            .store
            .as_ref()
            .ok_or_else(|| StoreError::HomeDirUnavailable)
            .and_then(|store| store.delete_session(session_id))
        {
            Ok(()) => {
                self.refresh_sessions();
                self.selected_session_id = None;
                self.last_message = Some(String::from("deleted session"));
            }
            Err(error) => self.last_message = Some(error.to_string()),
        }
    }

    pub(crate) fn set_task_sort(&mut self, column: TaskSortColumn) {
        if self.task_query.sort_by == column {
            self.task_query.descending = !self.task_query.descending;
        } else {
            self.task_query.sort_by = column;
            self.task_query.descending = !matches!(
                column,
                TaskSortColumn::Id | TaskSortColumn::Name | TaskSortColumn::State
            );
        }
    }

    pub(crate) fn set_resource_sort(&mut self, column: ResourceSortColumn) {
        if self.resource_query.sort_by == column {
            self.resource_query.descending = !self.resource_query.descending;
        } else {
            self.resource_query.sort_by = column;
            self.resource_query.descending = !matches!(
                column,
                ResourceSortColumn::Id | ResourceSortColumn::Name | ResourceSortColumn::Kind
            );
        }
    }

    pub(crate) fn filtered_sessions(&self) -> Vec<Session> {
        filter_sessions(&self.sessions, &self.session_filter)
    }

    pub(crate) fn queried_tasks(&self) -> Vec<tracescope_core::Task> {
        query_tasks(&self.snapshot.tasks, &self.task_query)
    }

    pub(crate) fn queried_resources(&self) -> Vec<tracescope_core::Resource> {
        query_resources(&self.snapshot.resources, &self.resource_query)
    }

    pub(crate) fn warnings(&self) -> &[WarningRecord] {
        &self.snapshot.warnings
    }

    pub(crate) fn connection_label(&self) -> String {
        match &self.connection_state {
            ConnectionState::Disconnected => String::from("Disconnected"),
            ConnectionState::Connecting { target_address } => {
                format!("Connecting to {target_address}")
            }
            ConnectionState::Connected { target_address } => {
                format!("Connected to {target_address}")
            }
            ConnectionState::Error {
                target_address,
                message,
            } => {
                format!("Error talking to {target_address}: {message}")
            }
        }
    }

    fn persist_recording(
        &self,
        started_at: chrono::DateTime<Utc>,
    ) -> Result<SessionId, StoreError> {
        let Some(store) = &self.store else {
            return Err(StoreError::HomeDirUnavailable);
        };

        let ended_at = Some(Utc::now());
        let session_name = format!("Recording {}", Local::now().format("%Y-%m-%d %H:%M:%S"));
        let metadata = BTreeMap::from([
            (
                String::from("task_count"),
                self.snapshot.tasks.len().to_string(),
            ),
            (
                String::from("span_count"),
                self.snapshot.spans.len().to_string(),
            ),
            (
                String::from("resource_count"),
                self.snapshot.resources.len().to_string(),
            ),
        ]);

        let session_id = store.save_session_snapshot(
            &SessionDraft {
                name: session_name,
                target_address: self.snapshot.target_address.clone(),
                started_at,
                ended_at,
                metadata,
            },
            &self.snapshot.tasks,
            &self.snapshot.spans,
            &self.snapshot.resources,
        )?;
        Ok(session_id)
    }
}

impl eframe::App for TraceScopeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events();
        ctx.request_repaint_after(Duration::from_millis(100));

        egui::SidePanel::left("navigation")
            .resizable(false)
            .default_width(170.0)
            .show(ctx, |ui| {
                ui.heading("TraceScope");
                ui.label("Graphical async tracing recorder");
                ui.separator();

                for view in [
                    NavigationView::Connection,
                    NavigationView::Tasks,
                    NavigationView::Timeline,
                    NavigationView::Resources,
                    NavigationView::Sessions,
                    NavigationView::Warnings,
                ] {
                    if ui
                        .selectable_label(self.current_view == view, view.label())
                        .clicked()
                    {
                        self.current_view = view;
                    }
                }
            });

        egui::TopBottomPanel::bottom("status_bar")
            .resizable(false)
            .show(ctx, |ui| widgets::status_bar::render(ui, self));

        egui::CentralPanel::default().show(ctx, |ui| match self.current_view {
            NavigationView::Connection => views::connection::render(ui, self),
            NavigationView::Tasks => views::tasks::render(ui, self),
            NavigationView::Timeline => views::timeline::render(ui, self),
            NavigationView::Resources => views::resources::render(ui, self),
            NavigationView::Sessions => views::sessions::render(ui, self),
            NavigationView::Warnings => views::warnings::render(ui, self),
        });
    }
}

fn snapshot_from_loaded_session(session: &LoadedSession) -> CollectorSnapshot {
    let warnings = session
        .tasks
        .iter()
        .flat_map(tracescope_core::Task::warning_records)
        .collect::<Vec<_>>();

    CollectorSnapshot {
        target_address: session.session.target_address.clone(),
        connected: false,
        tasks: session.tasks.clone(),
        spans: session.spans.clone(),
        resources: session.resources.clone(),
        warnings,
        updated_at: session
            .session
            .ended_at
            .or(Some(session.session.started_at)),
    }
}
