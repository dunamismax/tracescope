//! gRPC collector for Tokio console telemetry.

use std::{
    collections::{BTreeMap, HashMap},
    future::pending,
    sync::mpsc::Sender,
};

use chrono::{DateTime, Utc};
use console_api::{
    field,
    instrument::{instrument_client::InstrumentClient, InstrumentRequest, Update},
    metadata, resources, tasks,
    trace::{self, trace_client::TraceClient},
    Attribute, Field, MetaId, Metadata, RegisterMetadata, Span as ApiSpan,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tonic::transport::{Channel, Endpoint};
use tracing::{debug, warn};

use crate::model::{
    DurationValue, FieldValue, Resource, ResourceId, ResourceStats, ResourceVisibility, Span,
    SpanId, SpanLevel, Task, TaskId, TaskState, TaskStats, WarningRecord,
};

/// Snapshot emitted by the collector for UI consumption.
#[derive(Debug, Clone)]
pub struct CollectorSnapshot {
    /// Target currently being observed.
    pub target_address: String,
    /// Whether the collector is currently connected.
    pub connected: bool,
    /// Latest tasks.
    pub tasks: Vec<Task>,
    /// Latest spans.
    pub spans: Vec<Span>,
    /// Latest resources.
    pub resources: Vec<Resource>,
    /// Flattened warning list.
    pub warnings: Vec<WarningRecord>,
    /// Latest update timestamp.
    pub updated_at: Option<DateTime<Utc>>,
}

impl CollectorSnapshot {
    /// Creates an empty snapshot for a target.
    #[must_use]
    pub fn empty(target_address: impl Into<String>) -> Self {
        Self {
            target_address: target_address.into(),
            connected: false,
            tasks: Vec::new(),
            spans: Vec::new(),
            resources: Vec::new(),
            warnings: Vec::new(),
            updated_at: None,
        }
    }
}

/// Connection state surfaced to the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    /// No active connection.
    Disconnected,
    /// Connection is being established.
    Connecting { target_address: String },
    /// Collector is streaming updates.
    Connected { target_address: String },
    /// Connection failed or ended unexpectedly.
    Error {
        target_address: String,
        message: String,
    },
}

/// Command sent from the sync UI to the collector manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectorCommand {
    /// Connect to a target address.
    Connect { target_address: String },
    /// Stop the active collector, if any.
    Disconnect,
}

/// Event emitted by the collector to the UI.
#[derive(Debug, Clone)]
pub enum CollectorEvent {
    /// Connection status transition.
    Status(ConnectionState),
    /// Snapshot update.
    Snapshot(CollectorSnapshot),
}

/// Errors returned by the collector.
#[derive(Debug, Error)]
pub enum CollectorError {
    /// The configured target could not be parsed as a tonic endpoint.
    #[error("invalid collector target `{target}`: {source}")]
    InvalidEndpoint {
        /// Invalid target string.
        target: String,
        /// Underlying tonic error.
        source: tonic::transport::Error,
    },
    /// Initial connection failed.
    #[error("failed to connect to `{target}`: {source}")]
    Transport {
        /// Target string.
        target: String,
        /// Underlying tonic error.
        source: tonic::transport::Error,
    },
    /// A gRPC request failed.
    #[error("grpc request failed for `{target}`: {source}")]
    Status {
        /// Target string.
        target: String,
        /// Underlying gRPC status.
        source: tonic::Status,
    },
}

/// Live collector that streams Tokio console telemetry.
#[derive(Debug, Clone)]
pub struct Collector {
    target_address: String,
}

impl Collector {
    /// Creates a collector for a target address.
    #[must_use]
    pub fn new(target_address: impl Into<String>) -> Self {
        Self {
            target_address: normalize_target(&target_address.into()),
        }
    }

    /// Returns the normalized target address.
    #[must_use]
    pub fn target_address(&self) -> &str {
        &self.target_address
    }

