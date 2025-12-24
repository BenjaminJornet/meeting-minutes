// audio/transcription/remote_whisper_provider.rs
//
// Remote Whisper transcription provider implementation.
// Sends audio to a remote whisper.cpp server via HTTP for transcription.

use super::provider::{TranscriptionError, TranscriptionProvider, TranscriptResult};
use async_trait::async_trait;
use log::{debug, error, info, warn};
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Response from the whisper.cpp server /inference endpoint
// Response format 1: {"text": "..."}
#[derive(Debug, Deserialize)]
struct WhisperServerResponse {
    text: String,
}

// Response format 2: {"transcription": [{"timestamps": {...}, "offsets": {...}, "text": "..."}]}
#[derive(Debug, Deserialize)]
struct WhisperServerResponseWithTranscription {
    transcription: Vec<TranscriptionSegment>,
}

#[derive(Debug, Deserialize)]
struct TranscriptionSegment {
    text: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    tokens: Vec<serde_json::Value>,
}

/// Configuration for the remote whisper server
#[derive(Debug, Clone)]
pub struct RemoteWhisperConfig {
    /// Base URL of the whisper server (e.g., "http://100.64.0.4:8178")
    pub server_url: String,
    /// Request timeout in seconds
    pub timeout_secs: u64,
}

impl Default for RemoteWhisperConfig {
    fn default() -> Self {
        Self {
            server_url: std::env::var("MEETILY_WHISPER_URL")
                .unwrap_or_else(|_| "http://localhost:8178".to_string()),
            timeout_secs: std::env::var("MEETILY_WHISPER_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3600),
        }
    }
}

/// Remote Whisper transcription provider
/// Sends audio to a remote whisper.cpp server for transcription
pub struct RemoteWhisperProvider {
    config: Arc<RwLock<RemoteWhisperConfig>>,
    client: reqwest::Client,
}

impl RemoteWhisperProvider {
    /// Create a new RemoteWhisperProvider with the given server URL
    pub fn new(server_url: String) -> Self {
        let config = RemoteWhisperConfig {
            server_url,
            ..Default::default()
        };
        
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");
        
        Self {
            config: Arc::new(RwLock::new(config)),
            client,
        }
    }

    /// Update the server URL
    pub async fn set_server_url(&self, url: String) {
        let mut config = self.config.write().await;
        config.server_url = url;
    }

    /// Get the current server URL
    pub async fn get_server_url(&self) -> String {
        self.config.read().await.server_url.clone()
    }

    /// Convert f32 audio samples to WAV bytes (16kHz mono)
    fn samples_to_wav(samples: &[f32]) -> Vec<u8> {
        const SAMPLE_RATE: u32 = 16000;
        const BITS_PER_SAMPLE: u16 = 16;
        const NUM_CHANNELS: u16 = 1;

        // Convert f32 samples to i16
        let i16_samples: Vec<i16> = samples
            .iter()
            .map(|&s| {
                let clamped = s.clamp(-1.0, 1.0);
                (clamped * 32767.0) as i16
            })
            .collect();

        let data_size = (i16_samples.len() * 2) as u32;
        let file_size = 36 + data_size;

        let mut wav = Vec::with_capacity(44 + i16_samples.len() * 2);

        // RIFF header
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&file_size.to_le_bytes());
        wav.extend_from_slice(b"WAVE");

        // fmt subchunk
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&NUM_CHANNELS.to_le_bytes());
        wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        let byte_rate = SAMPLE_RATE * u32::from(NUM_CHANNELS) * u32::from(BITS_PER_SAMPLE) / 8;
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        let block_align = NUM_CHANNELS * BITS_PER_SAMPLE / 8;
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());

        // data subchunk
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());

        for sample in &i16_samples {
            wav.extend_from_slice(&sample.to_le_bytes());
        }

        wav
    }

    /// Check if the server is reachable
    pub async fn check_server_health(&self) -> Result<bool, String> {
        let config = self.config.read().await;
        let url = format!("{}/", config.server_url);
        
        match self.client.get(&url).timeout(std::time::Duration::from_secs(5)).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    info!("✅ Remote whisper server reachable at {}", config.server_url);
                    Ok(true)
                } else {
                    warn!("⚠️ Remote whisper server returned status {}", response.status());
                    Ok(false)
                }
            }
            Err(e) => {
                error!("❌ Cannot reach remote whisper server at {}: {}", config.server_url, e);
                Err(format!("Server unreachable: {}", e))
            }
        }
    }
}

#[async_trait]
impl TranscriptionProvider for RemoteWhisperProvider {
    async fn transcribe(
        &self,
        audio: Vec<f32>,
        language: Option<String>,
    ) -> std::result::Result<TranscriptResult, TranscriptionError> {
        let config = self.config.read().await;
        let inference_url = format!("{}/inference", config.server_url);

        debug!("🌐 Sending {} samples to remote whisper at {}", audio.len(), inference_url);

        // Minimum audio length check
        if audio.len() < 1600 {
            return Err(TranscriptionError::AudioTooShort {
                samples: audio.len(),
                minimum: 1600,
            });
        }

        // Convert to WAV
        let wav_data = Self::samples_to_wav(&audio);

        // Build multipart form
        let file_part = Part::bytes(wav_data)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| TranscriptionError::EngineFailed(format!("Failed to create file part: {}", e)))?;

        let mut form = Form::new()
            .part("file", file_part)
            .text("temperature", "0.0")
            .text("temperature_inc", "0.2")
            .text("response_format", "json");

        // Add language if specified
        if let Some(lang) = language {
            if lang != "auto" && lang != "auto-translate" {
                form = form.text("language", lang);
            }
        }

        // Send request
        let response = self
            .client
            .post(&inference_url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| {
                error!("❌ Remote whisper request failed: {}", e);
                TranscriptionError::EngineFailed(format!("HTTP request failed: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("❌ Remote whisper error {}: {}", status, error_text);
            return Err(TranscriptionError::EngineFailed(format!(
                "Server error {}: {}",
                status, error_text
            )));
        }

        // Parse response - handle multiple JSON formats from whisper.cpp server
        let response_text = response.text().await.map_err(|e| {
            error!("❌ Failed to read whisper response: {}", e);
            TranscriptionError::EngineFailed(format!("Failed to read response: {}", e))
        })?;

        debug!("🌐 Raw response from remote whisper: {}", response_text);

        // Try format 1: {"text": "..."}
        let text = if let Ok(result) = serde_json::from_str::<WhisperServerResponse>(&response_text) {
            result.text.trim().to_string()
        }
        // Try format 2: {"transcription": [{"text": "..."}]}
        else if let Ok(result) = serde_json::from_str::<WhisperServerResponseWithTranscription>(&response_text) {
            result.transcription
                .iter()
                .filter_map(|seg| seg.text.as_ref())
                .map(|t| t.trim())
                .collect::<Vec<_>>()
                .join(" ")
        }
        // Fallback: plain text response
        else {
            warn!("⚠️ Could not parse JSON, using raw response as text");
            response_text.trim().to_string()
        };

        debug!("🌐 Remote transcription: '{}'", text);

        Ok(TranscriptResult {
            text,
            confidence: None,
            is_partial: false,
        })
    }

    async fn is_model_loaded(&self) -> bool {
        self.check_server_health().await.unwrap_or(false)
    }

    async fn get_current_model(&self) -> Option<String> {
        let url = self.config.read().await.server_url.clone();
        Some(format!("Remote ({})", url))
    }

    fn provider_name(&self) -> &'static str {
        "Remote Whisper"
    }
}
