use crate::database::repositories::{
    meeting::MeetingsRepository, setting::SettingsRepository, summary::SummaryProcessesRepository,
};
use crate::summary::llm_client::LLMProvider;
use crate::summary::processor::{extract_meeting_name_from_markdown, generate_meeting_summary};
use crate::ollama::metadata::ModelMetadataCache;
use sqlx::SqlitePool;
use std::time::{Duration, Instant};
use tracing::{error, info};
use once_cell::sync::Lazy;
use super::checkpoints::{self, Checkpoints, SummaryOptions};

// Global cache for model metadata (5 minute TTL)
static METADATA_CACHE: Lazy<ModelMetadataCache> = Lazy::new(|| {
    ModelMetadataCache::new(Duration::from_secs(300))
});

/// Summary service - handles all summary generation logic
pub struct SummaryService;

impl SummaryService {
    /// Processes transcript in the background and generates summary
    ///
    /// This function is designed to be spawned as an async task and does not block
    /// the main thread. It updates the database with progress and results.
    ///
    /// # Arguments
    /// * `pool` - SQLx connection pool
    /// * `meeting_id` - Unique identifier for the meeting
    /// * `text` - Full transcript text
    /// * `model_provider` - LLM provider name (e.g., "ollama", "openai")
    /// * `model_name` - Specific model (e.g., "gpt-4", "llama3.2:latest")
    /// * `custom_prompt` - Optional user-provided context
    /// * `template_id` - Template identifier (e.g., "daily_standup", "standard_meeting")
    pub async fn process_transcript_background(
        pool: SqlitePool,
        meeting_id: String,
        text: String,
        model_provider: String,
        model_name: String,
        custom_prompt: String,
        template_id: String,
        requested_options: Option<SummaryOptions>,
    ) {
        let start_time = Instant::now();
        info!(
            "🚀 Starting background processing for meeting_id: {}",
            meeting_id
        );

        // Parse provider
        let provider = match LLMProvider::from_str(&model_provider) {
            Ok(p) => p,
            Err(e) => {
                Self::update_process_failed(&pool, &meeting_id, &e).await;
                return;
            }
        };

        // Validate and setup api_key, Flexible for Ollama
        let api_key = match SettingsRepository::get_api_key(&pool, &model_provider).await {
            Ok(Some(key)) if !key.is_empty() => key,
            Ok(None) | Ok(Some(_)) => {
                if provider != LLMProvider::Ollama {
                    let err_msg = format!("Api key not found for {}", &model_provider);
                    Self::update_process_failed(&pool, &meeting_id, &err_msg).await;
                    return;
                }
                String::new()
            }
            Err(e) => {
                let err_msg = format!("Failed to retrieve api key for {} : {}", &model_provider, e);
                Self::update_process_failed(&pool, &meeting_id, &err_msg).await;
                return;
            }
        };

        // Get Ollama endpoint if provider is Ollama
        let ollama_endpoint = if provider == LLMProvider::Ollama {
            match SettingsRepository::get_model_config(&pool).await {
                Ok(Some(config)) => config.ollama_endpoint,
                Ok(None) => None,
                Err(e) => {
                    info!("Failed to retrieve Ollama endpoint: {}, using default", e);
                    None
                }
            }
        } else {
            None
        };

        let endpoint = checkpoints::endpoint_key(ollama_endpoint.as_deref());
        let mut options = match requested_options {
            Some(options) => options,
            None => match checkpoints::load_options(&pool, &endpoint, &model_provider, &model_name).await {
                Ok(options) => options,
                Err(error) => { Self::update_process_failed(&pool, &meeting_id, &error).await; return; }
            }
        };
        if provider == LLMProvider::Ollama {
            if let Ok(metadata) = METADATA_CACHE.get_or_fetch(&model_name, ollama_endpoint.as_deref()).await {
                options.context_tokens = options.context_tokens.min(metadata.context_size);
                options.output_tokens = options.output_tokens.min(options.context_tokens / 4);
            }
        }
        let template_fingerprint = match crate::summary::templates::get_template(&template_id) {
            Ok(template) => format!("{}\n{}",template.to_markdown_structure(),template.to_section_instructions()),
            Err(error) => { Self::update_process_failed(&pool,&meeting_id,&format!("template_invalid: {error}")).await; return; }
        };
        let run_key = checkpoints::hash(&serde_json::json!({
            "pipeline_version": 2, "meeting":meeting_id, "text":text, "context":custom_prompt,
            "template":template_fingerprint, "model":model_name, "provider":model_provider,
            "endpoint":endpoint, "options":options, "language":std::env::var("MEETILY_LANGUAGE").unwrap_or_else(|_| "en".into()),
        }).to_string());
        let checkpoints = Checkpoints { pool: pool.clone(), meeting_id: meeting_id.clone(), run_key };
        if let Err(error) = sqlx::query("UPDATE summary_processes SET status='processing' WHERE meeting_id=?")
            .bind(&meeting_id).execute(&pool).await {
            Self::update_process_failed(&pool,&meeting_id,&error.to_string()).await; return;
        }
        let client = match reqwest::Client::builder().connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(1200)).build() {
                Ok(client) => client,
                Err(error) => { Self::update_process_failed(&pool,&meeting_id,&error.to_string()).await; return; }
            };
        let result = generate_meeting_summary(&client, &provider, &model_name, &api_key,
            &text, &custom_prompt, &template_id, &options, ollama_endpoint.as_deref(), &checkpoints).await;