    /// Connects to the target and streams updates until shutdown or disconnect.
    pub async fn run(
        &self,
        event_tx: Sender<CollectorEvent>,
        shutdown: CancellationToken,
    ) -> Result<(), CollectorError> {
        let endpoint = Endpoint::from_shared(self.target_address.clone()).map_err(|source| {
            CollectorError::InvalidEndpoint {
                target: self.target_address.clone(),
                source,
            }
        })?;

        let channel = endpoint
            .connect()
            .await
            .map_err(|source| CollectorError::Transport {
                target: self.target_address.clone(),
                source,
            })?;

        let mut state = CollectorState::new(self.target_address.clone());
        let mut instrument = InstrumentClient::new(channel.clone());
        let mut updates = instrument
            .watch_updates(InstrumentRequest {})
            .await
            .map_err(|source| CollectorError::Status {
                target: self.target_address.clone(),
                source,
            })?
            .into_inner();

        let mut trace_stream = connect_trace_stream(self.target_address.clone(), channel)
            .await
            .map_err(|source| CollectorError::Status {
                target: self.target_address.clone(),
                source,
            })?;

        send_event(
            &event_tx,
            CollectorEvent::Status(ConnectionState::Connected {
                target_address: self.target_address.clone(),
            }),
        );

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    debug!("collector shutdown requested");
                    break;
                }
                update = updates.message() => {
                    match update.map_err(|source| CollectorError::Status {
                        target: self.target_address.clone(),
                        source,
                    })? {
                        Some(update) => {
                            state.apply_update(update);
                            send_event(&event_tx, CollectorEvent::Snapshot(state.snapshot()));
                        }
                        None => break,
                    }
                }
                trace_event = async {
                    match trace_stream.as_mut() {
                        Some(stream) => stream.message().await,
                        None => pending::<Result<Option<trace::TraceEvent>, tonic::Status>>().await,
                    }
                } => {
                    match trace_event.map_err(|source| CollectorError::Status {
                        target: self.target_address.clone(),
                        source,
                    })? {
                        Some(event) => {
                            state.apply_trace_event(event);
                            send_event(&event_tx, CollectorEvent::Snapshot(state.snapshot()));
                        }
                        None => trace_stream = None,
                    }
                }
            }
        }

        send_event(
            &event_tx,
            CollectorEvent::Status(ConnectionState::Disconnected),
        );
        Ok(())
    }
}

async fn connect_trace_stream(
    target_address: String,
    channel: Channel,
) -> Result<Option<tonic::codec::Streaming<trace::TraceEvent>>, tonic::Status> {
    let mut client = TraceClient::new(channel);
    match client
        .watch(trace::WatchRequest {
            filter: String::new(),
        })
        .await
    {
        Ok(response) => Ok(Some(response.into_inner())),
        Err(error) => {
            warn!(target = %target_address, %error, "trace stream unavailable, continuing with task/resource updates");
            if error.code() == tonic::Code::Unimplemented {
                Ok(None)
            } else {
                Err(error)
            }
        }
    }
}

fn send_event(event_tx: &Sender<CollectorEvent>, event: CollectorEvent) {
    let _ = event_tx.send(event);
}

#[derive(Debug, Default)]
struct MetadataCatalog {
    entries: HashMap<u64, Metadata>,
}

impl MetadataCatalog {
    fn insert_batch(&mut self, metadata: Option<RegisterMetadata>) {
        if let Some(metadata) = metadata {
            for entry in metadata.metadata {
                if let (Some(id), Some(payload)) = (entry.id, entry.metadata) {
                    self.entries.insert(id.id, payload);
                }
            }
        }
    }

    fn get(&self, meta_id: Option<MetaId>) -> Option<&Metadata> {
        meta_id.and_then(|meta_id| self.entries.get(&meta_id.id))
    }
}

#[derive(Debug)]
struct CollectorState {
    target_address: String,
    metadata: MetadataCatalog,
    tasks: HashMap<TaskId, Task>,
    spans: HashMap<SpanId, Span>,
    resources: HashMap<ResourceId, Resource>,
    active_spans: HashMap<SpanId, DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
}

impl CollectorState {
    fn new(target_address: String) -> Self {
        Self {
            target_address,
            metadata: MetadataCatalog::default(),
            tasks: HashMap::new(),
            spans: HashMap::new(),
            resources: HashMap::new(),
            active_spans: HashMap::new(),
            updated_at: None,
        }
    }

    fn apply_update(&mut self, update: Update) {
        self.updated_at = timestamp_to_datetime(update.now);
        self.metadata.insert_batch(update.new_metadata);

        if let Some(task_update) = update.task_update {
            self.apply_task_update(task_update);
        }

        if let Some(resource_update) = update.resource_update {
            self.apply_resource_update(resource_update);
        }
    }

