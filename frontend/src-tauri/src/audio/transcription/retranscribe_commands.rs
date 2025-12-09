// audio/transcription/retranscribe_commands.rs
//
// Commands for re-transcribing audio files with a remote whisper server
// for higher quality transcripts after recording.

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

/// Response from the whisper.cpp server /inference endpoint
/// The server can return different formats depending on configuration
#[derive(Debug, Deserialize)]
struct WhisperServerResponse {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    transcription: Option<Vec<TranscriptionSegment>>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TranscriptionSegment {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    speaker: Option<String>,
    #[serde(default)]
    t0: Option<i64>,
    #[serde(default)]
    t1: Option<i64>,
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
    
    info!("🔄 Starting re-transcription of: {}", audio_path);
    
    // Get server URL from parameter or environment
    let server_url = remote_url
        .or_else(|| std::env::var("MEETILY_WHISPER_URL").ok())
        .unwrap_or_else(|| "http://localhost:8178".to_string());
    
    info!("🌐 Using remote whisper server: {}", server_url);
    
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
    
    // Check file extension
    let extension = audio_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    // If not a WAV file, convert it first
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
    
    info!("📤 Sending {} bytes to remote whisper server...", wav_data.len());
    
    // Create HTTP client with longer timeout for large files
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // 5 minutes for long recordings
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    
    // Build multipart form
    let file_part = Part::bytes(wav_data)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| format!("Failed to create file part: {}", e))?;
    
    let mut form = Form::new()
        .part("file", file_part)
        .text("temperature", "0.0")
        .text("temperature_inc", "0.2")
        .text("response_format", "json")
        .text("diarize", "true");  // Enable speaker diarization
    
    // Add language if specified, or use from environment
    let lang = language.or_else(|| std::env::var("MEETILY_LANGUAGE").ok());
    if let Some(ref l) = lang {
        if l != "auto" && l != "auto-translate" {
            form = form.text("language", l.clone());
        }
    }
    
    info!("📋 Request params: diarize=true, language={:?}", lang);
    
    // Send request
    let inference_url = format!("{}/inference", server_url);
    let response = match client
        .post(&inference_url)
        .multipart(form)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            error!("❌ Remote whisper request failed: {}", e);
            return Ok(RetranscribeResult {
                success: false,
                transcript: None,
                error: Some(format!("Failed to connect to remote server: {}", e)),
                audio_duration_secs: None,
                processing_time_secs: start_time.elapsed().as_secs_f64(),
            });
        }
    };
    
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        error!("❌ Remote whisper error {}: {}", status, error_text);
        return Ok(RetranscribeResult {
            success: false,
            transcript: None,
            error: Some(format!("Server error {}: {}", status, error_text)),
            audio_duration_secs: None,
            processing_time_secs: start_time.elapsed().as_secs_f64(),
        });
    }
    
    // Parse response - first get the raw text to log it
    let response_text = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            error!("❌ Failed to read response body: {}", e);
            return Ok(RetranscribeResult {
                success: false,
                transcript: None,
                error: Some(format!("Failed to read server response: {}", e)),
                audio_duration_secs: None,
                processing_time_secs: start_time.elapsed().as_secs_f64(),
            });
        }
    };
    
    info!("📥 Raw response (first 500 chars): {}", &response_text.chars().take(500).collect::<String>());
    
    // Try to parse as JSON
    let result: WhisperServerResponse = match serde_json::from_str(&response_text) {
        Ok(r) => r,
        Err(e) => {
            error!("❌ Failed to parse whisper response as JSON: {}", e);
            // If it's not JSON, maybe it's plain text
            if !response_text.trim().is_empty() && !response_text.contains("error") {
                info!("📝 Treating response as plain text transcript");
                let processing_time = start_time.elapsed().as_secs_f64();
                return Ok(RetranscribeResult {
                    success: true,
                    transcript: Some(response_text.trim().to_string()),
                    error: None,
                    audio_duration_secs: None,
                    processing_time_secs: processing_time,
                });
            }
            return Ok(RetranscribeResult {
                success: false,
                transcript: None,
                error: Some(format!("Failed to parse server response: {}. Raw: {}", e, &response_text.chars().take(200).collect::<String>())),
                audio_duration_secs: None,
                processing_time_secs: start_time.elapsed().as_secs_f64(),
            });
        }
    };
    
    // Check for error in response
    if let Some(err) = result.error {
        error!("❌ Server returned error: {}", err);
        return Ok(RetranscribeResult {
            success: false,
            transcript: None,
            error: Some(err),
            audio_duration_secs: None,
            processing_time_secs: start_time.elapsed().as_secs_f64(),
        });
    }
    
    // Build transcript from response
    let transcript = if let Some(text) = result.text {
        // Simple text response
        text.trim().to_string()
    } else if let Some(segments) = result.transcription {
        // Diarized response with segments
        segments
            .iter()
            .map(|seg| {
                let speaker = seg.speaker.as_deref().unwrap_or("Speaker");
                let text = seg.text.as_deref().unwrap_or("");
                format!("[{}]: {}", speaker, text.trim())
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        error!("❌ No transcript found in response");
        return Ok(RetranscribeResult {
            success: false,
            transcript: None,
            error: Some("No transcript in server response".to_string()),
            audio_duration_secs: None,
            processing_time_secs: start_time.elapsed().as_secs_f64(),
        });
    };
    
    let processing_time = start_time.elapsed().as_secs_f64();
    
    info!("✅ Re-transcription completed in {:.2}s", processing_time);
    info!("📝 Transcript length: {} characters", transcript.len());
    
    // Clean up temporary WAV file if we created one
    if extension != "wav" && wav_path != audio_path {
        if let Err(e) = std::fs::remove_file(&wav_path) {
            warn!("Failed to clean up temp WAV file: {}", e);
        }
    }
    
    Ok(RetranscribeResult {
        success: true,
        transcript: Some(transcript),
        error: None,
        audio_duration_secs: None, // Could calculate from WAV header
        processing_time_secs: processing_time,
    })
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
                
                if audio_path.exists() {
                    let meeting_name = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    
                    let has_transcript = transcript_path.exists();
                    
                    // Get file size
                    let file_size = std::fs::metadata(&audio_path)
                        .map(|m| m.len())
                        .unwrap_or(0);
                    
                    recordings.push(RecordingInfo {
                        meeting_name,
                        audio_path: audio_path.to_string_lossy().to_string(),
                        has_transcript,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct RecordingInfo {
    pub meeting_name: String,
    pub audio_path: String,
    pub has_transcript: bool,
    pub file_size_bytes: u64,
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
