//! SQLite-backed session persistence.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use thiserror::Error;

use crate::model::{LoadedSession, Resource, Session, SessionId, Span, Task};

/// Input required to create a new session row.
#[derive(Debug, Clone)]
pub struct SessionDraft {
    /// Human-readable session name.
    pub name: String,
    /// Telemetry source used when recording.
    pub target_address: String,
    /// Recording start timestamp.
    pub started_at: DateTime<Utc>,
    /// Recording end timestamp.
    pub ended_at: Option<DateTime<Utc>>,
    /// Extra user or runtime metadata.
    pub metadata: BTreeMap<String, String>,
}

/// Errors returned by the session store.
#[derive(Debug, Error)]
pub enum StoreError {
    /// The user's home directory could not be resolved.
    #[error("unable to resolve the home directory for TraceScope data")]
    HomeDirUnavailable,
    /// Filesystem interaction failed.
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    /// SQLite interaction failed.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// JSON serialization or deserialization failed.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// Session identifier overflowed the supported range.
    #[error("numeric conversion failed while handling a session identifier")]
    IdConversion,
    /// The requested session does not exist.
    #[error("session {0} was not found")]
    SessionNotFound(SessionId),
}

/// SQLite-backed store for recorded sessions.
#[derive(Debug, Clone)]
pub struct SessionStore {
    database_path: PathBuf,
}

impl SessionStore {
    /// Opens the default TraceScope data store at `~/.tracescope/sessions.db`.
    pub fn open_default() -> Result<Self, StoreError> {
        let home = dirs::home_dir().ok_or(StoreError::HomeDirUnavailable)?;
        Self::open_in_dir(home.join(".tracescope"))
    }

    /// Opens or creates the store in the provided data directory.
    pub fn open_in_dir(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let data_dir = path.as_ref();
        fs::create_dir_all(data_dir)?;
        let database_path = data_dir.join("sessions.db");
        let store = Self { database_path };
        store.initialize()?;
        Ok(store)
    }

    /// Returns the underlying database path.
    #[must_use]
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    /// Creates a new session and returns its assigned identifier.
    pub fn create_session(&self, draft: &SessionDraft) -> Result<SessionId, StoreError> {
        let connection = self.connection()?;
        insert_session(&connection, draft)
    }

    /// Persists a full recorded snapshot in a single transaction.
    pub fn save_session_snapshot(
        &self,
        draft: &SessionDraft,
        tasks: &[Task],
        spans: &[Span],
        resources: &[Resource],
    ) -> Result<SessionId, StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;

        let session_id = insert_session(&transaction, draft)?;
        replace_task_batch(&transaction, session_id, tasks)?;
        replace_span_batch(&transaction, session_id, spans)?;
        replace_resource_batch(&transaction, session_id, resources)?;