    fn apply_trace_event(&mut self, event: trace::TraceEvent) {
        use trace::trace_event::Event;

        match event.event {
            Some(Event::RegisterMetadata(metadata)) => self.metadata.insert_batch(Some(metadata)),
            Some(Event::NewSpan(span)) => self.register_span(span),
            Some(Event::EnterSpan(enter)) => {
                if let (Some(span_id), Some(at)) = (enter.span_id, timestamp_to_datetime(enter.at))
                {
                    let span_id = SpanId(span_id.id);
                    let span = self.spans.entry(span_id).or_insert_with(|| Span {
                        id: span_id,
                        parent_id: None,
                        name: format!("span-{}", span_id.0),
                        target: String::from("unknown"),
                        level: SpanLevel::Info,
                        fields: BTreeMap::new(),
                        entered_at: Some(at),
                        exited_at: None,
                        busy_duration: DurationValue::default(),
                    });
                    span.entered_at.get_or_insert(at);
                    self.active_spans.insert(span_id, at);
                }
            }
            Some(Event::ExitSpan(exit)) => {
                if let (Some(span_id), Some(at)) = (exit.span_id, timestamp_to_datetime(exit.at)) {
                    let span_id = SpanId(span_id.id);
                    if let Some(span) = self.spans.get_mut(&span_id) {
                        span.exited_at = Some(at);
                        if let Some(started_at) = self.active_spans.remove(&span_id) {
                            span.busy_duration = span
                                .busy_duration
                                .saturating_add(duration_between(started_at, at));
                        }
                    }
                }
            }
            Some(Event::CloseSpan(close)) => {
                if let (Some(span_id), Some(at)) = (close.span_id, timestamp_to_datetime(close.at))
                {
                    if let Some(span) = self.spans.get_mut(&SpanId(span_id.id)) {
                        span.exited_at.get_or_insert(at);
                    }
                }
            }
            Some(Event::RegisterThread(_)) | None => {}
        }
    }

    fn apply_task_update(&mut self, update: tasks::TaskUpdate) {
        for new_task in update.new_tasks {
            let Some(id) = new_task.id.map(|value| TaskId(value.id)) else {
                continue;
            };

            let metadata = self.metadata.get(new_task.metadata);
            let task = self.tasks.entry(id).or_insert_with(|| Task {
                id,
                name: metadata
                    .map(|value| value.name.clone())
                    .unwrap_or_else(|| format!("task-{id:?}")),
                state: TaskState::Idle,
                fields: decode_fields(metadata, &new_task.fields),
                stats: TaskStats::default(),
                warnings: Vec::new(),
                created_at: None,
                dropped_at: None,
            });

            task.name = metadata
                .map(|value| value.name.clone())
                .unwrap_or_else(|| format!("task-{}", id.0));
            task.fields = decode_fields(metadata, &new_task.fields);
        }

        for (task_id, stats) in update.stats_update {
            let task_id = TaskId(task_id);
            if let Some(task) = self.tasks.get_mut(&task_id) {
                let poll_stats = stats.poll_stats.unwrap_or_default();
                let created_at = timestamp_to_datetime(stats.created_at);
                let dropped_at = timestamp_to_datetime(stats.dropped_at);
                let last_wake = timestamp_to_datetime(stats.last_wake);
                let last_poll_started = timestamp_to_datetime(poll_stats.last_poll_started);
                let last_poll_ended = timestamp_to_datetime(poll_stats.last_poll_ended);
                let busy_duration = duration_from_prost(poll_stats.busy_time);
                let scheduled_duration = duration_from_prost(stats.scheduled_time);
                let ended_at = dropped_at
                    .or(last_poll_ended)
                    .or(self.updated_at)
                    .or(created_at);
                let total_duration = match (created_at, ended_at) {
                    (Some(started_at), Some(ended_at)) => duration_between(started_at, ended_at),
                    _ => DurationValue::default(),
                };
                let idle_duration = total_duration
                    .saturating_sub(busy_duration)
                    .saturating_sub(scheduled_duration);

                task.created_at = created_at;
                task.dropped_at = dropped_at;
                task.stats = TaskStats {
                    poll_count: poll_stats.polls,
                    wake_count: stats.wakes,
                    self_wake_count: stats.self_wakes,
                    busy_duration,
                    scheduled_duration,
                    idle_duration,
                    total_duration,
                };
                task.state =
                    derive_task_state(dropped_at, last_wake, last_poll_started, last_poll_ended);
                task.refresh_warnings();
            }
        }
    }

