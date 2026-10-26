//! Tests the actual production cores without native audio/GUI dependencies.
#[path = "../frontend/src-tauri/src/meeting_files.rs"]
pub mod meeting_files;
#[path = "../frontend/src-tauri/src/archive_core.rs"]
pub mod archive_core;
#[path = "../frontend/src-tauri/src/summary/checkpoints.rs"]
pub mod checkpoints;
#[path = "../frontend/src-tauri/src/summary/llm_client.rs"]
pub mod llm_client;
#[path = "../frontend/src-tauri/src/summary/processor.rs"]
pub mod processor;
#[path = "../frontend/src-tauri/src/summary/templates/mod.rs"]
pub mod templates;
#[path = "../frontend/src-tauri/src/summary/service.rs"]
pub mod service;
#[path = "../frontend/src-tauri/src/ollama/metadata.rs"]
pub mod metadata;
#[path = "../frontend/src-tauri/src/database/models.rs"]
pub mod models;
#[path = "../frontend/src-tauri/src/database/repositories/meeting.rs"]
pub mod meeting_repository;
#[path = "../frontend/src-tauri/src/database/repositories/summary.rs"]
pub mod summary_repository;
#[path = "../frontend/src-tauri/src/database/repositories/setting.rs"]
pub mod setting_repository;
pub mod summary { pub use crate::{checkpoints, llm_client, processor, templates, service}; }
pub mod ollama { pub use crate::metadata; }
pub mod database {
    pub use crate::models;
    pub mod repositories {
        pub use crate::meeting_repository as meeting;
        pub use crate::summary_repository as summary;
        pub use crate::setting_repository as setting;
    }
}
// Boundary DTO fixtures, matching the existing native API. Core/repositories are unmodified.
pub mod api {
    #[derive(serde::Serialize)]
    pub struct MeetingDetails {
        pub id: String, pub title: String, pub created_at: String, pub updated_at: String,
        pub transcripts: Vec<MeetingTranscript>, pub improved_transcript: Option<String>, pub folder_path: Option<String>,
    }
    #[derive(serde::Serialize)]
    pub struct MeetingTranscript {
        pub id: String, pub text: String, pub timestamp: String,
        pub audio_start_time: Option<f64>, pub audio_end_time: Option<f64>, pub duration: Option<f64>,
    }
}

#[cfg(test)]
mod integration;