        transaction.commit()?;
        Ok(session_id)
    }

    /// Persists a full batch of tasks for a session.
    pub fn save_task_batch(&self, session_id: SessionId, tasks: &[Task]) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        replace_task_batch(&transaction, session_id, tasks)?;
        transaction.commit()?;
        Ok(())
    }

    /// Persists a full batch of spans for a session.
    pub fn save_span_batch(&self, session_id: SessionId, spans: &[Span]) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        replace_span_batch(&transaction, session_id, spans)?;
        transaction.commit()?;
        Ok(())
    }

    /// Persists a full batch of resources for a session.
    pub fn save_resource_batch(
        &self,
        session_id: SessionId,
        resources: &[Resource],
    ) -> Result<(), StoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        replace_resource_batch(&transaction, session_id, resources)?;
        transaction.commit()?;
        Ok(())
    }

    /// Lists all persisted sessions in reverse chronological order.
    pub fn list_sessions(&self) -> Result<Vec<Session>, StoreError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, target_address, started_at, ended_at, metadata_json
             FROM sessions
             ORDER BY started_at DESC",
        )?;

        let rows = statement.query_map([], |row| {
            let id = SessionId(
                u64::try_from(row.get::<_, i64>(0)?)
                    .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, 0))?,
            );
            let started_at = parse_datetime(row.get::<_, String>(3)?).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            let ended_at = row
                .get::<_, Option<String>>(4)?
                .map(parse_datetime)
                .transpose()
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            let metadata = serde_json::from_str(&row.get::<_, String>(5)?).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;

            Ok(Session {
                id,
                name: row.get(1)?,
                target_address: row.get(2)?,
                started_at,
                ended_at,
                metadata,
            })
        })?;

        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?);
        }

        Ok(sessions)
    }

    /// Loads all persisted data for a single session.
    pub fn load_session(&self, session_id: SessionId) -> Result<LoadedSession, StoreError> {
        let connection = self.connection()?;
        let session = load_session_row(&connection, session_id)?
            .ok_or(StoreError::SessionNotFound(session_id))?;
        let tasks = load_payloads::<Task>(&connection, PayloadTable::Tasks, session_id)?;
        let spans = load_payloads::<Span>(&connection, PayloadTable::Spans, session_id)?;
        let resources =
            load_payloads::<Resource>(&connection, PayloadTable::Resources, session_id)?;

        Ok(LoadedSession {
            session,
            tasks,
            spans,
            resources,
        })
    }

    /// Deletes a session and all related rows.
    pub fn delete_session(&self, session_id: SessionId) -> Result<(), StoreError> {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM sessions WHERE id = ?1",
            params![as_sql_id(session_id)?],
        )?;
        Ok(())
    }

    fn initialize(&self) -> Result<(), StoreError> {
        let connection = self.connection()?;
        connection.execute_batch(
            "
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                target_address TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                metadata_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tasks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                task_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                state TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS spans (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                span_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                target TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS resources (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                resource_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );
            ",
        )?;
        Ok(())
    }

    fn connection(&self) -> Result<Connection, StoreError> {
        let connection = Connection::open(&self.database_path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Ok(connection)
    }
}

#[derive(Debug, Clone, Copy)]
enum PayloadTable {
    Tasks,
    Spans,
    Resources,
}

impl PayloadTable {
    fn select_payload_sql(self) -> &'static str {
        match self {
            Self::Tasks => {
                "SELECT payload_json FROM tasks WHERE session_id = ?1 ORDER BY task_id ASC"
            }
            Self::Spans => {
                "SELECT payload_json FROM spans WHERE session_id = ?1 ORDER BY span_id ASC"
            }
            Self::Resources => {
                "SELECT payload_json FROM resources WHERE session_id = ?1 ORDER BY resource_id ASC"
            }
        }
    }
}

fn load_payloads<T>(
    connection: &Connection,
    table: PayloadTable,
    session_id: SessionId,
) -> Result<Vec<T>, StoreError>
where
    T: serde::de::DeserializeOwned,
{
    let mut statement = connection.prepare(table.select_payload_sql())?;
    let rows = statement.query_map(params![as_sql_id(session_id)?], |row| {
        let payload = row.get::<_, String>(0)?;
        serde_json::from_str::<T>(&payload).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
    })?;

    let mut values = Vec::new();
    for row in rows {
        values.push(row?);
    }
    Ok(values)
}

fn insert_session(connection: &Connection, draft: &SessionDraft) -> Result<SessionId, StoreError> {
    connection.execute(
        "INSERT INTO sessions (name, target_address, started_at, ended_at, metadata_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            draft.name,
            draft.target_address,
            draft.started_at.to_rfc3339(),
            draft.ended_at.map(|value| value.to_rfc3339()),
            serde_json::to_string(&draft.metadata)?,
        ],
    )?;

    let id = u64::try_from(connection.last_insert_rowid()).map_err(|_| StoreError::IdConversion)?;
    Ok(SessionId(id))
}

fn replace_task_batch(
    connection: &Connection,
    session_id: SessionId,
    tasks: &[Task],
) -> Result<(), StoreError> {
    connection.execute(
        "DELETE FROM tasks WHERE session_id = ?1",
        params![as_sql_id(session_id)?],
    )?;

    for task in tasks {
        connection.execute(
            "INSERT INTO tasks (session_id, task_id, name, state, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                as_sql_id(session_id)?,
                as_sql_u64(task.id.0)?,
                task.name,
                task.state.to_string(),
                serde_json::to_string(task)?,
            ],
        )?;
    }

    Ok(())
}

fn replace_span_batch(
    connection: &Connection,
    session_id: SessionId,
    spans: &[Span],
) -> Result<(), StoreError> {
    connection.execute(
        "DELETE FROM spans WHERE session_id = ?1",
        params![as_sql_id(session_id)?],
    )?;

    for span in spans {
        connection.execute(
            "INSERT INTO spans (session_id, span_id, name, target, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                as_sql_id(session_id)?,
                as_sql_u64(span.id.0)?,
                span.name,
                span.target,
                serde_json::to_string(span)?,
            ],
        )?;
    }

    Ok(())
}