    fn apply_resource_update(&mut self, update: resources::ResourceUpdate) {
        for new_resource in update.new_resources {
            let Some(id) = new_resource.id.map(|value| ResourceId(value.id)) else {
                continue;
            };

            let metadata = self.metadata.get(new_resource.metadata);
            let kind =
                decode_resource_kind(new_resource.kind.as_ref(), &new_resource.concrete_type);
            let resource = self.resources.entry(id).or_insert_with(|| Resource {
                id,
                kind: kind.clone(),
                name: metadata
                    .map(|value| value.name.clone())
                    .unwrap_or_else(|| new_resource.concrete_type.clone()),
                stats: ResourceStats::default(),
                visibility: if new_resource.is_internal {
                    ResourceVisibility::Internal
                } else {
                    ResourceVisibility::Visible
                },
            });

            resource.kind = kind;
            resource.name = metadata
                .map(|value| value.name.clone())
                .unwrap_or_else(|| new_resource.concrete_type.clone());
            resource.visibility = if new_resource.is_internal {
                ResourceVisibility::Internal
            } else {
                ResourceVisibility::Visible
            };
        }

        for (resource_id, stats) in update.stats_update {
            if let Some(resource) = self.resources.get_mut(&ResourceId(resource_id)) {
                resource.stats.created_at = timestamp_to_datetime(stats.created_at);
                resource.stats.dropped_at = timestamp_to_datetime(stats.dropped_at);
                resource.stats.attributes = decode_attributes(&stats.attributes);
            }
        }

        for poll_op in update.new_poll_ops {
            if let Some(resource_id) = poll_op.resource_id.map(|value| ResourceId(value.id)) {
                if let Some(resource) = self.resources.get_mut(&resource_id) {
                    resource.stats.poll_op_count = resource.stats.poll_op_count.saturating_add(1);
                    if poll_op.is_ready {
                        resource.stats.ready_count = resource.stats.ready_count.saturating_add(1);
                    } else {
                        resource.stats.pending_count =
                            resource.stats.pending_count.saturating_add(1);
                    }
                }
            }
        }
    }

    fn register_span(&mut self, span: ApiSpan) {
        let Some(id) = span.id.map(|value| SpanId(value.id)) else {
            return;
        };

        let metadata = self.metadata.get(span.metadata_id);
        let entry = self.spans.entry(id).or_insert_with(|| Span {
            id,
            parent_id: None,
            name: metadata
                .map(|value| value.name.clone())
                .unwrap_or_else(|| format!("span-{}", id.0)),
            target: metadata
                .map(|value| value.target.clone())
                .unwrap_or_else(|| String::from("unknown")),
            level: metadata
                .map(level_from_metadata)
                .unwrap_or_else(|| SpanLevel::Unknown(String::from("unknown"))),
            fields: decode_fields(metadata, &span.fields),
            entered_at: timestamp_to_datetime(span.at),
            exited_at: None,
            busy_duration: DurationValue::default(),
        });

        entry.fields = decode_fields(metadata, &span.fields);
        if entry.entered_at.is_none() {
            entry.entered_at = timestamp_to_datetime(span.at);
        }
    }

    fn snapshot(&self) -> CollectorSnapshot {
        let mut tasks: Vec<Task> = self.tasks.values().cloned().collect();
        tasks.sort_by_key(|task| task.id.0);

        let mut spans: Vec<Span> = self.spans.values().cloned().collect();
        spans.sort_by_key(|span| span.id.0);

        let mut resources: Vec<Resource> = self.resources.values().cloned().collect();
        resources.sort_by_key(|resource| resource.id.0);

        let warnings = tasks
            .iter()
            .flat_map(Task::warning_records)
            .collect::<Vec<_>>();

        CollectorSnapshot {
            target_address: self.target_address.clone(),
            connected: true,
            tasks,
            spans,
            resources,
            warnings,
            updated_at: self.updated_at,
        }
    }
}

fn normalize_target(input: &str) -> String {
    if input.starts_with("http://") || input.starts_with("https://") {
        input.to_string()
    } else {
        format!("http://{input}")
    }
}

fn timestamp_to_datetime(timestamp: Option<prost_types::Timestamp>) -> Option<DateTime<Utc>> {
    timestamp.and_then(|timestamp| {
        DateTime::<Utc>::from_timestamp(timestamp.seconds, timestamp.nanos as u32)
    })
}

