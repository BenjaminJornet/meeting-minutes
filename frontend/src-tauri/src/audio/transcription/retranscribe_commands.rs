// audio/transcription/retranscribe_commands.rs
//
// Commands for re-transcribing audio files with a remote whisper server
// for higher quality transcripts after recording.

use chrono::Utc;
use log::{error, info, warn};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::command;

/// Result of a re-transcription operation
#[derive(Debug, Serialize, Deserialize)]
pub struct RetranscribeResult {
    pub success: bool,
    pub transcript: Option<String>,
    pub error: Option<String>,
    pub audio_duration_secs: Option<f64>,
    pub processing_time_secs: f64,
}

/// Response from the enhanced transcription service
#[derive(Debug, Deserialize)]
struct EnhancedJobResponse {
    job_id: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct EnhancedStatusResponse {
    status: String,
    progress: Option<i32>,
    result: Option<EnhancedResult>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EnhancedResult {
    segments: Vec<TranscriptionSegment>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TranscriptionSegment {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    speaker: Option<String>,
    #[serde(default)]
    start: Option<f64>,
    #[serde(default)]
    end: Option<f64>,
    #[serde(default)]
    words: Option<Vec<Word>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Word {
    word: String,
    start: f64,
    end: f64,
    probability: f64,
}

/// Re-transcribe an audio file using the remote whisper server
/// This provides higher quality transcription using a GPU server
#[command]
pub async fn retranscribe_audio_file(
    audio_path: String,
    remote_url: Option<String>,
    language: Option<String>,
) -> Result<RetranscribeResult, String> {
    let start_time = std::time::Instant::now();
    
    info!("🔄 Starting enhanced re-transcription of: {}", audio_path);
    
    // Use meetily-backend URL (default 5167)
    // If remote_url is provided, assume it points to the backend base URL
    let backend_url = remote_url
        .or_else(|| std::env::var("MEETILY_BACKEND_URL").ok())
        .unwrap_or_else(|| "http://localhost:5167".to_string());
    
    info!("🌐 Using backend server: {}", backend_url);
    
    // Read the audio file
    let audio_path = PathBuf::from(&audio_path);
    if !audio_path.exists() {
        return Ok(RetranscribeResult {
            success: false,
            transcript: None,
            error: Some(format!("Audio file not found: {}", audio_path.display())),
            audio_duration_secs: None,
            processing_time_secs: start_time.elapsed().as_secs_f64(),
        });
    }
    
    // Check file extension and convert if needed
    let extension = audio_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    let wav_path = if extension != "wav" {
        info!("📦 Converting {} to WAV format...", extension);
        match convert_to_wav(&audio_path).await {
            Ok(path) => path,
            Err(e) => {
                return Ok(RetranscribeResult {
                    success: false,
                    transcript: None,
                    error: Some(format!("Failed to convert audio to WAV: {}", e)),
                    audio_duration_secs: None,
                    processing_time_secs: start_time.elapsed().as_secs_f64(),
                });
            }
        }
    } else {
        audio_path.clone()
    };
    
    // Read WAV file
    let wav_data = match std::fs::read(&wav_path) {
        Ok(data) => data,
        Err(e) => {
            return Ok(RetranscribeResult {
                success: false,
                transcript: None,
                error: Some(format!("Failed to read WAV file: {}", e)),
                audio_duration_secs: None,
                processing_time_secs: start_time.elapsed().as_secs_f64(),
            });
        }
    };
    
    info!("📤 Sending {} bytes to backend...", wav_data.len());
    
    // Create HTTP client
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30)) // Short timeout for start request
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    
    // Build multipart form
    let file_part = Part::bytes(wav_data)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| format!("Failed to create file part: {}", e))?;
    
    let mut form = Form::new().part("file", file_part);
    
    if let Some(ref l) = language {
        if l != "auto" {
            form = form.text("language", l.clone());
        }
    }
    
    // Start Job
    let start_url = format!("{}/enhanced-transcribe", backend_url);
    let response = match client.post(&start_url).multipart(form).send().await {
        Ok(r) => r,
        Err(e) => return Ok(RetranscribeResult {
            success: false,
            transcript: None,
            error: Some(format!("Failed to connect to backend: {}", e)),
            audio_duration_secs: None,
            processing_time_secs: start_time.elapsed().as_secs_f64(),
        }),
    };

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Ok(RetranscribeResult {
            success: false,
            transcript: None,
            error: Some(format!("Backend error: {}", error_text)),
            audio_duration_secs: None,
            processing_time_secs: start_time.elapsed().as_secs_f64(),
        });
    }

