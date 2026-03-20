use std::{path::PathBuf, sync::mpsc, thread};

use anyhow::Context;
use clap::Parser;
use eframe::{egui, NativeOptions};
use tokio_util::sync::CancellationToken;
use tracescope_core::{Collector, CollectorCommand, CollectorEvent, ConnectionState};
use tracescope_ui::{TraceScopeApp, TraceScopeAppConfig};
use tracing_subscriber::EnvFilter;

/// Command line arguments for TraceScope.
#[derive(Debug, Parser)]
#[command(author, version, about = "Graphical flight recorder for async Rust")]
struct Args {
    /// Tokio console gRPC target.
    #[arg(long, default_value = "127.0.0.1:6669")]
    target: String,
    /// Directory for TraceScope data.
    #[arg(long)]
    data_dir: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                EnvFilter::new("info,tracescope_core=debug,tracescope_app=debug")
            }),
        )
        .init();

    let args = Args::parse();
    let data_dir = args.data_dir.unwrap_or_else(default_data_dir);
    let initial_target_address = normalize_target(&args.target);

    let (command_tx, command_rx) = mpsc::channel::<CollectorCommand>();
    let (event_tx, event_rx) = mpsc::channel::<CollectorEvent>();
    let _collector_thread = spawn_collector_manager(command_rx, event_tx);

    let native_options = NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0]),
        ..Default::default()
    };

    eframe::run_native(
        "TraceScope",
        native_options,
        Box::new(move |_cc| {
            Ok(Box::new(TraceScopeApp::new(TraceScopeAppConfig {
                initial_target_address,
                data_dir,
                command_tx,
                event_rx,
            })) as Box<dyn eframe::App>)
        }),
    )
    .context("failed to launch TraceScope window")?;

    Ok(())
}

fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".tracescope")
}

fn normalize_target(input: &str) -> String {
    if input.starts_with("http://") || input.starts_with("https://") {
        input.to_string()
    } else {
        format!("http://{input}")
    }
}

fn spawn_collector_manager(
    command_rx: mpsc::Receiver<CollectorCommand>,
    event_tx: mpsc::Sender<CollectorEvent>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("tracescope-runtime")
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = event_tx.send(CollectorEvent::Status(ConnectionState::Error {
                    target_address: String::from("runtime"),
                    message: error.to_string(),
                }));
                return;
            }
        };

        struct ActiveCollector {
            shutdown: CancellationToken,
            handle: tokio::task::JoinHandle<()>,
        }

        let mut active: Option<ActiveCollector> = None;

        while let Ok(command) = command_rx.recv() {
            match command {
                CollectorCommand::Connect { target_address } => {
                    if let Some(active_collector) = active.take() {
                        active_collector.shutdown.cancel();
                        let _ = runtime.block_on(active_collector.handle);
                    }

                    let shutdown = CancellationToken::new();
                    let collector = Collector::new(target_address.clone());
                    let event_tx_clone = event_tx.clone();
                    let shutdown_clone = shutdown.clone();

                    let _ = event_tx.send(CollectorEvent::Status(ConnectionState::Connecting {
                        target_address: target_address.clone(),
                    }));

                    let handle = runtime.spawn(async move {
                        if let Err(error) =
                            collector.run(event_tx_clone.clone(), shutdown_clone).await
                        {
                            let _ = event_tx_clone.send(CollectorEvent::Status(
                                ConnectionState::Error {
                                    target_address,
                                    message: error.to_string(),
                                },
                            ));
                        }
                    });

                    active = Some(ActiveCollector { shutdown, handle });
                }
                CollectorCommand::Disconnect => {
                    if let Some(active_collector) = active.take() {
                        active_collector.shutdown.cancel();
                        let _ = runtime.block_on(active_collector.handle);
                    } else {
                        let _ =
                            event_tx.send(CollectorEvent::Status(ConnectionState::Disconnected));
                    }
                }
            }
        }

        if let Some(active_collector) = active.take() {
            active_collector.shutdown.cancel();
            let _ = runtime.block_on(active_collector.handle);
        }
    })
}
