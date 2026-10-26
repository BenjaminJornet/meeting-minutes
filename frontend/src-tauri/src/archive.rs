use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}};
use tauri::command;
use regex::Regex;
use crate::archive_core::{ArchiveClient, ArchiveInventory, inventory};

#[derive(Debug, Serialize, Deserialize)]
pub struct ArchiveResult { pub success: bool, pub error: Option<String> }
pub type RestoreResult = ArchiveResult;
#[derive(Debug, Serialize, Deserialize)]
pub struct ArchivableMeeting {
    pub id: String, pub title: String, pub date: String, pub folder_path: String,
    pub is_archived: bool, pub file_size_mb: f64,
    pub archive_state: Option<String>, pub residual_audio_count: usize,
}

// Archive operations are limited to app-owned recording folders.
async fn owned_folder(path: &str) -> Result<PathBuf, String> {
    let root = crate::audio::recording_preferences::get_default_recordings_folder();
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let input = Path::new(path);
    if fs::symlink_metadata(input).map_err(|e| e.to_string())?.file_type().is_symlink() {
        return Err("meeting_path_invalid: dossier symbolique interdit".into());
    }
    let folder = input.canonicalize().map_err(|e| e.to_string())?;
    if folder.parent() != Some(root.as_path()) || !folder.join("metadata.json").is_file() {
        return Err("meeting_path_invalid: dossier hors des enregistrements".into());
    }
    if crate::audio::recording_commands::is_recording().await {
        if crate::audio::recording_commands::get_meeting_folder_path().await?
            .is_some_and(|p| PathBuf::from(p).canonicalize().ok().as_ref() == Some(&folder)) {
            return Err("meeting_busy: enregistrement en cours".into());
        }
    }
    Ok(folder)
}

#[command]
pub async fn archive_meeting(meeting_folder_path: String) -> Result<ArchiveResult, String> {
    let folder = owned_folder(&meeting_folder_path).await?;
    let result = ArchiveClient::from_env()?.archive(&folder).await;
    Ok(ArchiveResult { success: result.is_ok(), error: result.err() })
}

#[command]
pub async fn restore_meeting(meeting_folder_path: String) -> Result<RestoreResult, String> {
    let folder = owned_folder(&meeting_folder_path).await?;
    let result = ArchiveClient::from_env()?.restore(&folder).await;
    Ok(ArchiveResult { success: result.is_ok(), error: result.err() })
}

#[command]
pub async fn get_archive_audio_inventory(meeting_folder_path: String) -> Result<ArchiveInventory, String> {
    inventory(&owned_folder(&meeting_folder_path).await?)
}

/// List meetings eligible for archiving (> 1 month old)
#[command]
pub async fn get_archivable_meetings() -> Result<Vec<ArchivableMeeting>, String> {
    let recordings_dir = crate::audio::recording_preferences::get_default_recordings_folder_path()
        .await
        .map_err(|e| format!("Failed to get recordings dir: {}", e))?;
        
    let mut meetings = Vec::new();

    if let Ok(entries) = fs::read_dir(&recordings_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let metadata_path = path.join("metadata.json");

                
                // Check if it's a valid meeting folder (has metadata)
                if !metadata_path.exists() {
                    continue;
                }

                // Read metadata
                let mut is_archived = false;
                let mut title = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let mut date = "Unknown".to_string();
                
                if let Ok(content) = fs::read_to_string(&metadata_path) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        is_archived = json.get("is_archived").and_then(|v| v.as_bool()).unwrap_or(false);
                        if let Some(t) = json.get("title").and_then(|v| v.as_str()) {
                            title = t.to_string();
                        }
                        // Try created_at first (actual field in MeetingMetadata), then date
                        if let Some(d) = json.get("created_at").and_then(|v| v.as_str()) {
                            // created_at is an ISO timestamp like "2025-01-15T14:30:00Z"
                            // Extract just the date part YYYY-MM-DD
                            date = d.chars().take(10).collect();
                        } else if let Some(d) = json.get("date").and_then(|v| v.as_str()) {
                            date = d.to_string();
                        } else {
                            // Try to extract date from folder name using regex
                            // Folder format is: MeetingName_YYYY-MM-DD_HH-MM
                            let folder_name = path.file_name().unwrap_or_default().to_string_lossy();
                            if let Ok(re) = Regex::new(r"(\d{4}-\d{2}-\d{2})") {
                                if let Some(caps) = re.captures(&folder_name) {
                                    date = caps[1].to_string();
                                }
                            }
                        }
                    }
                }

                let audio = inventory(&path)?;
                let archive_state = crate::archive_core::read_metadata(&path).ok()
                    .and_then(|v| v["archive_state"].as_str().map(str::to_owned));
                meetings.push(ArchivableMeeting {
                    id: path.to_string_lossy().to_string(), // Use path as ID
                    title,
                    date,
                    folder_path: path.to_string_lossy().to_string(),
                    is_archived,
                    file_size_mb: audio.bytes as f64 / 1024.0 / 1024.0,
                    archive_state,
                    residual_audio_count: audio.files.len(),
                });
            }
        }
    }
    
    // Sort by date desc
    meetings.sort_by(|a, b| b.date.cmp(&a.date));

    Ok(meetings)
}
