//! gRPC collector for Tokio console telemetry.

use std::{
    collections::{hash_map::Entry, BTreeMap, HashMap},
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
    fn insert_batch(&mut self, metadata: Option<RegisterMetadata>) -> Vec<u64> {
        let mut inserted = Vec::new();

        if let Some(metadata) = metadata {
            for entry in metadata.metadata {
                if let (Some(id), Some(payload)) = (entry.id, entry.metadata) {
                    inserted.push(id.id);
                    self.entries.insert(id.id, payload);
                }
            }
        }

        inserted
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
    span_metadata_ids: HashMap<SpanId, u64>,
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
            span_metadata_ids: HashMap::new(),
            resources: HashMap::new(),
            active_spans: HashMap::new(),
            updated_at: None,
        }
    }

    fn apply_update(&mut self, update: Update) {
        self.observe_timestamp(timestamp_to_datetime(update.now));
        let updated_metadata = self.metadata.insert_batch(update.new_metadata);
        self.hydrate_spans_for_metadata(&updated_metadata);

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
            Some(Event::RegisterMetadata(metadata)) => {
                let updated_metadata = self.metadata.insert_batch(Some(metadata));
                self.hydrate_spans_for_metadata(&updated_metadata);
            }
            Some(Event::NewSpan(span)) => self.register_span(span),
            Some(Event::EnterSpan(enter)) => {
                if let (Some(span_id), Some(at)) = (enter.span_id, timestamp_to_datetime(enter.at))
                {
                    self.observe_timestamp(Some(at));
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
                    self.observe_timestamp(Some(at));
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
                    self.observe_timestamp(Some(at));
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
            Some(Event::RegisterThread(_)) | None => {}
        }
    }

    fn apply_task_update(&mut self, update: tasks::TaskUpdate) {
        for new_task in update.new_tasks {
            let Some(id) = new_task.id.map(|value| TaskId(value.id)) else {
                continue;
            };

            let metadata = self.metadata.get(new_task.metadata);
            let name = metadata
                .map(|value| value.name.clone())
                .unwrap_or_else(|| format!("task-{}", id.0));
            let fields = decode_fields(metadata, &new_task.fields);

            match self.tasks.entry(id) {
                Entry::Vacant(entry) => {
                    entry.insert(Task {
                        id,
                        name,
                        state: TaskState::Idle,
                        fields,
                        stats: TaskStats::default(),
                        warnings: Vec::new(),
                        created_at: None,
                        dropped_at: None,
                    });
                }
                Entry::Occupied(mut entry) => {
                    let task = entry.get_mut();
                    task.name = name;
                    task.fields = fields;
                }
            }
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
            let name = metadata
                .map(|value| value.name.clone())
                .unwrap_or_else(|| new_resource.concrete_type.clone());
            let visibility = if new_resource.is_internal {
                ResourceVisibility::Internal
            } else {
                ResourceVisibility::Visible
            };

            match self.resources.entry(id) {
                Entry::Vacant(entry) => {
                    entry.insert(Resource {
                        id,
                        kind,
                        name,
                        stats: ResourceStats::default(),
                        visibility,
                    });
                }
                Entry::Occupied(mut entry) => {
                    let resource = entry.get_mut();
                    resource.kind = kind;
                    resource.name = name;
                    resource.visibility = visibility;
                }
            }
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

        let observed_at = timestamp_to_datetime(span.at);
        self.observe_timestamp(observed_at);
        let metadata_id = span.metadata_id.map(|value| value.id);
        let metadata = metadata_id.and_then(|meta_id| self.metadata.entries.get(&meta_id).cloned());

        if let Some(metadata_id) = metadata_id {
            self.span_metadata_ids.insert(id, metadata_id);
        }

        let entry = self
            .spans
            .entry(id)
            .or_insert_with(|| placeholder_span(id, observed_at));
        hydrate_span(entry, metadata.as_ref(), &span.fields, observed_at);
    }

    fn hydrate_spans_for_metadata(&mut self, metadata_ids: &[u64]) {
        if metadata_ids.is_empty() {
            return;
        }

        for (span_id, span) in &mut self.spans {
            let Some(metadata_id) = self.span_metadata_ids.get(span_id) else {
                continue;
            };
            if !metadata_ids.contains(metadata_id) {
                continue;
            }

            if let Some(metadata) = self.metadata.entries.get(metadata_id) {
                hydrate_span_metadata(span, metadata);
            }
        }
    }

    fn observe_timestamp(&mut self, observed_at: Option<DateTime<Utc>>) {
        if let Some(observed_at) = observed_at {
            self.updated_at = Some(
                self.updated_at
                    .map_or(observed_at, |current| current.max(observed_at)),
            );
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

pub fn normalize_target(input: &str) -> String {
    if input.starts_with("http://") || input.starts_with("https://") {
        input.to_string()
    } else {
        format!("http://{input}")
    }
}

fn timestamp_to_datetime(timestamp: Option<prost_types::Timestamp>) -> Option<DateTime<Utc>> {
    timestamp.and_then(|timestamp| {
        let nanos = u32::try_from(timestamp.nanos).ok()?;
        DateTime::<Utc>::from_timestamp(timestamp.seconds, nanos)
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

fn placeholder_span(id: SpanId, entered_at: Option<DateTime<Utc>>) -> Span {
    Span {
        id,
        parent_id: None,
        name: format!("span-{}", id.0),
        target: String::from("unknown"),
        level: SpanLevel::Unknown(String::from("unknown")),
        fields: BTreeMap::new(),
        entered_at,
        exited_at: None,
        busy_duration: DurationValue::default(),
    }
}

fn hydrate_span(
    span: &mut Span,
    metadata: Option<&Metadata>,
    fields: &[Field],
    observed_at: Option<DateTime<Utc>>,
) {
    if let Some(metadata) = metadata {
        hydrate_span_metadata(span, metadata);
    }

    span.fields = decode_fields(metadata, fields);
    span.entered_at = match (span.entered_at, observed_at) {
        (Some(existing), Some(observed_at)) => Some(existing.min(observed_at)),
        (None, Some(observed_at)) => Some(observed_at),
        (Some(existing), None) => Some(existing),
        (None, None) => None,
    };
}

fn hydrate_span_metadata(span: &mut Span, metadata: &Metadata) {
    span.name = metadata.name.clone();
    span.target = metadata.target.clone();
    span.level = level_from_metadata(metadata);
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use console_api::{
        field, instrument::Update, metadata, register_metadata, resources, tasks, trace, Field, Id,
        MetaId, Metadata, PollStats, RegisterMetadata, Span as ApiSpan,
    };
    use proptest::{collection::vec, prelude::*};

    use crate::model::{
        DurationValue, FieldValue, Resource, ResourceId, ResourceStats, ResourceVisibility, Span,
        SpanId, SpanLevel, Task, TaskId, TaskState, TaskStats, TaskWarning, WarningKind,
    };

    use super::{normalize_target, timestamp_to_datetime, CollectorState};

    #[test]
    fn normalize_target_adds_http_scheme_when_missing() {
        assert_eq!(normalize_target("127.0.0.1:6669"), "http://127.0.0.1:6669");
        assert_eq!(
            normalize_target("https://demo.internal:7000"),
            "https://demo.internal:7000"
        );
    }

    #[test]
    fn snapshot_sorts_entities_and_flattens_warning_records() {
        let mut state = CollectorState::new(String::from("http://127.0.0.1:6669"));

        state.tasks.insert(
            TaskId(9),
            Task {
                id: TaskId(9),
                name: String::from("beta"),
                state: TaskState::Idle,
                fields: BTreeMap::from([(
                    String::from("queue"),
                    FieldValue::String(String::from("critical")),
                )]),
                stats: TaskStats::default(),
                warnings: vec![TaskWarning {
                    kind: WarningKind::SelfWake,
                    message: String::from("task self-woke 1 times"),
                }],
                created_at: None,
                dropped_at: None,
            },
        );
        state.tasks.insert(
            TaskId(3),
            Task {
                id: TaskId(3),
                name: String::from("alpha"),
                state: TaskState::Running,
                fields: BTreeMap::new(),
                stats: TaskStats::default(),
                warnings: vec![
                    TaskWarning {
                        kind: WarningKind::LongPoll,
                        message: String::from("average poll time is 75 ms"),
                    },
                    TaskWarning {
                        kind: WarningKind::SelfWake,
                        message: String::from("task self-woke 2 times"),
                    },
                ],
                created_at: None,
                dropped_at: None,
            },
        );

        state.spans.insert(SpanId(8), span_fixture(8));
        state.spans.insert(SpanId(1), span_fixture(1));
        state
            .resources
            .insert(ResourceId(5), resource_fixture(5, 2, 1));
        state
            .resources
            .insert(ResourceId(2), resource_fixture(2, 1, 0));

        let snapshot = state.snapshot();

        assert_eq!(
            snapshot
                .tasks
                .iter()
                .map(|task| task.id.0)
                .collect::<Vec<_>>(),
            vec![3, 9]
        );
        assert_eq!(
            snapshot
                .spans
                .iter()
                .map(|span| span.id.0)
                .collect::<Vec<_>>(),
            vec![1, 8]
        );
        assert_eq!(
            snapshot
                .resources
                .iter()
                .map(|resource| resource.id.0)
                .collect::<Vec<_>>(),
            vec![2, 5]
        );
        assert_eq!(snapshot.warnings.len(), 3);
        assert_eq!(snapshot.warnings[0].task_id, TaskId(3));
    }

    #[test]
    fn late_span_registration_hydrates_placeholder_metadata() {
        let mut state = CollectorState::new(String::from("http://127.0.0.1:6669"));

        state.apply_trace_event(trace::TraceEvent {
            event: Some(trace::trace_event::Event::RegisterMetadata(
                register_metadata(7, "worker", "demo"),
            )),
        });
        state.apply_trace_event(enter_span_event(1, 5));
        state.apply_trace_event(trace::TraceEvent {
            event: Some(trace::trace_event::Event::NewSpan(ApiSpan {
                id: span_id(1),
                metadata_id: meta_id(7),
                fields: vec![Field {
                    name: Some(field::Name::StrName(String::from("task"))),
                    value: Some(field::Value::StrVal(String::from("alpha"))),
                    ..Default::default()
                }],
                at: timestamp(10),
            })),
        });

        let span = state.spans.get(&SpanId(1)).expect("span recorded");

        assert_eq!(span.name, "worker");
        assert_eq!(span.target, "demo");
        assert_eq!(span.level, SpanLevel::Info);
        assert_eq!(
            span.fields.get("task"),
            Some(&FieldValue::String(String::from("alpha")))
        );
        assert_eq!(span.entered_at, timestamp_to_datetime(timestamp(5)));
    }

    #[test]
    fn trace_only_activity_advances_updated_at() {
        let mut state = CollectorState::new(String::from("http://127.0.0.1:6669"));

        state.apply_trace_event(enter_span_event(1, 25));
        assert_eq!(state.updated_at, timestamp_to_datetime(timestamp(25)));

        state.apply_trace_event(close_span_event(1, 40));

        assert_eq!(state.updated_at, timestamp_to_datetime(timestamp(40)));
        assert_eq!(
            state.snapshot().updated_at,
            timestamp_to_datetime(timestamp(40))
        );
    }

    proptest! {
        #[test]
        fn task_stat_aggregation_partitions_total_duration(
            (total_micros, busy_micros, scheduled_micros, polls, wakes, self_wakes) in task_stats_strategy()
        ) {
            let mut state = CollectorState::new(String::from("http://127.0.0.1:6669"));
            state.apply_update(Update {
                now: timestamp(total_micros),
                task_update: Some(tasks::TaskUpdate {
                    new_tasks: vec![tasks::Task {
                        id: id(1),
                        ..Default::default()
                    }],
                    stats_update: HashMap::from([(
                        1,
                        tasks::Stats {
                            created_at: timestamp(0),
                            dropped_at: timestamp(total_micros),
                            wakes,
                            poll_stats: Some(PollStats {
                                polls,
                                last_poll_started: timestamp(total_micros),
                                last_poll_ended: timestamp(total_micros),
                                busy_time: duration(busy_micros),
                                ..Default::default()
                            }),
                            self_wakes,
                            scheduled_time: duration(scheduled_micros),
                            ..Default::default()
                        },
                    )]),
                    ..Default::default()
                }),
                ..Default::default()
            });

            let task = state.tasks.get(&TaskId(1)).expect("task inserted");

            prop_assert_eq!(task.state, TaskState::Done);
            prop_assert_eq!(task.stats.total_duration.as_micros(), total_micros);
            prop_assert_eq!(task.stats.busy_duration.as_micros(), busy_micros);
            prop_assert_eq!(task.stats.scheduled_duration.as_micros(), scheduled_micros);
            prop_assert_eq!(
                task.stats.idle_duration.as_micros(),
                total_micros - busy_micros - scheduled_micros
            );
            prop_assert_eq!(
                task.stats.idle_duration.as_micros()
                    + task.stats.busy_duration.as_micros()
                    + task.stats.scheduled_duration.as_micros(),
                task.stats.total_duration.as_micros()
            );
        }

        #[test]
        fn span_timeline_busy_duration_matches_enter_exit_intervals(
            intervals in vec((0u64..20_000, 0u64..20_000), 1..12)
        ) {
            let mut state = CollectorState::new(String::from("http://127.0.0.1:6669"));
            state.apply_trace_event(trace::TraceEvent {
                event: Some(trace::trace_event::Event::RegisterMetadata(register_metadata(7, "timeline", "demo"))),
            });
            state.apply_trace_event(trace::TraceEvent {
                event: Some(trace::trace_event::Event::NewSpan(ApiSpan {
                    id: span_id(1),
                    metadata_id: meta_id(7),
                    at: timestamp(0),
                    ..Default::default()
                })),
            });

            let mut clock = 0;
            let mut expected_busy = 0;

            for (index, (gap_micros, busy_micros)) in intervals.iter().copied().enumerate() {
                clock += gap_micros;
                state.apply_trace_event(enter_span_event(1, clock));
                clock += busy_micros;
                expected_busy += busy_micros;

                let event = if index + 1 == intervals.len() {
                    close_span_event(1, clock)
                } else {
                    exit_span_event(1, clock)
                };
                state.apply_trace_event(event);
            }

            let span = state.spans.get(&SpanId(1)).expect("span recorded");

            prop_assert_eq!(span.busy_duration.as_micros(), expected_busy);
            prop_assert!(state.active_spans.is_empty());
            prop_assert_eq!(span.entered_at, timestamp_to_datetime(timestamp(0)));
            prop_assert_eq!(span.exited_at, timestamp_to_datetime(timestamp(clock)));
        }

        #[test]
        fn resource_poll_aggregation_counts_ready_and_pending_ops(ready_flags in vec(any::<bool>(), 0..64)) {
            let mut state = CollectorState::new(String::from("http://127.0.0.1:6669"));
            state.apply_resource_update(resources::ResourceUpdate {
                new_resources: vec![resources::Resource {
                    id: id(1),
                    concrete_type: String::from("tokio::time::Sleep"),
                    kind: Some(resources::resource::Kind {
                        kind: Some(resources::resource::kind::Kind::Known(
                            resources::resource::kind::Known::Timer as i32,
                        )),
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            });
            state.apply_resource_update(resources::ResourceUpdate {
                new_poll_ops: ready_flags
                    .iter()
                    .copied()
                    .map(|is_ready| resources::PollOp {
                        resource_id: id(1),
                        is_ready,
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            });

            let resource = state.resources.get(&ResourceId(1)).expect("resource inserted");
            let ready_count = ready_flags.iter().filter(|is_ready| **is_ready).count() as u64;
            let pending_count = ready_flags.len() as u64 - ready_count;

            prop_assert_eq!(resource.stats.ready_count, ready_count);
            prop_assert_eq!(resource.stats.pending_count, pending_count);
            prop_assert_eq!(resource.stats.poll_op_count, ready_count + pending_count);
        }
    }

    fn task_stats_strategy() -> impl Strategy<Value = (u64, u64, u64, u64, u64, u64)> {
        (0u64..5_000_000, 0u64..256, 0u64..256, 0u64..256).prop_flat_map(
            |(total_micros, polls, wakes, self_wakes)| {
                (Just(total_micros), 0u64..=total_micros).prop_flat_map(
                    move |(total_micros, busy_micros)| {
                        (
                            Just(total_micros),
                            Just(busy_micros),
                            0u64..=(total_micros - busy_micros),
                            Just(polls),
                            Just(wakes),
                            Just(self_wakes),
                        )
                    },
                )
            },
        )
    }

    fn id(value: u64) -> Option<Id> {
        Some(Id { id: value })
    }

    fn span_id(value: u64) -> Option<console_api::SpanId> {
        Some(console_api::SpanId { id: value })
    }

    fn meta_id(value: u64) -> Option<MetaId> {
        Some(MetaId { id: value })
    }

    fn timestamp(micros: u64) -> Option<prost_types::Timestamp> {
        let seconds = i64::try_from(micros / 1_000_000).expect("seconds fit in i64");
        let nanos = i32::try_from((micros % 1_000_000) * 1_000).expect("nanos fit in i32");
        Some(prost_types::Timestamp { seconds, nanos })
    }

    fn duration(micros: u64) -> Option<prost_types::Duration> {
        let seconds = i64::try_from(micros / 1_000_000).expect("seconds fit in i64");
        let nanos = i32::try_from((micros % 1_000_000) * 1_000).expect("nanos fit in i32");
        Some(prost_types::Duration { seconds, nanos })
    }

    fn register_metadata(id: u64, name: &str, target: &str) -> RegisterMetadata {
        RegisterMetadata {
            metadata: vec![register_metadata::NewMetadata {
                id: meta_id(id),
                metadata: Some(Metadata {
                    name: name.to_string(),
                    target: target.to_string(),
                    module_path: String::from("tracescope::tests"),
                    location: None,
                    kind: metadata::Kind::Span as i32,
                    level: metadata::Level::Info as i32,
                    field_names: Vec::new(),
                }),
            }],
        }
    }

    fn enter_span_event(span_id_value: u64, at_micros: u64) -> trace::TraceEvent {
        trace::TraceEvent {
            event: Some(trace::trace_event::Event::EnterSpan(
                trace::trace_event::Enter {
                    span_id: span_id(span_id_value),
                    at: timestamp(at_micros),
                    ..Default::default()
                },
            )),
        }
    }

    fn exit_span_event(span_id_value: u64, at_micros: u64) -> trace::TraceEvent {
        trace::TraceEvent {
            event: Some(trace::trace_event::Event::ExitSpan(
                trace::trace_event::Exit {
                    span_id: span_id(span_id_value),
                    at: timestamp(at_micros),
                    ..Default::default()
                },
            )),
        }
    }

    fn close_span_event(span_id_value: u64, at_micros: u64) -> trace::TraceEvent {
        trace::TraceEvent {
            event: Some(trace::trace_event::Event::CloseSpan(
                trace::trace_event::Close {
                    span_id: span_id(span_id_value),
                    at: timestamp(at_micros),
                },
            )),
        }
    }

    fn span_fixture(id: u64) -> Span {
        Span {
            id: SpanId(id),
            parent_id: None,
            name: format!("span-{id}"),
            target: String::from("demo"),
            level: SpanLevel::Info,
            fields: BTreeMap::new(),
            entered_at: None,
            exited_at: None,
            busy_duration: DurationValue::from_millis(5),
        }
    }

    fn resource_fixture(id: u64, ready_count: u64, pending_count: u64) -> Resource {
        Resource {
            id: ResourceId(id),
            kind: String::from("timer"),
            name: format!("resource-{id}"),
            stats: ResourceStats {
                poll_op_count: ready_count + pending_count,
                ready_count,
                pending_count,
                ..ResourceStats::default()
            },
            visibility: ResourceVisibility::Visible,
        }
    }
}
