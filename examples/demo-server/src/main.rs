use std::sync::Arc;

use tokio::{
    sync::{mpsc, Mutex},
    time::{self, Duration},
};
use tracing::{debug, info, info_span, instrument, warn, Instrument};

#[tokio::main]
async fn main() {
    console_subscriber::init();

    let shared = Arc::new(Mutex::new(0_u64));
    let (tx, rx) = mpsc::channel::<u64>(64);

    tokio::spawn(producer(
        "fast-producer",
        tx.clone(),
        Duration::from_millis(250),
    ));
    tokio::spawn(producer(
        "slow-producer",
        tx.clone(),
        Duration::from_millis(900),
    ));
    tokio::spawn(channel_consumer("channel-consumer", rx));
    tokio::spawn(mutex_worker("mutex-worker-a", Arc::clone(&shared), 3));
    tokio::spawn(mutex_worker("mutex-worker-b", Arc::clone(&shared), 5));
    tokio::spawn(interval_reporter(Arc::clone(&shared)));
    drop(tx);

    info!("demo server running on Tokio console endpoint 127.0.0.1:6669");

    loop {
        time::sleep(Duration::from_secs(60)).await;
    }
}

#[instrument(name = "producer", skip_all, fields(task.name = name))]
async fn producer(name: &'static str, tx: mpsc::Sender<u64>, delay: Duration) {
    let mut counter = 0_u64;
    loop {
        let keep_running = async {
            time::sleep(delay).await;

            if tx.send(counter).await.is_err() {
                warn!(task = name, "consumer channel closed");
                return false;
            }

            debug!(task = name, counter, "sent value");
            true
        }
        .instrument(info_span!("produce-cycle", task = name, counter))
        .await;

        if !keep_running {
            break;
        }

        counter = counter.saturating_add(1);
    }
}

#[instrument(name = "channel_consumer", skip_all, fields(task.name = name))]
async fn channel_consumer(name: &'static str, mut rx: mpsc::Receiver<u64>) {
    while let Some(value) = rx.recv().await {
        async {
            time::sleep(Duration::from_millis(120)).await;
            info!(task = name, value, "processed channel message");
        }
        .instrument(info_span!("consume-value", task = name, value))
        .await;
    }
}

#[instrument(name = "mutex_worker", skip_all, fields(task.name = name))]
async fn mutex_worker(name: &'static str, shared: Arc<Mutex<u64>>, factor: u64) {
    loop {
        async {
            let mut guard = shared.lock().await;
            *guard = guard.saturating_add(factor);
            debug!(task = name, total = *guard, "updated shared state");
            time::sleep(Duration::from_millis(80)).await;
        }
        .instrument(info_span!("mutex-critical-section", task = name))
        .await;

        time::sleep(Duration::from_millis(450)).await;
    }
}

#[instrument(name = "interval_reporter", skip_all)]
async fn interval_reporter(shared: Arc<Mutex<u64>>) {
    let mut ticker = time::interval(Duration::from_secs(2));
    loop {
        ticker.tick().await;
        let snapshot = async { *shared.lock().await }
            .instrument(info_span!("report-snapshot"))
            .await;
        info!(snapshot, "current shared total");
    }
}
