use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::command;
use log::info;
use reqwest::multipart::{Form, Part};
use std::fs;
use chrono::Utc;

#[derive(Debug, Serialize, Deserialize)]
pub struct ArchiveResult {
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RestoreResult {
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArchivableMeeting {
    pub id: String,
    pub title: String,
    pub date: String,
    pub folder_path: String,
    pub is_archived: bool,
    pub file_size_mb: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct N8nArchiveResponse {
    #[serde(rename = "storedPath")]
    stored_path: Option<String>, // Made optional as we might not get it or need it with new flow
    #[serde(rename = "originalName")]
    original_name: Option<String>,
}

// Helper to update metadata.json
fn update_metadata(folder_path: &PathBuf, is_archived: bool, remote_folder_name: Option<String>) -> Result<(), String> {
    let metadata_path = folder_path.join("metadata.json");
    
    let mut metadata: serde_json::Value = if metadata_path.exists() {
        let content = fs::read_to_string(&metadata_path)
            .map_err(|e| format!("Failed to read metadata: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse metadata: {}", e))?
    } else {
        serde_json::json!({})
    };

    if let Some(obj) = metadata.as_object_mut() {
        obj.insert("is_archived".to_string(), serde_json::Value::Bool(is_archived));
        if is_archived {
            obj.insert("archived_at".to_string(), serde_json::Value::String(Utc::now().to_rfc3339()));
            if let Some(name) = remote_folder_name {
                obj.insert("remote_folder_name".to_string(), serde_json::Value::String(name));
            }
        } else {
            obj.remove("archived_at");
            obj.remove("remote_folder_name");
            // Also remove old key if exists
            obj.remove("remote_archive_path");
        }
    }

    fs::write(&metadata_path, serde_json::to_string_pretty(&metadata).unwrap())
        .map_err(|e| format!("Failed to save metadata: {}", e))?;
        
    Ok(())
}

/// Archive a meeting by sending audio.mp4 to Nextcloud via n8n webhook
#[command]
pub async fn archive_meeting(meeting_folder_path: String) -> Result<ArchiveResult, String> {
    info!("📦 Archiving meeting: {}", meeting_folder_path);
    
    let folder_path = PathBuf::from(&meeting_folder_path);
    let audio_path = folder_path.join("audio.mp4");
    
    if !audio_path.exists() {
        return Ok(ArchiveResult {
            success: false,
            error: Some("Audio file not found".to_string()),
        });
    }

    // Get webhook URL and API Key from env
    let webhook_url = std::env::var("MEETILY_ARCHIVE_WEBHOOK_URL")
        .map_err(|_| "MEETILY_ARCHIVE_WEBHOOK_URL not set in .env".to_string())?;
    let api_key = std::env::var("MEETILY_N8N_X_API_KEY")
        .map_err(|_| "MEETILY_N8N_X_API_KEY not set in .env".to_string())?;

    // Extract folder name
    let folder_name = folder_path.file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid folder path")?
        .to_string();

    // Read file
    let file_content = fs::read(&audio_path)
        .map_err(|e| format!("Failed to read audio file: {}", e))?;
    
    let part = Part::bytes(file_content)
        .file_name("audio.mp4")
        .mime_str("audio/mp4")
        .map_err(|e| format!("Failed to create multipart: {}", e))?;

    // Create multipart form with 'folder' and 'file'
    let form = Form::new()
        .text("folder", folder_name.clone())
        .part("file", part);

    let client = reqwest::Client::new();
    let response = client.post(&webhook_url)
        .header("X-API-Key", api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    if !response.status().is_success() {
        return Ok(ArchiveResult {
            success: false,
            error: Some(format!("Server returned error: {}", response.status())),
        });
    }

    // We don't strictly need the response body anymore if we trust the folder name convention
    // But let's try to parse it just in case, or ignore if it fails/is empty
    // info!("✅ Archived successfully. Remote folder: {}", folder_name);
    
    // Try to parse response just to log it, but don't fail if it's not JSON
    if let Ok(resp_json) = response.json::<N8nArchiveResponse>().await {
        info!("✅ Archived successfully. Remote path: {:?}", resp_json.stored_path);
    } else {
        info!("✅ Archived successfully (no JSON response). Remote folder: {}", folder_name);
    }

    // Update metadata with the folder name we used
    update_metadata(&folder_path, true, Some(folder_name))?;

    // Delete local audio file
    fs::remove_file(&audio_path)
        .map_err(|e| format!("Failed to delete local audio file: {}", e))?;

    Ok(ArchiveResult {
        success: true,
        error: None,
    })
}

/// Restore a meeting by downloading audio.mp4 from Nextcloud via n8n webhook
#[command]
pub async fn restore_meeting(meeting_folder_path: String) -> Result<RestoreResult, String> {
    info!("♻️ Restoring meeting: {}", meeting_folder_path);
    
    let folder_path = PathBuf::from(&meeting_folder_path);
    let audio_path = folder_path.join("audio.mp4");
    let metadata_path = folder_path.join("metadata.json");
    
    // Get webhook URL and API Key from env
    let webhook_url = std::env::var("MEETILY_RESTORE_WEBHOOK_URL")
        .map_err(|_| "MEETILY_RESTORE_WEBHOOK_URL not set in .env".to_string())?;
    let api_key = std::env::var("MEETILY_N8N_X_API_KEY")
        .map_err(|_| "MEETILY_N8N_X_API_KEY not set in .env".to_string())?;

    // Read metadata to get remote folder name
    let content = fs::read_to_string(&metadata_path)
        .map_err(|e| format!("Failed to read metadata: {}", e))?;
    let metadata: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse metadata: {}", e))?;
        
    // Use stored remote_folder_name, or fallback to current folder name
    let remote_folder_name = metadata.get("remote_folder_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            folder_path.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
        .ok_or("Could not determine remote folder name")?;

    // Prepare JSON body
    let body = serde_json::json!({
        "folder": remote_folder_name,
        "file": "audio.mp4"
    });

    // Call restore webhook (POST with JSON)
    let client = reqwest::Client::new();
    let response = client.post(&webhook_url)
        .header("X-API-Key", api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {}", e))?;

    if !response.status().is_success() {
        return Ok(RestoreResult {
            success: false,
            error: Some(format!("Server returned error: {}", response.status())),
        });
    }

    // Save file
    let bytes = response.bytes().await
        .map_err(|e| format!("Failed to get response bytes: {}", e))?;
        
    fs::write(&audio_path, bytes)
        .map_err(|e| format!("Failed to write audio file: {}", e))?;

    // Update metadata
    update_metadata(&folder_path, false, None)?;

    Ok(RestoreResult {
        success: true,
        error: None,
    })
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
            if path.is_dir() {
                let metadata_path = path.join("metadata.json");
                let audio_path = path.join("audio.mp4");
                
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
                        if let Some(d) = json.get("date").and_then(|v| v.as_str()) {
                            date = d.to_string();
                        } else {
                            // Try to parse date from folder name YYYY-MM-DD_...
                            let folder_name = path.file_name().unwrap_or_default().to_string_lossy();
                            if folder_name.len() >= 10 {
                                date = folder_name[0..10].to_string();
                            }
                        }
                    }
                }

                // Get file size (audio.mp4)
                let file_size_mb = if audio_path.exists() {
                    fs::metadata(&audio_path).map(|m| m.len() as f64 / 1024.0 / 1024.0).unwrap_or(0.0)
                } else {
                    0.0
                };

                meetings.push(ArchivableMeeting {
                    id: path.to_string_lossy().to_string(), // Use path as ID
                    title,
                    date,
                    folder_path: path.to_string_lossy().to_string(),
                    is_archived,
                    file_size_mb,
                });
            }
        }
    }
    
    // Sort by date desc
    meetings.sort_by(|a, b| b.date.cmp(&a.date));

    Ok(meetings)
}