    let job_resp: EnhancedJobResponse = response.json().await.map_err(|e| format!("Failed to parse job response: {}", e))?;
    let job_id = job_resp.job_id;
    info!("✅ Job started: {}", job_id);

    // Poll for completion
    let poll_client = reqwest::Client::new();
    let status_url = format!("{}/enhanced-transcribe/{}", backend_url, job_id);
    
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        
        let status_resp = match poll_client.get(&status_url).send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("Poll failed: {}", e);
                continue;
            }
        };
        
        if !status_resp.status().is_success() {
             warn!("Poll error status: {}", status_resp.status());
             continue;
        }
        
        let status: EnhancedStatusResponse = match status_resp.json().await {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to parse status: {}", e);
                continue;
            }
        };
        
        info!("Job {} status: {} (progress: {:?})", job_id, status.status, status.progress);
        
        if status.status == "completed" {
            if let Some(result) = status.result {
                let json_str = serde_json::to_string(&result.segments).unwrap_or_default();
                
                // Save improved transcript to file
                let audio_path_buf = PathBuf::from(&audio_path);
                if let Some(meeting_folder) = audio_path_buf.parent() {
                    let improved_path = meeting_folder.join("transcripts_improved.json");
                    let improved_json = serde_json::json!({
                        "version": "1.0",
                        "source": "enhanced_backend",
                        "server_url": backend_url,
                        "created_at": chrono::Utc::now().to_rfc3339(),
                        "processing_time_secs": start_time.elapsed().as_secs_f64(),
                        "transcript": json_str, // Store raw segments JSON
                        "segments": result.segments
                    });
                    
                    if let Err(e) = std::fs::write(&improved_path, serde_json::to_string_pretty(&improved_json).unwrap_or_default()) {
                        warn!("Failed to save improved transcript: {}", e);
                    } else {
                        info!("💾 Saved improved transcript to {}", improved_path.display());
                    }
                }

                return Ok(RetranscribeResult {
                    success: true,
                    transcript: Some(json_str),
                    error: None,
                    audio_duration_secs: None,
                    processing_time_secs: start_time.elapsed().as_secs_f64(),
                });
            } else {
                return Ok(RetranscribeResult {
                    success: false,
                    transcript: None,
                    error: Some("Job completed but no result found".to_string()),
                    audio_duration_secs: None,
                    processing_time_secs: start_time.elapsed().as_secs_f64(),
                });
            }
        } else if status.status == "failed" {
            return Ok(RetranscribeResult {
                success: false,
                transcript: None,
                error: status.error.or(Some("Job failed".to_string())),
                audio_duration_secs: None,
                processing_time_secs: start_time.elapsed().as_secs_f64(),
            });
        }
        
        // Timeout check (e.g. 2 hours)
        if start_time.elapsed().as_secs() > 7200 {
             return Ok(RetranscribeResult {
                success: false,
                transcript: None,
                error: Some("Job timed out".to_string()),
                audio_duration_secs: None,
                processing_time_secs: start_time.elapsed().as_secs_f64(),
            });
        }
    }
}

