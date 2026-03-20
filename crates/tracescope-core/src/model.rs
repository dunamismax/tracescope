//! Domain model types for TraceScope.

use std::collections::BTreeMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A monotonic duration encoded as microseconds for stable serialization.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DurationValue {
    micros: u64,
}

impl DurationValue {
    /// Creates a duration from microseconds.
    #[must_use]
    pub const fn from_micros(micros: u64) -> Self {
        Self { micros }
    }

    /// Creates a duration from milliseconds.
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self {
            micros: millis.saturating_mul(1_000),
        }
    }

    /// Returns the stored duration in microseconds.
    #[must_use]
    pub const fn as_micros(self) -> u64 {
        self.micros
    }

    /// Returns the stored duration in milliseconds.
    #[must_use]
    pub const fn as_millis(self) -> u64 {
        self.micros / 1_000
    }

    /// Adds another duration, saturating on overflow.
    #[must_use]
    pub const fn saturating_add(self, rhs: Self) -> Self {
        Self::from_micros(self.micros.saturating_add(rhs.micros))
    }

    /// Subtracts another duration, saturating at zero.
    #[must_use]
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        Self::from_micros(self.micros.saturating_sub(rhs.micros))
    }
}

/// Unique identifier for a persisted session.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct SessionId(pub u64);

impl fmt::Display for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Unique identifier for a task.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct TaskId(pub u64);

impl fmt::Display for TaskId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Unique identifier for a span.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct SpanId(pub u64);

impl fmt::Display for SpanId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Unique identifier for a resource.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct ResourceId(pub u64);

impl fmt::Display for ResourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

/// Serializable field value representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldValue {
    /// A debug-formatted string.
    Debug(String),
    /// A UTF-8 string value.
    String(String),
    /// An unsigned integer value.
    U64(u64),
    /// A signed integer value.
    I64(i64),
    /// A boolean value.
    Bool(bool),
}

impl FieldValue {
    /// Renders the field as user-facing text.
    #[must_use]
    pub fn as_display(&self) -> String {
        match self {
            Self::Debug(value) | Self::String(value) => value.clone(),
            Self::U64(value) => value.to_string(),
            Self::I64(value) => value.to_string(),
            Self::Bool(value) => value.to_string(),
        }
    }
}

/// A session recorded by TraceScope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Stable session identifier.
    pub id: SessionId,
    /// Human-readable session name.
    pub name: String,
    /// Telemetry target used to collect the session.
    pub target_address: String,
    /// UTC timestamp when recording started.
    pub started_at: DateTime<Utc>,
    /// UTC timestamp when recording ended.
    pub ended_at: Option<DateTime<Utc>>,
    /// Additional metadata for the session.
    pub metadata: BTreeMap<String, String>,
}

/// Runtime state for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    /// Task is currently executing.
    Running,
    /// Task is waiting for work.
    Idle,
    /// Task has been scheduled and is waiting to be polled.
    Scheduled,
    /// Task has completed or has been dropped.
    Done,
}

/// Aggregated task timing and poll statistics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStats {
    /// Number of observed polls.
    pub poll_count: u64,
    /// Number of observed wakes.
    pub wake_count: u64,
    /// Number of self-wakes observed.
    pub self_wake_count: u64,
    /// Cumulative time spent actively polling.
    pub busy_duration: DurationValue,
    /// Cumulative time spent scheduled before polling.
    pub scheduled_duration: DurationValue,
    /// Cumulative time spent idle or waiting.
    pub idle_duration: DurationValue,
    /// Total lifetime duration of the task.
    pub total_duration: DurationValue,
}

/// Warning category surfaced for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WarningKind {
    /// Average poll time is unexpectedly long.
    LongPoll,
    /// Task wakes itself repeatedly.
    SelfWake,
}

/// Warning attached to a specific task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskWarning {
    /// Kind of warning.
    pub kind: WarningKind,
    /// Human-readable explanation.
    pub message: String,
}

/// A warning record used by the warnings view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarningRecord {
    /// Related task identifier.
    pub task_id: TaskId,
    /// Related task name for convenient rendering.
    pub task_name: String,
    /// Warning kind.
    pub kind: WarningKind,
    /// Human-readable explanation.
    pub message: String,
}

/// A Tokio task observed by TraceScope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// Stable task identifier.
    pub id: TaskId,
    /// Human-readable task name.
    pub name: String,
    /// Current runtime state.
    pub state: TaskState,
    /// Captured task fields.
    pub fields: BTreeMap<String, FieldValue>,
    /// Polling and timing statistics.
    pub stats: TaskStats,
    /// Derived warnings for this task.
    pub warnings: Vec<TaskWarning>,
    /// UTC timestamp when the task was created.
    pub created_at: Option<DateTime<Utc>>,
    /// UTC timestamp when the task was dropped.
    pub dropped_at: Option<DateTime<Utc>>,
}

