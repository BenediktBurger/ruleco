//! Entry point, sets up CoordinatorApp

use ruleco_coordinator::app::CoordinatorApp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create and run the coordinator app
    let mut app = CoordinatorApp::new("COORDINATOR".to_string(), None)?;
    app.run()?;
    Ok(())
}
