// Test config loading
use std::path::PathBuf;

fn main() {
    let project_dir = std::env::current_dir().unwrap();
    println!("Current directory: {:?}", project_dir);
    
    // Check if config file exists
    let config_path = PathBuf::from("ruleco.toml");
    if config_path.exists() {
        println!("Config file found at: {:?}", config_path);
        let content = std::fs::read_to_string(&config_path).unwrap();
        println!("Config content:\n{}", content);
    } else {
        println!("Config file not found");
    }
}
