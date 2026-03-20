use std::collections::BTreeMap;

use chrono::{Duration, TimeZone, Utc};
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use tempfile::tempdir;
use tracescope_core::{
    query::{
        query_resources, query_tasks, ResourceQuery, ResourceSortColumn, TaskQuery, TaskSortColumn,
    },
    DurationValue, FieldValue, Resource, ResourceId, ResourceStats, ResourceVisibility, Session,
    SessionDraft, SessionId, SessionStore, Span, SpanId, SpanLevel, Task, TaskId, TaskState,
    TaskStats,
};

fn snapshot_save_and_load_benches(criterion: &mut Criterion) {
    let fixture = bench_fixture();

    let mut group = criterion.benchmark_group("snapshot_import");
    group.bench_function(
        "save_session_snapshot_512_tasks_1024_spans_256_resources",
        |bench| {
            bench.iter_batched(
                || {
                    let temp = tempdir().expect("tempdir");
                    let store = SessionStore::open_in_dir(temp.path()).expect("store");
                    (temp, store)
                },
                |(_temp, store)| {
                    black_box(
                        store
                            .save_session_snapshot(
                                &fixture.draft,
                                &fixture.tasks,
                                &fixture.spans,
                                &fixture.resources,
                            )
                            .expect("save snapshot"),
                    );
                },
                BatchSize::SmallInput,
            );
        },
    );

    group.bench_function("load_session_512_tasks_1024_spans_256_resources", |bench| {
        bench.iter_batched(
            || {
                let temp = tempdir().expect("tempdir");
                let store = SessionStore::open_in_dir(temp.path()).expect("store");
                let session_id = store
                    .save_session_snapshot(
                        &fixture.draft,
                        &fixture.tasks,
                        &fixture.spans,
                        &fixture.resources,
                    )
                    .expect("seed snapshot");
                (temp, store, session_id)
            },
            |(_temp, store, session_id)| {
                black_box(store.load_session(session_id).expect("load session"));
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

fn query_hot_path_benches(criterion: &mut Criterion) {
    let fixture = bench_fixture();
    let task_query = TaskQuery {
        filter: String::from("critical"),
        sort_by: TaskSortColumn::Warnings,
        descending: true,
    };
    let resource_query = ResourceQuery {
        filter: String::from("timer"),
        sort_by: ResourceSortColumn::Ready,
        descending: true,
    };

    let mut group = criterion.benchmark_group("query_hot_paths");
    group.bench_function("query_tasks_filtered_sorted_512_rows", |bench| {
        bench.iter(|| {
            black_box(query_tasks(
                black_box(&fixture.tasks),
                black_box(&task_query),
            ))
        });
    });
    group.bench_function("query_resources_filtered_sorted_256_rows", |bench| {
        bench.iter(|| {
            black_box(query_resources(
                black_box(&fixture.resources),
                black_box(&resource_query),
            ))
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    snapshot_save_and_load_benches,
    query_hot_path_benches
);
criterion_main!(benches);

struct BenchFixture {
    draft: SessionDraft,
    tasks: Vec<Task>,
    spans: Vec<Span>,
    resources: Vec<Resource>,
    _sessions: Vec<Session>,
}

fn bench_fixture() -> BenchFixture {
    let started_at = Utc.with_ymd_and_hms(2026, 3, 20, 12, 0, 0).unwrap();
    let ended_at = started_at + Duration::seconds(30);

    BenchFixture {
        draft: SessionDraft {
            name: String::from("criterion snapshot"),
            target_address: String::from("http://127.0.0.1:6669"),
            started_at,
            ended_at: Some(ended_at),
            metadata: BTreeMap::from([
                (String::from("suite"), String::from("criterion")),
                (String::from("scenario"), String::from("snapshot-import")),
            ]),
        },
        tasks: (0..512)
            .map(|index| task_fixture(index, started_at, ended_at))
            .collect(),
        spans: (0..1024)
            .map(|index| span_fixture(index, started_at, ended_at))
            .collect(),
        resources: (0..256)
            .map(|index| resource_fixture(index, started_at, ended_at))
            .collect(),
        _sessions: (0..64)
            .map(|index| Session {
                id: SessionId(index + 1),
                name: format!("session-{index}"),
                target_address: String::from("http://127.0.0.1:6669"),
                started_at,
                ended_at: Some(ended_at),
                metadata: BTreeMap::new(),
            })
            .collect(),
    }
}

fn task_fixture(
    index: u64,
    started_at: chrono::DateTime<Utc>,
    ended_at: chrono::DateTime<Utc>,
) -> Task {
    let total_millis = 100 + index;
    let busy_millis = 10 + (index % 25);
    let scheduled_millis = 5 + (index % 10);
    let idle_millis = total_millis - busy_millis - scheduled_millis;
    let mut task = Task {
        id: TaskId(index + 1),
        name: format!("worker-{index}"),
        state: if index % 7 == 0 {
            TaskState::Running
        } else if index % 5 == 0 {
            TaskState::Scheduled
        } else {
            TaskState::Idle
        },
        fields: BTreeMap::from([
            (
                String::from("queue"),
                FieldValue::String(if index % 3 == 0 {
                    String::from("critical")
                } else {
                    String::from("bulk")
                }),
            ),
            (String::from("shard"), FieldValue::U64(index % 16)),
        ]),
        stats: TaskStats {
            poll_count: 10 + index,
            wake_count: 20 + index,
            self_wake_count: index % 4,
            busy_duration: DurationValue::from_millis(busy_millis),
            scheduled_duration: DurationValue::from_millis(scheduled_millis),
            idle_duration: DurationValue::from_millis(idle_millis),
            total_duration: DurationValue::from_millis(total_millis),
        },
        warnings: Vec::new(),
        created_at: Some(started_at),
        dropped_at: Some(ended_at),
    };
    task.refresh_warnings();
    task
}

fn span_fixture(
    index: u64,
    started_at: chrono::DateTime<Utc>,
    ended_at: chrono::DateTime<Utc>,
) -> Span {
    Span {
        id: SpanId(index + 1),
        parent_id: (index > 0).then_some(SpanId(index)),
        name: format!("span-{index}"),
        target: if index % 2 == 0 {
            String::from("tokio::task")
        } else {
            String::from("tokio::time")
        },
        level: match index % 3 {
            0 => SpanLevel::Info,
            1 => SpanLevel::Debug,
            _ => SpanLevel::Trace,
        },
        fields: BTreeMap::from([(
            String::from("worker"),
            FieldValue::String(format!("worker-{}", index % 64)),
        )]),
        entered_at: Some(started_at),
        exited_at: Some(ended_at),
        busy_duration: DurationValue::from_millis(1 + (index % 250)),
    }
}

fn resource_fixture(
    index: u64,
    started_at: chrono::DateTime<Utc>,
    ended_at: chrono::DateTime<Utc>,
) -> Resource {
    Resource {
        id: ResourceId(index + 1),
        kind: if index % 3 == 0 {
            String::from("timer")
        } else {
            String::from("channel")
        },
        name: format!("resource-{index}"),
        stats: ResourceStats {
            created_at: Some(started_at),
            dropped_at: Some(ended_at),
            attributes: BTreeMap::from([(String::from("owner"), format!("worker-{}", index % 32))]),
            poll_op_count: 40 + index,
            ready_count: 10 + (index % 20),
            pending_count: 5 + (index % 10),
        },
        visibility: if index % 4 == 0 {
            ResourceVisibility::Internal
        } else {
            ResourceVisibility::Visible
        },
    }
}