fn duration_from_prost(duration: Option<prost_types::Duration>) -> DurationValue {
    duration
        .and_then(|duration| {
            let seconds = u64::try_from(duration.seconds).ok()?;
            let nanos = u32::try_from(duration.nanos).ok()?;
            Some(DurationValue::from_micros(
                seconds
                    .saturating_mul(1_000_000)
                    .saturating_add(u64::from(nanos / 1_000)),
            ))
        })
        .unwrap_or_default()
}

fn duration_between(started_at: DateTime<Utc>, ended_at: DateTime<Utc>) -> DurationValue {
    let micros = ended_at
        .signed_duration_since(started_at)
        .num_microseconds()
        .unwrap_or_default();
    DurationValue::from_micros(u64::try_from(micros.max(0)).unwrap_or_default())
}

fn decode_fields(metadata: Option<&Metadata>, fields: &[Field]) -> BTreeMap<String, FieldValue> {
    fields
        .iter()
        .filter_map(|field| {
            let key = match &field.name {
                Some(field::Name::StrName(name)) => Some(name.clone()),
                Some(field::Name::NameIdx(index)) => metadata
                    .and_then(|metadata| metadata.field_names.get(*index as usize))
                    .cloned(),
                None => None,
            }?;

            let value = match &field.value {
                Some(field::Value::DebugVal(value)) => FieldValue::Debug(value.clone()),
                Some(field::Value::StrVal(value)) => FieldValue::String(value.clone()),
                Some(field::Value::U64Val(value)) => FieldValue::U64(*value),
                Some(field::Value::I64Val(value)) => FieldValue::I64(*value),
                Some(field::Value::BoolVal(value)) => FieldValue::Bool(*value),
                None => return None,
            };

            Some((key, value))
        })
        .collect()
}

fn decode_attributes(attributes: &[Attribute]) -> BTreeMap<String, String> {
    attributes
        .iter()
        .filter_map(|attribute| {
            let field = attribute.field.as_ref()?;
            let key = match &field.name {
                Some(field::Name::StrName(name)) => name.clone(),
                Some(field::Name::NameIdx(index)) => format!("field_{index}"),
                None => return None,
            };
            let value = match &field.value {
                Some(field::Value::DebugVal(value)) => value.clone(),
                Some(field::Value::StrVal(value)) => value.clone(),
                Some(field::Value::U64Val(value)) => value.to_string(),
                Some(field::Value::I64Val(value)) => value.to_string(),
                Some(field::Value::BoolVal(value)) => value.to_string(),
                None => return None,
            };
            let decorated = attribute
                .unit
                .as_ref()
                .map(|unit| format!("{value} {unit}"))
                .unwrap_or(value);
            Some((key, decorated))
        })
        .collect()
}

fn decode_resource_kind(kind: Option<&resources::resource::Kind>, fallback: &str) -> String {
    kind.and_then(|kind| match &kind.kind {
        Some(resources::resource::kind::Kind::Known(value)) => {
            resources::resource::kind::Known::try_from(*value)
                .ok()
                .map(|value| value.as_str_name().to_ascii_lowercase())
        }
        Some(resources::resource::kind::Kind::Other(value)) => Some(value.clone()),
        None => None,
    })
    .unwrap_or_else(|| fallback.to_string())
}

fn level_from_metadata(metadata: &Metadata) -> SpanLevel {
    match metadata::Level::try_from(metadata.level).ok() {
        Some(metadata::Level::Error) => SpanLevel::Error,
        Some(metadata::Level::Warn) => SpanLevel::Warn,
        Some(metadata::Level::Info) => SpanLevel::Info,
        Some(metadata::Level::Debug) => SpanLevel::Debug,
        Some(metadata::Level::Trace) => SpanLevel::Trace,
        None => SpanLevel::Unknown(metadata.level.to_string()),
    }
}

fn derive_task_state(
    dropped_at: Option<DateTime<Utc>>,
    last_wake: Option<DateTime<Utc>>,
    last_poll_started: Option<DateTime<Utc>>,
    last_poll_ended: Option<DateTime<Utc>>,
) -> TaskState {
    if dropped_at.is_some() {
        return TaskState::Done;
    }

    match (last_poll_started, last_poll_ended) {
        (Some(started), Some(ended)) if started > ended => TaskState::Running,
        (Some(_), None) => TaskState::Running,
        _ => {
            if let (Some(last_wake), Some(last_poll_started)) = (last_wake, last_poll_started) {
                if last_wake > last_poll_started {
                    return TaskState::Scheduled;
                }
            }
            TaskState::Idle
        }
    }
}
