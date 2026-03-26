#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use log;
use env_logger;
use std::path::PathBuf;

/// Try to load .env from multiple locations (prioritizing project root)
fn load_env_file() {
    // List of possible .env locations to try (in priority order)
    // Project root .env is checked first for centralized configuration
    let possible_paths: Vec<PathBuf> = vec![
        // 1. Project root (centralized config) - highest priority
        PathBuf::from("../../.env"),
        // 1b. Frontend root .env (common dev setup for this repo)
        PathBuf::from("../.env"),
        // 2. Current directory (dev mode)
        PathBuf::from(".env"),
        // 2b. Tauri crate sibling frontend/.env when launched from repo root or src-tauri
        PathBuf::from("frontend/.env"),
        // 3. Executable's directory (bundled app)
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join(".env")))
            .unwrap_or_default(),
        // 4. macOS: Inside the app bundle's Resources
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("../Resources/.env")))
            .unwrap_or_default(),
        // 5. User's config directory for Meetily
        dirs::config_dir()
            .map(|p| p.join("Meetily/.env"))
            .unwrap_or_default(),
        // 6. User's home directory
        dirs::home_dir()
            .map(|p| p.join(".meetily.env"))
            .unwrap_or_default(),
    ];

    for path in possible_paths {
        if path.exists() {
            match dotenvy::from_path(&path) {
                Ok(_) => {
                    eprintln!("✅ Loaded .env from: {:?}", path);
                    // Continue to load other .env files (e.g. frontend/.env) to fill in missing vars
                }
                Err(e) => {
                    eprintln!("⚠️ Failed to load .env from {:?}: {}", path, e);
                }
            }
        }
    }
}

fn main() {
    // Load environment variables from .env file (if it exists)
    // This allows users to configure MEETILY_BACKEND_URL, MEETILY_OLLAMA_URL, etc.
    // without modifying source code
    load_env_file();

    std::env::set_var("RUST_LOG", "info");
    env_logger::init();

    // Async logger will be initialized lazily when first needed (after Tauri runtime starts)
    log::info!("Starting application...");
    log::info!("Backend URL: {}", std::env::var("MEETILY_BACKEND_URL").unwrap_or_else(|_| "http://localhost:5167 (default)".to_string()));
    log::info!("Ollama URL: {}", std::env::var("MEETILY_OLLAMA_URL").unwrap_or_else(|_| "http://localhost:11434 (default)".to_string()));
    log::info!("Whisper URL: {}", std::env::var("MEETILY_WHISPER_URL").unwrap_or_else(|_| "not set (local mode)".to_string()));
    log::info!("Language: {}", std::env::var("MEETILY_LANGUAGE").unwrap_or_else(|_| "en (default)".to_string()));
    
    app_lib::run();
}
