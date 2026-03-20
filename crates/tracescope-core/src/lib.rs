//! Core data model, collector, query helpers, and storage for TraceScope.

pub mod collector;
pub mod model;
pub mod query;
pub mod store;

pub use collector::{
    Collector, CollectorCommand, CollectorEvent, CollectorSnapshot, ConnectionState,
};
pub use model::{
    DurationValue, FieldValue, LoadedSession, Resource, ResourceId, ResourceStats,
    ResourceVisibility, Session, SessionId, Span, SpanId, SpanLevel, Task, TaskId, TaskState,
    TaskStats, TaskWarning, WarningKind, WarningRecord,
};
pub use store::{SessionDraft, SessionStore, StoreError};
