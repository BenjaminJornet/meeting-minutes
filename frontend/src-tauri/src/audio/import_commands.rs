use crate::api::api::{api_get_transcript_config, api_save_transcript, TranscriptSegment};
use crate::audio::audio_processing::{create_meeting_folder, sanitize_filename};
use crate::audio::recording_preferences::get_default_recordings_folder;
use crate::audio::transcription::engine::{get_or_init_transcription_engine, validate_transcription_model_ready, TranscriptionEngine};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tauri::{command, AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;

#[derive(Debug, Clone, Serialize)]
struct ImportProgressPayload<'a> {
    phase: &'a str,
    message: &'a str,
    meeting_title: Option<&'a str>,
    folder_path: Option<String>,
    meeting_id: Option<&'a str>,
    task_kind: &'a str,
    phase_progress: f64,
    overall_progress: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportAudioMeetingResult {
    pub cancelled: bool,
    pub meeting_id: Option<String>,
    pub meeting_title: Option<String>,
    pub folder_path: Option<String>,
    pub source_file: Option<String>,
    pub was_converted: bool,
}

#[derive(Debug, Serialize)]
struct MeetingMetadata<'a> {
    version: &'a str,
    meeting_name: &'a str,
    status: &'a str,
    source: &'a str,
    audio_file: &'a str,
    transcript_file: &'a str,
    created_at: String,
    completed_at: Option<String>,
    source_file: String,
    meeting_id: Option<String>,
}

#[command]
pub async fn import_audio_file_as_meeting<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ImportAudioMeetingResult, String> {
    let selected = app.dialog().file().add_filter(
        "Audio / Video",
        &["mp3", "m4a", "wav", "aac", "ogg", "mp4", "flac"],
    ).blocking_pick_file();

    let Some(selected_path) = selected.and_then(|p| p.into_path().ok()) else {
        return Ok(ImportAudioMeetingResult {
            cancelled: true,
            meeting_id: None,
            meeting_title: None,
            folder_path: None,
            source_file: None,
            was_converted: false,
        });
    };

    if !selected_path.exists() {
        return Err(format!("Selected file does not exist: {}", selected_path.display()));
    }

    let title = derive_meeting_title(&selected_path);
    let recordings_dir = get_default_recordings_folder();
    let meeting_folder = create_meeting_folder(&recordings_dir, &title)
        .map_err(|e| format!("Failed to create meeting folder: {}", e))?;

    emit_progress(
        &app,
        "creating",
        "Meeting created. Preparing files...",
        Some(&title),
        Some(meeting_folder.to_string_lossy().to_string()),
        None,
    );

    let final_audio_path = meeting_folder.join("audio.mp4");

    write_metadata(&meeting_folder, &title, "processing", &selected_path, None, None)?;

    emit_progress(
        &app,
        "converting",
        "Converting or copying audio to audio.mp4...",
        Some(&title),
        Some(meeting_folder.to_string_lossy().to_string()),
        None,
    );
    let was_converted = match ensure_audio_mp4(&selected_path, &final_audio_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = write_metadata(&meeting_folder, &title, "error", &selected_path, None, None);
            emit_progress(
                &app,
                "error",
                &error,
                Some(&title),
                Some(meeting_folder.to_string_lossy().to_string()),
                None,
            );
            return Err(error);
        }
    };

    emit_progress(
        &app,
        "transcribing",
        "Transcribing imported audio...",
        Some(&title),
        Some(meeting_folder.to_string_lossy().to_string()),
        None,
    );
    let segments = match transcribe_imported_audio(&app, &final_audio_path).await {
        Ok(segments) => segments,
        Err(error) => {
            let _ = write_metadata(&meeting_folder, &title, "error", &selected_path, None, None);
            emit_progress(
                &app,
                "error",
                &error,
                Some(&title),
                Some(meeting_folder.to_string_lossy().to_string()),
                None,
            );
            return Err(error);
        }
    };

    if segments.is_empty() {
        warn!("Imported audio generated no segments, saving fallback empty segment list");
    }

    write_transcripts_json(&meeting_folder, &title, &segments)?;

    emit_progress(
        &app,
        "saving",
        "Saving transcript and registering the meeting...",
        Some(&title),
        Some(meeting_folder.to_string_lossy().to_string()),
        None,
    );
    let state = app.state();
    let save_result = match api_save_transcript(
        app.clone(),
        state,
        title.clone(),
        segments
            .iter()
            .map(|s| serde_json::to_value(s).unwrap_or_default())
            .collect(),
        Some(meeting_folder.to_string_lossy().to_string()),
        None,
    )
    .await {
        Ok(result) => result,
        Err(error) => {
            let _ = write_metadata(&meeting_folder, &title, "error", &selected_path, None, None);
            emit_progress(
                &app,
                "error",
                &error,
                Some(&title),
                Some(meeting_folder.to_string_lossy().to_string()),
                None,
            );
            return Err(error);
        }
    };

    let meeting_id = save_result
        .get("meeting_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Meeting ID missing from save result".to_string())?
        .to_string();

    write_metadata(
        &meeting_folder,
        &title,
        "completed",
        &selected_path,
        Some(&meeting_id),
        Some(chrono::Utc::now().to_rfc3339()),
    )?;

    emit_progress(
        &app,
        "completed",
        "Audio imported successfully.",
        Some(&title),
        Some(meeting_folder.to_string_lossy().to_string()),
        Some(&meeting_id),
    );

    Ok(ImportAudioMeetingResult {
        cancelled: false,
        meeting_id: Some(meeting_id),
        meeting_title: Some(title),
        folder_path: Some(meeting_folder.to_string_lossy().to_string()),
        source_file: Some(selected_path.to_string_lossy().to_string()),
        was_converted,
    })
}