        let duration = start_time.elapsed().as_secs_f64();

        match result {
            Ok((mut final_markdown, num_chunks)) => {
                if num_chunks == 0 && final_markdown.is_empty() {
                    Self::update_process_failed(
                        &pool,
                        &meeting_id,
                        "Summary generation failed: No content was processed.",
                    )
                    .await;
                    return;
                }

                info!(
                    "✓ Successfully processed {} chunks for meeting_id: {}. Duration: {:.2}s",
                    num_chunks, meeting_id, duration
                );


                // Extract and update meeting name if present
                if let Some(name) = extract_meeting_name_from_markdown(&final_markdown) {
                    if !name.is_empty() {
                        info!(
                            "📝 Updating meeting name to '{}' for meeting_id: {}",
                            name, meeting_id
                        );
                        if let Err(e) =
                            MeetingsRepository::update_meeting_title(&pool, &meeting_id, &name).await
                        {
                            error!("⚠️ Failed to update meeting name for {}: {}", meeting_id, e);
                        }

                        // Strip the title line from markdown
                        info!("✂️ Stripping title from final_markdown");
                        if let Some(hash_pos) = final_markdown.find('#') {
                            // Find end of first line after '#'
                            let body_start =
                                if let Some(line_end) = final_markdown[hash_pos..].find('\n') {
                                    hash_pos + line_end
                                } else {
                                    final_markdown.len() // No newline, whole string is title
                                };

                            final_markdown = final_markdown[body_start..].trim_start().to_string();
                        } else {
                            // No '#' found, clear the string
                            final_markdown.clear();
                        }
                    }
                }

                if final_markdown.trim().is_empty() {
                    Self::update_process_failed(&pool, &meeting_id, "llm_response_empty: compte rendu sans contenu après nettoyage").await;
                    return;
                }

                // Create result JSON with markdown only (summary_json will be added on first edit)
                let result_json = serde_json::json!({
                    "markdown": final_markdown,
                });

                // Update database with completed status
                if let Err(e) = SummaryProcessesRepository::update_process_completed(
                    &pool,
                    &meeting_id,
                    result_json,
                    num_chunks,
                    duration,
                )
                .await
                {
                    Self::update_process_failed(&pool, &meeting_id, &format!("summary_save_failed: {e}")).await;
                } else {
                    info!(
                        "💾 Summary saved successfully for meeting_id: {}",
                        meeting_id
                    );
                }
            }
            Err(e) => {
                Self::update_process_failed(&pool, &meeting_id, &e).await;
            }
        }
    }

    /// Updates the summary process status to failed with error message
    ///
    /// # Arguments
    /// * `pool` - SQLx connection pool
    /// * `meeting_id` - Meeting identifier
    /// * `error_msg` - Error message to store
    async fn update_process_failed(pool: &SqlitePool, meeting_id: &str, error_msg: &str) {
        let code = error_msg.split(':').next().unwrap_or("summary_failed");
        let _ = sqlx::query("UPDATE summary_progress SET error_code=? WHERE meeting_id=?")
            .bind(code).bind(meeting_id).execute(pool).await;
        error!(
            "❌ Processing failed for meeting_id {}: {}",
            meeting_id, error_msg
        );
        if let Err(e) =
            SummaryProcessesRepository::update_process_failed(pool, meeting_id, error_msg).await
        {
            error!(
                "⚠️ Failed to update DB status to failed for {}: {}",
                meeting_id, e
            );
        }
    }
}
