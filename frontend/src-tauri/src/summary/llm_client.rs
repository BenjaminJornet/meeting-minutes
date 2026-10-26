use reqwest::{header, Client};
use serde::{Deserialize, Serialize};
use tracing::info;
use super::checkpoints::{SummaryOptions, endpoint_key};

// Generic structure for OpenAI-compatible API chat messages
#[derive(Debug, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

// Generic structure for OpenAI-compatible API chat requests
#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: usize,
}

// Generic structure for OpenAI-compatible API chat responses
#[derive(Deserialize, Debug)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize, Debug)]
pub struct Choice {
    pub finish_reason: Option<String>,
    pub message: MessageContent,
}

#[derive(Deserialize, Debug)]
pub struct MessageContent {
    pub content: String,
}

// Claude-specific request structure
#[derive(Debug, Serialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub max_tokens: u32,
    pub system: String,
    pub messages: Vec<ChatMessage>,
}

// Claude-specific response structure
#[derive(Deserialize, Debug)]
pub struct ClaudeChatResponse {
    pub content: Vec<ClaudeChatContent>,
    pub stop_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ClaudeChatContent {
    pub text: String,
}

/// LLM Provider enumeration for multi-provider support
#[derive(Debug, Clone, PartialEq)]
pub enum LLMProvider {
    OpenAI,
    Claude,
    Groq,
    Ollama,
    OpenRouter,
}

impl LLMProvider {
    /// Parse provider from string (case-insensitive)
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(Self::OpenAI),
            "claude" => Ok(Self::Claude),
            "groq" => Ok(Self::Groq),
            "ollama" => Ok(Self::Ollama),
            "openrouter" => Ok(Self::OpenRouter),
            _ => Err(format!("Unsupported LLM provider: {}", s)),
        }
    }
}

