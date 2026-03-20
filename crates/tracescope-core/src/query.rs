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
