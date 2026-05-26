use anyhow::{Context, Result};
use clap::Parser;
use crossbeam_channel::{bounded, Receiver};
use log::info;
use ruleco_coordinator::adapters::{DataPublisherAdapter, SyncPublisher};
use ruleco_coordinator::app::CoordinatorApp;
use ruleco_coordinator::config::CoordinatorConfig;
use ruleco_coordinator::logging::{LoggingConfig, init_logger};
use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::thread;

#[derive(Parser, Debug)]
#[command(name = "ruleco-coordinator")]
#[command(about = "RuLECO Coordinator - Routes messages between components")]
struct Args {
    #[arg(short = 'n', long = "namespace")]
    namespace: Option<String>,

    #[arg(short = 'p', long = "port")]
    port: Option<u16>,

    #[arg(short = 'b', long = "bind-address")]
    bind_address: Option<String>,

    #[arg(short = 'a', long = "public-address")]
    public_address: Option<String>,

    #[arg(short = 't', long = "timeout-interval")]
    timeout_interval: Option<u64>,

    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    #[arg(long = "data-publisher-addr")]
    data_publisher_addr: Option<String>,

    #[arg(long = "log-publish-level", value_name = "LEVEL")]
    log_publish_level: Option<String>,
}

fn setup_signal_handler() -> Result<Receiver<i32>> {
    let (tx, rx) = bounded(2);
    let mut signals = Signals::new([SIGINT, SIGTERM])
        .context("Failed to create signal handler")?;
    thread::spawn(move || {
        for sig in signals.forever() {
            let _ = tx.send(sig);
        }
    });
    Ok(rx)
}

fn run() -> Result<()> {
    let args = Args::parse();

    let config = CoordinatorConfig::load()
        .apply_cli_overrides(
            args.namespace,
            args.port,
            args.bind_address,
            args.public_address,
            args.timeout_interval,
            args.data_publisher_addr,
            args.log_publish_level,
        );

    let stderr_level = if args.verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    let logging_config = LoggingConfig {
        stderr_level,
        publish_level: config.log_publish_level,
        data_publisher_addr: config.data_publisher_addr.clone(),
        topic: format!("{}.Coordinator", config.namespace),
    };

    init_logger(&logging_config, |topic, addr| {
        DataPublisherAdapter::new(topic, addr)
            .map(SyncPublisher::new)
            .map_err(|e| e.to_string())
    })?;

    let signal_rx = setup_signal_handler()
        .context("Failed to setup signal handlers")?;

    info!(
        "Starting coordinator with namespace={}, bind={}, public={}, timeout={}s",
        config.namespace,
        config.bind_address,
        config.public_address,
        config.timeout_interval
    );

    let mut app = CoordinatorApp::new_with_addresses(
        &config.namespace,
        &config.bind_address,
        &config.public_address,
        Some(config.timeout_interval),
    )
    .context("Failed to initialize coordinator")?;

    let (shutdown_tx, shutdown_rx) = bounded(1);

    thread::spawn(move || {
        if let Ok(signal) = signal_rx.recv() {
            info!("Received signal {signal}");
            let _ = shutdown_tx.send(());
        }
    });

    app.run(shutdown_rx)
        .context("Coordinator run loop failed")?;

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e:?}");
        std::process::exit(1);
    }
}
