//! Entry point, sets up CoordinatorApp

use ruleco_coordinator::app::CoordinatorApp;
use ruleco_coordinator::config::CoordinatorConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CoordinatorConfig::load();
    println!("Using namespace: {}", config.namespace);
    println!(
        "Using timeout interval: {} seconds",
        config.timeout_interval
    );
    println!("Binding to: {}", config.bind_address);
    println!("Public address: {}", config.public_address);

    let mut app = CoordinatorApp::new_with_addresses(
        &config.namespace,
        &config.bind_address,
        &config.public_address,
        Some(config.timeout_interval),
    )?;
    app.run()?;
    Ok(())
}