fn derive_meeting_title(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Meeting");
    let sanitized = sanitize_filename(stem).trim().to_string();
    if sanitized.is_empty() {
        format!("Meeting {}", chrono::Utc::now().format("%Y-%m-%d %H:%M"))
    } else {
        sanitized
    }
}

fn write_metadata(
    meeting_folder: &Path,
    meeting_name: &str,
    status: &str,
    source_file: &Path,
    meeting_id: Option<&str>,
    completed_at: Option<String>,
) -> Result<(), String> {
    let metadata = MeetingMetadata {
        version: "1.0",
        meeting_name,
        status,
        source: "imported_audio",
        audio_file: "audio.mp4",
        transcript_file: "transcripts.json",
        created_at: chrono::Utc::now().to_rfc3339(),
        completed_at,
        source_file: source_file.to_string_lossy().to_string(),
        meeting_id: meeting_id.map(|s| s.to_string()),
    };

    fs::write(
        meeting_folder.join("metadata.json"),
        serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Failed to write metadata.json: {}", e))
}

fn write_transcripts_json(
    meeting_folder: &Path,
    meeting_name: &str,
    segments: &[TranscriptSegment],
) -> Result<(), String> {
    let payload = serde_json::json!({
        "version": "1.0",
        "meeting_name": meeting_name,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "audio_file": "audio.mp4",
        "segments": segments,
    });
    fs::write(
        meeting_folder.join("transcripts.json"),
        serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Failed to write transcripts.json: {}", e))
}

fn emit_progress<R: Runtime>(
    app: &AppHandle<R>,
    phase: &str,
    message: &str,
    meeting_title: Option<&str>,
    folder_path: Option<String>,
    meeting_id: Option<&str>,
) {
    let phase_progress = match phase {
        "creating" => 100.0,
        "converting" => 100.0,
        "uploading" => 0.0,
        "queued" => 100.0,
        "transcribing" => 0.0,
        "saving" => 100.0,
        "completed" => 100.0,
        _ => 0.0,
    };
    let overall_progress = compute_overall_progress(phase, phase_progress);

    let _ = app.emit(
        "audio-import-progress",
        ImportProgressPayload {
            phase,
            message,
            meeting_title,
            folder_path,
            meeting_id,
            task_kind: "import",
            phase_progress,
            overall_progress,
        },
    );
}

fn emit_progress_with_percent<R: Runtime>(
    app: &AppHandle<R>,
    phase: &str,
    message: String,
    meeting_title: Option<&str>,
    folder_path: Option<String>,
    meeting_id: Option<&str>,
    phase_progress: f64,
) {
    let _ = app.emit(
        "audio-import-progress",
        ImportProgressPayload {
            phase,
            message: &message,
            meeting_title,
            folder_path,
            meeting_id,
            task_kind: "import",
            phase_progress,
            overall_progress: compute_overall_progress(phase, phase_progress),
        },
    );
}

fn compute_overall_progress(phase: &str, phase_progress: f64) -> f64 {
    let weights = [
        ("creating", 5.0),
        ("converting", 10.0),
        ("uploading", 30.0),
        ("queued", 5.0),
        ("transcribing", 40.0),
        ("saving", 10.0),
    ];

    if phase == "completed" {
        return 100.0;
    }

    let mut completed = 0.0;
    for (name, weight) in weights {
        if name == phase {
            return (completed + weight * (phase_progress.clamp(0.0, 100.0) / 100.0)).clamp(0.0, 100.0);
        }
        completed += weight;
    }
    phase_progress.clamp(0.0, 100.0)
}

fn ensure_audio_mp4(input: &Path, output: &Path) -> Result<bool, String> {
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "mp4" {
        fs::copy(input, output).map_err(|e| format!("Failed to copy mp4 file: {}", e))?;
        return Ok(false);
    }

    let ffmpeg = find_ffmpeg_path().ok_or_else(|| "FFmpeg not found. Please install FFmpeg.".to_string())?;

    let copy_attempt = std::process::Command::new(&ffmpeg)
        .args(["-y", "-i", input.to_str().unwrap_or_default(), "-c", "copy", output.to_str().unwrap_or_default()])
        .output()
        .map_err(|e| format!("Failed to run FFmpeg: {}", e))?;

    if copy_attempt.status.success() {
        return Ok(true);
    }

    let transcode_attempt = std::process::Command::new(&ffmpeg)
        .args([
            "-y",
            "-i",
            input.to_str().unwrap_or_default(),
            "-vn",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            output.to_str().unwrap_or_default(),
        ])
        .output()
        .map_err(|e| format!("Failed to run FFmpeg: {}", e))?;

    if !transcode_attempt.status.success() {
        return Err(format!(
            "FFmpeg conversion failed: {}",
            String::from_utf8_lossy(&transcode_attempt.stderr)
        ));
    }

    Ok(true)
}

#[cfg(target_os = "macos")]
fn find_ffmpeg_path() -> Option<String> {
    ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/usr/bin/ffmpeg"]
        .iter()
        .find(|p| Path::new(p).exists())
        .map(|p| p.to_string())
}

#[cfg(not(target_os = "macos"))]
fn find_ffmpeg_path() -> Option<String> {
    Some("ffmpeg".to_string())
}

async fn transcribe_imported_audio<R: Runtime>(
    app: &AppHandle<R>,
    audio_path: &Path,
) -> Result<Vec<TranscriptSegment>, String> {
    let config = api_get_transcript_config(app.clone(), app.clone().state(), None)
        .await?
        .unwrap_or(crate::api::api::TranscriptConfig {
            provider: "localWhisper".to_string(),
            model: "large-v3".to_string(),
            api_key: None,
            remote_url: None,
        });

    let env_whisper_url = std::env::var("MEETILY_WHISPER_URL")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let env_backend_url = std::env::var("MEETILY_BACKEND_URL")
        .ok()
        .filter(|value| !value.trim().is_empty());

    info!(
        "🎙️ Import audio transcription strategy - saved provider: {}, saved remote_url: {:?}, env whisper: {:?}, env backend: {:?}",
        config.provider,
        config.remote_url,
        env_whisper_url,
        env_backend_url
    );

    let should_prefer_remote = config.provider == "remoteWhisper"
        || env_whisper_url
            .as_ref()
            .map(|url| is_remote_url(url))
            .unwrap_or(false)
        || env_backend_url
            .as_ref()
            .map(|url| is_remote_url(url))
            .unwrap_or(false);

    if should_prefer_remote {
        let remote_target = config
            .remote_url
            .clone()
            .or_else(|| env_whisper_url.clone())
            .or_else(|| env_backend_url.clone())
            .filter(|value| !value.trim().is_empty());

        info!(
            "🌐 Import audio will use REMOTE transcription. effective_remote_target={:?}",
            remote_target
        );

        if remote_target.is_none() {
            return Err(
                "Remote Whisper is selected, but no remote server URL is configured. Please set it in Settings > Transcription."
                    .to_string(),
            );
        }

        return transcribe_remote(app, audio_path, remote_target).await;
    }

    info!(
        "💻 Import audio will use LOCAL transcription with provider '{}'",
        config.provider
    );

    validate_transcription_model_ready(app).await?;
    let samples = decode_audio_to_16k_mono(audio_path)?;
    let engine = get_or_init_transcription_engine(app).await?;

    match engine {
        TranscriptionEngine::Whisper(engine) => {
            let text = engine.transcribe_audio(samples, None).await.map_err(|e| e.to_string())?;
            Ok(vec![single_segment_from_text(text)])
        }
        TranscriptionEngine::Parakeet(engine) => {
            let text = engine.transcribe_audio(samples).await.map_err(|e| e.to_string())?;
            Ok(vec![single_segment_from_text(text)])
        }
        _ => Err("Unsupported transcription engine for imported audio".to_string()),
    }
}

fn is_remote_url(url: &str) -> bool {
    let normalized = url.trim().to_lowercase();
    !(normalized.contains("localhost") || normalized.contains("127.0.0.1"))
}

async fn transcribe_remote<R: Runtime>(
    app: &AppHandle<R>,
    audio_path: &Path,
    remote_url: Option<String>,
) -> Result<Vec<TranscriptSegment>, String> {
    emit_progress_with_percent(
        app,
        "uploading",
        "Uploading imported audio to remote backend...".to_string(),
        None,
        None,
        None,
        5.0,
    );
    let result = crate::audio::transcription::retranscribe_commands::retranscribe_audio_file(
        app.clone(),
        audio_path.to_string_lossy().to_string(),
        remote_url,
        None,
    )
    .await?;

    if !result.success {
        return Err(result.error.unwrap_or_else(|| "Remote transcription failed".to_string()));
    }

    let transcript = result.transcript.unwrap_or_default();
    let remote_segments: Vec<serde_json::Value> = serde_json::from_str(&transcript)
        .map_err(|e| format!("Failed to parse remote segments: {}", e))?;

    let mut segments = Vec::new();
    for (index, seg) in remote_segments.iter().enumerate() {
        let text = seg.get("text").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if text.is_empty() {
            continue;
        }
        let start = seg.get("start").and_then(|v| v.as_f64());
        let end = seg.get("end").and_then(|v| v.as_f64());
        segments.push(TranscriptSegment {
            id: format!("segment-{}", index + 1),
            text,
            timestamp: format_timestamp(start.unwrap_or(0.0)),
            audio_start_time: start,
            audio_end_time: end,
            duration: match (start, end) {
                (Some(s), Some(e)) => Some((e - s).max(0.0)),
                _ => None,
            },
        });
    }

    if segments.is_empty() {
        segments.push(single_segment_from_text(transcript));
    }

    Ok(segments)
}

fn single_segment_from_text(text: String) -> TranscriptSegment {
    TranscriptSegment {
        id: "segment-1".to_string(),
        text,
        timestamp: "00:00".to_string(),
        audio_start_time: Some(0.0),
        audio_end_time: None,
        duration: None,
    }
}

fn format_timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as i64;
    let minutes = total / 60;
    let secs = total % 60;
    format!("{:02}:{:02}", minutes, secs)
}

fn decode_audio_to_16k_mono(path: &Path) -> Result<Vec<f32>, String> {
    let src = std::fs::File::open(path).map_err(|e| format!("Failed to open audio file: {}", e))?;
    let mss = MediaSourceStream::new(Box::new(src), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("Failed to probe audio format: {}", e))?;

    let mut format = probed.format;
    let track = format.default_track().ok_or_else(|| "No default audio track found".to_string())?;
    let sample_rate = track.codec_params.sample_rate.ok_or_else(|| "Unknown sample rate".to_string())?;
    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(1) as u16;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("Failed to create audio decoder: {}", e))?;

    let track_id = track.id;
    let mut samples = Vec::<f32>::new();

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(symphonia::core::errors::Error::IoError(_)) => break,
            Err(e) => return Err(format!("Failed reading audio packet: {}", e)),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(format!("Failed to decode audio: {}", e)),
        };

        match decoded {
            AudioBufferRef::F32(buf) => {
                for frame in 0..buf.frames() {
                    let mut sum = 0.0f32;
                    for ch in 0..buf.spec().channels.count() {
                        sum += buf.chan(ch)[frame];
                    }
                    samples.push(sum / buf.spec().channels.count() as f32);
                }
            }
            AudioBufferRef::S16(buf) => {
                for frame in 0..buf.frames() {
                    let mut sum = 0.0f32;
                    for ch in 0..buf.spec().channels.count() {
                        sum += buf.chan(ch)[frame] as f32 / i16::MAX as f32;
                    }
                    samples.push(sum / buf.spec().channels.count() as f32);
                }
            }
            AudioBufferRef::U8(buf) => {
                for frame in 0..buf.frames() {
                    let mut sum = 0.0f32;
                    for ch in 0..buf.spec().channels.count() {
                        sum += (buf.chan(ch)[frame] as f32 - 128.0) / 128.0;
                    }
                    samples.push(sum / buf.spec().channels.count() as f32);
                }
            }
            _ => {
                return Err("Unsupported audio sample format for imported audio".to_string());
            }
        }
    }

    let mono = if channels > 1 { samples } else { samples };
    crate::audio::audio_processing::resample(&mono, sample_rate, 16_000)
        .map_err(|e| format!("Failed to resample audio: {}", e))
}