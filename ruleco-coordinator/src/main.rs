//! Entry point, sets up CoordinatorApp

use ruleco_coordinator::app::CoordinatorApp;
use ruleco_coordinator::config::CoordinatorConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CoordinatorConfig::load();
    println!("Using namespace: {}", config.namespace);
    println!("Using timeout interval: {} seconds", config.timeout_interval);

    let mut app = CoordinatorApp::new(&config.namespace, None, Some(config.timeout_interval))?;
    app.run()?;
    Ok(())
}
