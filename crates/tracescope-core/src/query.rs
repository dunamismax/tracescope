//! Query helpers for sorting and filtering TraceScope data.

use crate::model::{Resource, Session, Task, TaskState};

/// Sort column for task tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskSortColumn {
    /// Sort by identifier.
    Id,
    /// Sort by task name.
    Name,
    /// Sort by task state.
    State,
    /// Sort by total duration.
    Total,
    /// Sort by busy duration.
    Busy,
    /// Sort by scheduled duration.
    Scheduled,
    /// Sort by idle duration.
    Idle,
    /// Sort by poll count.
    Polls,
    /// Sort by warning count.
    Warnings,
}

/// Query options for task lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskQuery {
    /// Lowercase substring filter applied to name and field values.
    pub filter: String,
    /// Sort column.
    pub sort_by: TaskSortColumn,
    /// Whether to reverse the sort order.
    pub descending: bool,
}

impl Default for TaskQuery {
    fn default() -> Self {
        Self {
            filter: String::new(),
            sort_by: TaskSortColumn::Total,
            descending: true,
        }
    }
}

/// Sort column for resource tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceSortColumn {
    /// Sort by identifier.
    Id,
    /// Sort by name.
    Name,
    /// Sort by kind.
    Kind,
    /// Sort by poll operation count.
    PollOps,
    /// Sort by ready count.
    Ready,
    /// Sort by pending count.
    Pending,
}

/// Query options for resource lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceQuery {
    /// Lowercase substring filter applied to resource name and kind.
    pub filter: String,
    /// Sort column.
    pub sort_by: ResourceSortColumn,
    /// Whether to reverse the sort order.
    pub descending: bool,
}

impl Default for ResourceQuery {
    fn default() -> Self {
        Self {
            filter: String::new(),
            sort_by: ResourceSortColumn::PollOps,
            descending: true,
        }
    }
}

/// Returns filtered and sorted task data.
#[must_use]
pub fn query_tasks(tasks: &[Task], query: &TaskQuery) -> Vec<Task> {
    let filter = query.filter.to_lowercase();
    let mut rows: Vec<Task> = tasks
        .iter()
        .filter(|task| {
            if filter.is_empty() {
                return true;
            }

            task.name.to_lowercase().contains(&filter)
                || task.fields.iter().any(|(key, value)| {
                    key.to_lowercase().contains(&filter)
                        || value.as_display().to_lowercase().contains(&filter)
                })
        })
        .cloned()
        .collect();

    rows.sort_by(|left, right| {
        let order = match query.sort_by {
            TaskSortColumn::Id => left.id.0.cmp(&right.id.0),
            TaskSortColumn::Name => left.name.cmp(&right.name),
            TaskSortColumn::State => state_rank(left.state).cmp(&state_rank(right.state)),
            TaskSortColumn::Total => left.stats.total_duration.cmp(&right.stats.total_duration),
            TaskSortColumn::Busy => left.stats.busy_duration.cmp(&right.stats.busy_duration),
            TaskSortColumn::Scheduled => left
                .stats
                .scheduled_duration
                .cmp(&right.stats.scheduled_duration),
            TaskSortColumn::Idle => left.stats.idle_duration.cmp(&right.stats.idle_duration),
            TaskSortColumn::Polls => left.stats.poll_count.cmp(&right.stats.poll_count),
            TaskSortColumn::Warnings => left.warnings.len().cmp(&right.warnings.len()),
        };

        if query.descending {
            order.reverse()
        } else {
            order
        }
    });

    rows
}

/// Returns filtered and sorted resource data.
#[must_use]
pub fn query_resources(resources: &[Resource], query: &ResourceQuery) -> Vec<Resource> {
    let filter = query.filter.to_lowercase();
    let mut rows: Vec<Resource> = resources
        .iter()
        .filter(|resource| {
            filter.is_empty()
                || resource.name.to_lowercase().contains(&filter)
                || resource.kind.to_lowercase().contains(&filter)
        })
        .cloned()
        .collect();

    rows.sort_by(|left, right| {
        let order = match query.sort_by {
            ResourceSortColumn::Id => left.id.0.cmp(&right.id.0),
            ResourceSortColumn::Name => left.name.cmp(&right.name),
            ResourceSortColumn::Kind => left.kind.cmp(&right.kind),
            ResourceSortColumn::PollOps => left.stats.poll_op_count.cmp(&right.stats.poll_op_count),
            ResourceSortColumn::Ready => left.stats.ready_count.cmp(&right.stats.ready_count),
            ResourceSortColumn::Pending => left.stats.pending_count.cmp(&right.stats.pending_count),
        };

        if query.descending {
            order.reverse()
        } else {
            order
        }
    });

    rows
}