/// Generates a summary using the specified LLM provider
///
/// # Arguments
/// * `client` - Reqwest HTTP client (reused for performance)
/// * `provider` - The LLM provider to use
/// * `model_name` - The specific model to use (e.g., "gpt-4", "claude-3-opus")
/// * `api_key` - API key for the provider (not needed for Ollama)
/// * `system_prompt` - System instructions for the LLM
/// * `user_prompt` - User query/content to process
/// * `ollama_endpoint` - Optional custom Ollama endpoint (defaults to localhost:11434)
///
/// # Returns
/// The generated summary text or an error message
pub async fn generate_summary(
    client: &Client,
    provider: &LLMProvider,
    model_name: &str,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
    ollama_endpoint: Option<&str>,
    options: &SummaryOptions,
) -> Result<String, String> {
    let (api_url, mut headers) = match provider {
        LLMProvider::OpenAI => (
            "https://api.openai.com/v1/chat/completions".to_string(),
            header::HeaderMap::new(),
        ),
        LLMProvider::Groq => (
            "https://api.groq.com/openai/v1/chat/completions".to_string(),
            header::HeaderMap::new(),
        ),
        LLMProvider::OpenRouter => (
            "https://openrouter.ai/api/v1/chat/completions".to_string(),
            header::HeaderMap::new(),
        ),
        LLMProvider::Ollama => (format!("{}/api/chat", endpoint_key(ollama_endpoint)), header::HeaderMap::new()),
        LLMProvider::Claude => {
            let mut header_map = header::HeaderMap::new();
            header_map.insert(
                "x-api-key",
                api_key
                    .parse()
                    .map_err(|_| "Invalid API key format".to_string())?,
            );
            header_map.insert(
                "anthropic-version",
                "2023-06-01"
                    .parse()
                    .map_err(|_| "Invalid anthropic version".to_string())?,
            );
            ("https://api.anthropic.com/v1/messages".to_string(), header_map)
        }
    };

    // Add authorization header for non-Claude providers
    if provider != &LLMProvider::Claude && provider != &LLMProvider::Ollama {
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", api_key)
                .parse()
                .map_err(|_| "Invalid authorization header".to_string())?,
        );
    }
    headers.insert(
        header::CONTENT_TYPE,
        "application/json"
            .parse()
            .map_err(|_| "Invalid content type".to_string())?,
    );

    // Build request body based on provider
    let request_body = if provider == &LLMProvider::Ollama {
        serde_json::json!({
            "model": model_name, "stream": false, "think": false,
            "options": {"num_ctx": options.context_tokens, "num_predict": options.output_tokens},
            "messages": [{"role": "system", "content": system_prompt}, {"role": "user", "content": user_prompt}]
        })
    } else if provider != &LLMProvider::Claude {
        serde_json::json!(ChatRequest {
            model: model_name.to_string(),
            max_tokens: options.output_tokens,
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_prompt.to_string(),
                }
            ],
        })
    } else {
        serde_json::json!(ClaudeRequest {
            system: system_prompt.to_string(),
            model: model_name.to_string(),
            max_tokens: options.output_tokens as u32,
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            }]
        })
    };

    info!("🐞 LLM Request to {}: model={}", provider_name(provider), model_name);

    // Send request
    let response = client
        .post(api_url)
        .headers(headers)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| if e.is_timeout() { "upstream_llm_timeout: délai du modèle dépassé".to_string() }
            else { "upstream_llm_unavailable: modèle inaccessible".to_string() })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default().to_lowercase();
        if body.contains("context") || body.contains("token limit") {
            return Err("context_limit_exceeded: réduire la taille des morceaux ou augmenter le contexte".into());
        }
        return Err(format!("upstream_llm_error: HTTP {status}"));
    }
    if provider == &LLMProvider::Ollama {
        let body: serde_json::Value = response.json().await.map_err(|_| "llm_response_invalid: réponse Ollama invalide")?;
        if body.get("error").is_some() { return Err("upstream_llm_error: échec Ollama".into()); }
        if body["done"] != true || body["done_reason"].as_str() == Some("length") {
            return Err("llm_output_truncated: sortie interrompue, augmenter le budget de sortie".into());
        }
        if body["prompt_eval_count"].as_u64().is_some_and(|n| n as usize + options.output_tokens + 256 > options.context_tokens) {
            return Err("context_limit_exceeded: le prompt évalué ne réserve pas assez de place pour la sortie".into());
        }
        let content = body["message"]["content"].as_str().unwrap_or_default();
        return validated_content(content);
    }

    // Parse response based on provider
    if provider == &LLMProvider::Claude {
        let chat_response = response
            .json::<ClaudeChatResponse>()
            .await
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        info!("🐞 LLM Response received from Claude");

        if chat_response.stop_reason.as_deref() == Some("max_tokens") {
            return Err("llm_output_truncated: budget de sortie atteint".into());
        }
        let content = chat_response
            .content
            .get(0)
            .ok_or("No content in LLM response")?
            .text
            .trim();
        validated_content(content)
    } else {
        let chat_response = response
            .json::<ChatResponse>()
            .await
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        info!("🐞 LLM Response received from {}", provider_name(provider));

        if chat_response.choices.first().and_then(|c| c.finish_reason.as_deref()) == Some("length") {
            return Err("llm_output_truncated: budget de sortie atteint".into());
        }
        let content = chat_response
            .choices
            .get(0)
            .ok_or("No content in LLM response")?
            .message
            .content
            .trim();
        validated_content(content)
    }
}

/// Helper function to get provider name for logging
fn provider_name(provider: &LLMProvider) -> &str {
    match provider {
        LLMProvider::OpenAI => "OpenAI",
        LLMProvider::Claude => "Claude",
        LLMProvider::Groq => "Groq",
        LLMProvider::Ollama => "Ollama",
        LLMProvider::OpenRouter => "OpenRouter",
    }
}

fn validated_content(content: &str) -> Result<String, String> {
    let content = super::processor::clean_llm_markdown_output(content);
    if content.trim().is_empty() { Err("llm_response_empty: le modèle n’a pas produit de contenu".into()) }
    else { Ok(content) }
}
