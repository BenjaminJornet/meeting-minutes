#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use log;
use env_logger;

fn main() {
    // Load environment variables from .env file (if it exists)
    // This allows users to configure MEETILY_BACKEND_URL, MEETILY_OLLAMA_URL, etc.
    // without modifying source code
    if let Err(e) = dotenvy::dotenv() {
        // .env file is optional - don't fail if it doesn't exist
        if !e.to_string().contains("not found") {
            eprintln!("Warning: Error loading .env file: {}", e);
        }
    }

    std::env::set_var("RUST_LOG", "info");
    env_logger::init();

    // Async logger will be initialized lazily when first needed (after Tauri runtime starts)
    log::info!("Starting application...");
    log::info!("Backend URL: {}", std::env::var("MEETILY_BACKEND_URL").unwrap_or_else(|_| "http://localhost:5167 (default)".to_string()));
    log::info!("Ollama URL: {}", std::env::var("MEETILY_OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434 (default)".to_string()));
    
    app_lib::run();
}