/// Filters sessions by name or target address.
#[must_use]
pub fn filter_sessions(sessions: &[Session], filter: &str) -> Vec<Session> {
    let needle = filter.to_lowercase();
    sessions
        .iter()
        .filter(|session| {
            needle.is_empty()
                || session.name.to_lowercase().contains(&needle)
                || session.target_address.to_lowercase().contains(&needle)
        })
        .cloned()
        .collect()
}

fn state_rank(state: TaskState) -> u8 {
    match state {
        TaskState::Running => 0,
        TaskState::Scheduled => 1,
        TaskState::Idle => 2,
        TaskState::Done => 3,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{TimeZone, Utc};

    use crate::model::{
        DurationValue, FieldValue, ResourceId, ResourceStats, ResourceVisibility, Session,
        SessionId, Task, TaskId, TaskState, TaskStats, TaskWarning, WarningKind,
    };

    use super::{
        filter_sessions, query_resources, query_tasks, ResourceQuery, ResourceSortColumn,
        TaskQuery, TaskSortColumn,
    };

    #[test]
    fn query_tasks_filters_fields_and_sorts_by_warnings() {
        let tasks = vec![
            task_fixture(
                1,
                "alpha-worker",
                TaskState::Idle,
                &[("queue", FieldValue::String(String::from("critical")))],
                25,
                1,
            ),
            task_fixture(
                2,
                "beta-worker",
                TaskState::Running,
                &[("queue", FieldValue::String(String::from("bulk")))],
                75,
                2,
            ),
            task_fixture(
                3,
                "gamma-worker",
                TaskState::Done,
                &[("queue", FieldValue::String(String::from("critical")))],
                50,
                0,
            ),
        ];

        let rows = query_tasks(
            &tasks,
            &TaskQuery {
                filter: String::from("critical"),
                sort_by: TaskSortColumn::Warnings,
                descending: true,
            },
        );

        assert_eq!(
            rows.iter().map(|task| task.id.0).collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(rows[0].warnings.len(), 1);
        assert!(rows[0].fields.contains_key("queue"));
    }

    #[test]
    fn query_resources_filters_and_sorts_by_ready_count() {
        let resources = vec![
            resource_fixture(1, "timer", "interval-a", 2, 1, 1),
            resource_fixture(2, "channel", "mpsc", 4, 3, 1),
            resource_fixture(3, "timer", "interval-b", 3, 2, 1),
        ];

        let rows = query_resources(
            &resources,
            &ResourceQuery {
                filter: String::from("timer"),
                sort_by: ResourceSortColumn::Ready,
                descending: true,
            },
        );

        assert_eq!(
            rows.iter()
                .map(|resource| resource.id.0)
                .collect::<Vec<_>>(),
            vec![3, 1]
        );
        assert!(rows.iter().all(|resource| resource.kind == "timer"));
    }

    #[test]
    fn filter_sessions_matches_name_and_target_case_insensitively() {
        let started_at = Utc.with_ymd_and_hms(2026, 3, 20, 12, 0, 0).unwrap();
        let sessions = vec![
            Session {
                id: SessionId(1),
                name: String::from("Morning Capture"),
                target_address: String::from("http://127.0.0.1:6669"),
                started_at,
                ended_at: None,
                metadata: BTreeMap::new(),
            },
            Session {
                id: SessionId(2),
                name: String::from("Nightly Replay"),
                target_address: String::from("http://demo.internal:7000"),
                started_at,
                ended_at: None,
                metadata: BTreeMap::new(),
            },
        ];

        assert_eq!(filter_sessions(&sessions, "morning").len(), 1);
        assert_eq!(filter_sessions(&sessions, "DEMO.INTERNAL").len(), 1);
        assert_eq!(filter_sessions(&sessions, "missing").len(), 0);
    }

    fn task_fixture(
        id: u64,
        name: &str,
        state: TaskState,
        fields: &[(&str, FieldValue)],
        total_duration_millis: u64,
        warning_count: usize,
    ) -> Task {
        let warnings = (0..warning_count)
            .map(|index| TaskWarning {
                kind: if index % 2 == 0 {
                    WarningKind::LongPoll
                } else {
                    WarningKind::SelfWake
                },
                message: format!("warning-{index}"),
            })
            .collect();

        Task {
            id: TaskId(id),
            name: name.to_string(),
            state,
            fields: fields
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
            stats: TaskStats {
                total_duration: DurationValue::from_millis(total_duration_millis),
                ..TaskStats::default()
            },
            warnings,
            created_at: None,
            dropped_at: None,
        }
    }

    fn resource_fixture(
        id: u64,
        kind: &str,
        name: &str,
        poll_ops: u64,
        ready: u64,
        pending: u64,
    ) -> crate::model::Resource {
        crate::model::Resource {
            id: ResourceId(id),
            kind: kind.to_string(),
            name: name.to_string(),
            stats: ResourceStats {
                poll_op_count: poll_ops,
                ready_count: ready,
                pending_count: pending,
                ..ResourceStats::default()
            },
            visibility: ResourceVisibility::Visible,
        }
    }
}