impl Task {
    /// Recomputes the task warning list from its statistics.
    pub fn refresh_warnings(&mut self) {
        let mut warnings = Vec::new();

        if self.stats.poll_count > 0 {
            let average_poll = self.stats.busy_duration.as_micros() / self.stats.poll_count;
            if average_poll >= 50_000 {
                warnings.push(TaskWarning {
                    kind: WarningKind::LongPoll,
                    message: format!("average poll time is {} ms", average_poll / 1_000),
                });
            }
        }

        if self.stats.self_wake_count > 0 {
            warnings.push(TaskWarning {
                kind: WarningKind::SelfWake,
                message: format!("task self-woke {} times", self.stats.self_wake_count),
            });
        }

        self.warnings = warnings;
    }

    /// Converts per-task warnings into flat warning records for list views.
    #[must_use]
    pub fn warning_records(&self) -> Vec<WarningRecord> {
        self.warnings
            .iter()
            .map(|warning| WarningRecord {
                task_id: self.id,
                task_name: self.name.clone(),
                kind: warning.kind,
                message: warning.message.clone(),
            })
            .collect()
    }
}

/// Span verbosity level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanLevel {
    /// Error level.
    Error,
    /// Warning level.
    Warn,
    /// Informational level.
    Info,
    /// Debug level.
    Debug,
    /// Trace level.
    Trace,
    /// Unknown level provided by the source.
    Unknown(String),
}

/// A tracing span observed on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    /// Stable span identifier.
    pub id: SpanId,
    /// Optional parent span identifier.
    pub parent_id: Option<SpanId>,
    /// Span name.
    pub name: String,
    /// Tracing target.
    pub target: String,
    /// Verbosity level.
    pub level: SpanLevel,
    /// Captured span fields.
    pub fields: BTreeMap<String, FieldValue>,
    /// First observed entry timestamp.
    pub entered_at: Option<DateTime<Utc>>,
    /// Most recent exit timestamp.
    pub exited_at: Option<DateTime<Utc>>,
    /// Total active time accumulated across enter/exit pairs.
    pub busy_duration: DurationValue,
}

/// Resource visibility in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceVisibility {
    /// Visible user-facing resource.
    Visible,
    /// Internal resource backing another resource.
    Internal,
}

/// Aggregated stats for a resource.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceStats {
    /// Timestamp when the resource was created.
    pub created_at: Option<DateTime<Utc>>,
    /// Timestamp when the resource was dropped.
    pub dropped_at: Option<DateTime<Utc>>,
    /// Runtime attributes currently associated with the resource.
    pub attributes: BTreeMap<String, String>,
    /// Total number of observed poll operations.
    pub poll_op_count: u64,
    /// Number of ready poll results.
    pub ready_count: u64,
    /// Number of pending poll results.
    pub pending_count: u64,
}

/// A runtime resource observed on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    /// Stable resource identifier.
    pub id: ResourceId,
    /// Resource kind label.
    pub kind: String,
    /// Resource name.
    pub name: String,
    /// Runtime statistics.
    pub stats: ResourceStats,
    /// Visibility for rendering.
    pub visibility: ResourceVisibility,
}

/// Session contents loaded from the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedSession {
    /// Session metadata.
    pub session: Session,
    /// Persisted tasks.
    pub tasks: Vec<Task>,
    /// Persisted spans.
    pub spans: Vec<Span>,
    /// Persisted resources.
    pub resources: Vec<Resource>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_math_saturates() {
        let total = DurationValue::from_millis(10);
        let busy = DurationValue::from_millis(3);
        let idle = total.saturating_sub(busy);

        assert_eq!(idle.as_millis(), 7);
        assert_eq!(busy.saturating_sub(total).as_millis(), 0);
    }

    #[test]
    fn task_warnings_are_derived_from_stats() {
        let mut task = Task {
            id: TaskId(1),
            name: "worker".to_string(),
            state: TaskState::Idle,
            fields: BTreeMap::new(),
            stats: TaskStats {
                poll_count: 2,
                wake_count: 0,
                self_wake_count: 3,
                busy_duration: DurationValue::from_millis(140),
                scheduled_duration: DurationValue::default(),
                idle_duration: DurationValue::default(),
                total_duration: DurationValue::default(),
            },
            warnings: Vec::new(),
            created_at: None,
            dropped_at: None,
        };

        task.refresh_warnings();

        assert_eq!(task.warnings.len(), 2);
        assert_eq!(task.warning_records().len(), 2);
    }
}