fn replace_resource_batch(
    connection: &Connection,
    session_id: SessionId,
    resources: &[Resource],
) -> Result<(), StoreError> {
    connection.execute(
        "DELETE FROM resources WHERE session_id = ?1",
        params![as_sql_id(session_id)?],
    )?;

    for resource in resources {
        connection.execute(
            "INSERT INTO resources (session_id, resource_id, name, kind, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                as_sql_id(session_id)?,
                as_sql_u64(resource.id.0)?,
                resource.name,
                resource.kind,
                serde_json::to_string(resource)?,
            ],
        )?;
    }

    Ok(())
}

fn load_session_row(
    connection: &Connection,
    session_id: SessionId,
) -> Result<Option<Session>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT id, name, target_address, started_at, ended_at, metadata_json
         FROM sessions
         WHERE id = ?1",
    )?;
    let session = statement
        .query_row(params![as_sql_id(session_id)?], map_session_row)
        .optional()?;
    Ok(session)
}

fn map_session_row(row: &Row<'_>) -> rusqlite::Result<Session> {
    let id = SessionId(
        u64::try_from(row.get::<_, i64>(0)?)
            .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, 0))?,
    );
    let started_at = parse_datetime(row.get::<_, String>(3)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let ended_at = row
        .get::<_, Option<String>>(4)?
        .map(parse_datetime)
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let metadata = serde_json::from_str(&row.get::<_, String>(5)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;

    Ok(Session {
        id,
        name: row.get(1)?,
        target_address: row.get(2)?,
        started_at,
        ended_at,
        metadata,
    })
}

fn as_sql_id(id: SessionId) -> Result<i64, StoreError> {
    i64::try_from(id.0).map_err(|_| StoreError::IdConversion)
}

fn as_sql_u64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::IdConversion)
}