/// Convert an audio file to WAV format using FFmpeg
async fn convert_to_wav(input_path: &PathBuf) -> Result<PathBuf, String> {
    let output_path = input_path.with_extension("temp.wav");
    
    #[cfg(target_os = "macos")]
    let ffmpeg_path = find_ffmpeg_path()
        .ok_or_else(|| "FFmpeg not found. Please install FFmpeg.".to_string())?;
    
    #[cfg(not(target_os = "macos"))]
    let ffmpeg_path = "ffmpeg".to_string();
    
    // Note: Use stereo (-ac 2) for diarization support
    // whisper.cpp server requires stereo WAV for speaker identification
    let output = std::process::Command::new(&ffmpeg_path)
        .args([
            "-y",           // Overwrite output
            "-i", input_path.to_str().unwrap(),
            "-ar", "16000", // 16kHz sample rate (whisper requirement)
            "-ac", "2",     // Stereo (required for diarization)
            "-c:a", "pcm_s16le", // 16-bit PCM
            output_path.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| format!("Failed to run FFmpeg: {}", e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg conversion failed: {}", stderr));
    }
    
    Ok(output_path)
}

#[cfg(target_os = "macos")]
fn find_ffmpeg_path() -> Option<String> {
    // Check common paths on macOS
    let paths = [
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/usr/bin/ffmpeg",
    ];
    
    for path in paths {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    
    None
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecordingInfo {
    pub meeting_name: String,
    pub audio_path: String,
    pub has_transcript: bool,
    pub has_improved_transcript: bool,
    pub file_size_bytes: u64,
}

/// List all recordings that can be re-transcribed
#[command]
pub async fn list_recordings_for_retranscription() -> Result<Vec<RecordingInfo>, String> {
    let recordings_dir = get_recordings_dir()?;
    
    let mut recordings = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(&recordings_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Look for audio.mp4 in the meeting folder
                let audio_path = path.join("audio.mp4");
                let transcript_path = path.join("transcript.txt");
                let improved_path = path.join("transcripts_improved.json");
                
                if audio_path.exists() {
                    let meeting_name = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    
                    let has_transcript = transcript_path.exists();
                    let has_improved_transcript = improved_path.exists();
                    
                    // Get file size
                    let file_size = std::fs::metadata(&audio_path)
                        .map(|m| m.len())
                        .unwrap_or(0);
                    
                    recordings.push(RecordingInfo {
                        meeting_name,
                        audio_path: audio_path.to_string_lossy().to_string(),
                        has_transcript,
                        has_improved_transcript,
                        file_size_bytes: file_size,
                    });
                }
            }
        }
    }
    
    // Sort by meeting name (which includes date)
    recordings.sort_by(|a, b| b.meeting_name.cmp(&a.meeting_name));
    
    Ok(recordings)
}

fn get_recordings_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        if let Some(movies_dir) = dirs::video_dir() {
            return Ok(movies_dir.join("meetily-recordings"));
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        if let Some(music_dir) = dirs::audio_dir() {
            return Ok(music_dir.join("meetily-recordings"));
        }
    }
    
    dirs::document_dir()
        .map(|d| d.join("meetily-recordings"))
        .ok_or_else(|| "Could not find recordings directory".to_string())
}

/// Auto-retranscribe after recording stops (called from Rust, not a command)
/// This is triggered automatically when a recording ends
pub async fn auto_retranscribe_meeting(
    meeting_folder: PathBuf,
    remote_url: Option<String>,
) -> Result<RetranscribeResult, String> {
    info!("🔄 Auto-retranscription starting for folder: {}", meeting_folder.display());
    
    // Find audio file in meeting folder
    let audio_path = meeting_folder.join("audio.mp4");
    if !audio_path.exists() {
        warn!("⚠️ No audio.mp4 found in {}", meeting_folder.display());
        return Err("Audio file not found".to_string());
    }
    
    // Call the main retranscription function
    let result = retranscribe_audio_file(
        audio_path.to_string_lossy().to_string(),
        remote_url,
        None, // Will use MEETILY_LANGUAGE from env
    ).await?;
    
    if result.success {
        info!("✅ Auto-retranscription successful for {}", meeting_folder.display());
    } else {
        warn!("⚠️ Auto-retranscription failed: {:?}", result.error);
    }
    
    Ok(result)
}

/// Check if remote whisper is configured and available
pub async fn is_remote_whisper_available() -> bool {
    // Check if provider is set to remoteWhisper
    let server_url = std::env::var("MEETILY_WHISPER_URL")
        .unwrap_or_else(|_| "".to_string());
    
    if server_url.is_empty() {
        return false;
    }
    
    // Try to connect to the server
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build() {
        Ok(c) => c,
        Err(_) => return false,
    };
    
    match client.get(&server_url).send().await {
        Ok(response) => response.status().is_success() || response.status().as_u16() == 200,
        Err(_) => false,
    }
}