fn parse_datetime(input: String) -> Result<DateTime<Utc>, chrono::ParseError> {
    Ok(DateTime::parse_from_rfc3339(&input)?.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{Duration, Utc};
    use tempfile::tempdir;

    use crate::model::{
        DurationValue, Resource, ResourceId, ResourceStats, ResourceVisibility, SessionId, Span,
        SpanId, SpanLevel, Task, TaskId, TaskState, TaskStats,
    };

    use super::{as_sql_id, SessionDraft, SessionStore};

    #[test]
    fn session_round_trip_works() {
        let temp = tempdir().expect("tempdir");
        let store = SessionStore::open_in_dir(temp.path()).expect("store");
        let started_at = Utc::now();
        let ended_at = started_at + Duration::seconds(5);

        let session_id = store
            .create_session(&SessionDraft {
                name: "demo recording".to_string(),
                target_address: "http://127.0.0.1:6669".to_string(),
                started_at,
                ended_at: Some(ended_at),
                metadata: BTreeMap::from([(String::from("source"), String::from("test"))]),
            })
            .expect("create session");

        store
            .save_task_batch(
                session_id,
                &[Task {
                    id: TaskId(7),
                    name: "worker".to_string(),
                    state: TaskState::Idle,
                    fields: BTreeMap::new(),
                    stats: TaskStats {
                        poll_count: 4,
                        wake_count: 2,
                        self_wake_count: 0,
                        busy_duration: DurationValue::from_millis(25),
                        scheduled_duration: DurationValue::from_millis(5),
                        idle_duration: DurationValue::from_millis(20),
                        total_duration: DurationValue::from_millis(50),
                    },
                    warnings: Vec::new(),
                    created_at: Some(started_at),
                    dropped_at: Some(ended_at),
                }],
            )
            .expect("save tasks");

        store
            .save_span_batch(
                session_id,
                &[Span {
                    id: SpanId(11),
                    parent_id: None,
                    name: "poll".to_string(),
                    target: "demo".to_string(),
                    level: SpanLevel::Info,
                    fields: BTreeMap::new(),
                    entered_at: Some(started_at),
                    exited_at: Some(ended_at),
                    busy_duration: DurationValue::from_millis(50),
                }],
            )
            .expect("save spans");

        store
            .save_resource_batch(
                session_id,
                &[Resource {
                    id: ResourceId(3),
                    kind: "timer".to_string(),
                    name: "interval".to_string(),
                    stats: ResourceStats::default(),
                    visibility: ResourceVisibility::Visible,
                }],
            )
            .expect("save resources");

        let sessions = store.list_sessions().expect("list sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, session_id);

        let loaded = store.load_session(session_id).expect("load session");
        assert_eq!(loaded.session.id, SessionId(session_id.0));
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.spans.len(), 1);
        assert_eq!(loaded.resources.len(), 1);

        store.delete_session(session_id).expect("delete session");
        assert!(store.list_sessions().expect("list after delete").is_empty());
    }

    #[test]
    fn saving_a_batch_replaces_previous_rows_for_the_same_session() {
        let temp = tempdir().expect("tempdir");
        let store = SessionStore::open_in_dir(temp.path()).expect("store");
        let started_at = Utc::now();

        let session_id = store
            .create_session(&SessionDraft {
                name: "replace batch".to_string(),
                target_address: "http://127.0.0.1:6669".to_string(),
                started_at,
                ended_at: None,
                metadata: BTreeMap::new(),
            })
            .expect("create session");

        let original_task = Task {
            id: TaskId(1),
            name: "first".to_string(),
            state: TaskState::Idle,
            fields: BTreeMap::new(),
            stats: TaskStats::default(),
            warnings: Vec::new(),
            created_at: Some(started_at),
            dropped_at: None,
        };
        let replacement_task = Task {
            id: TaskId(2),
            name: "second".to_string(),
            state: TaskState::Running,
            fields: BTreeMap::new(),
            stats: TaskStats {
                poll_count: 3,
                ..TaskStats::default()
            },
            warnings: Vec::new(),
            created_at: Some(started_at),
            dropped_at: None,
        };

        store
            .save_task_batch(session_id, &[original_task])
            .expect("save original batch");
        store
            .save_task_batch(session_id, std::slice::from_ref(&replacement_task))
            .expect("save replacement batch");

        let loaded = store.load_session(session_id).expect("load session");
        assert_eq!(loaded.tasks, vec![replacement_task]);

        let connection = store.connection().expect("connection");
        let task_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE session_id = ?1",
                [as_sql_id(session_id).expect("session id")],
                |row| row.get(0),
            )
            .expect("task count");
        assert_eq!(task_count, 1);
    }

    #[test]
    fn deleting_a_session_cascades_to_related_rows() {
        let temp = tempdir().expect("tempdir");
        let store = SessionStore::open_in_dir(temp.path()).expect("store");
        let started_at = Utc::now();
        let ended_at = started_at + Duration::seconds(1);

        let session_id = store
            .create_session(&SessionDraft {
                name: "cascade delete".to_string(),
                target_address: "http://127.0.0.1:6669".to_string(),
                started_at,
                ended_at: Some(ended_at),
                metadata: BTreeMap::new(),
            })
            .expect("create session");

        store
            .save_task_batch(
                session_id,
                &[Task {
                    id: TaskId(9),
                    name: "task".to_string(),
                    state: TaskState::Done,
                    fields: BTreeMap::new(),
                    stats: TaskStats::default(),
                    warnings: Vec::new(),
                    created_at: Some(started_at),
                    dropped_at: Some(ended_at),
                }],
            )
            .expect("save tasks");
        store
            .save_span_batch(
                session_id,
                &[Span {
                    id: SpanId(4),
                    parent_id: None,
                    name: "span".to_string(),
                    target: "demo".to_string(),
                    level: SpanLevel::Info,
                    fields: BTreeMap::new(),
                    entered_at: Some(started_at),
                    exited_at: Some(ended_at),
                    busy_duration: DurationValue::from_millis(10),
                }],
            )
            .expect("save spans");
        store
            .save_resource_batch(
                session_id,
                &[Resource {
                    id: ResourceId(5),
                    kind: "timer".to_string(),
                    name: "interval".to_string(),
                    stats: ResourceStats::default(),
                    visibility: ResourceVisibility::Visible,
                }],
            )
            .expect("save resources");

        store.delete_session(session_id).expect("delete session");

        let connection = store.connection().expect("connection");
        for table in ["tasks", "spans", "resources"] {
            let count: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("count");
            assert_eq!(count, 0, "{table} rows should be deleted");
        }
    }
}
