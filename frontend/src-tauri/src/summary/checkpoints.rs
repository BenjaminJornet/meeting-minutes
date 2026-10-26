use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SummaryOptions {
    pub context_tokens: usize,
    pub max_chunk_chars: usize,
    pub output_tokens: usize,
}
impl Default for SummaryOptions {
    fn default() -> Self { Self { context_tokens: 32768, max_chunk_chars: 30000, output_tokens: 4096 } }
}
impl SummaryOptions {
    pub fn validate(&self) -> Result<(), String> {
        if !(2048..=262144).contains(&self.context_tokens) || !(1000..=200000).contains(&self.max_chunk_chars)
            || !(256..=32768).contains(&self.output_tokens) || self.output_tokens >= self.context_tokens / 2 {
            return Err("summary_options_invalid: contexte, taille des morceaux ou sortie invalide".into());
        }
        Ok(())
    }
}

pub fn endpoint_key(endpoint: Option<&str>) -> String {
    endpoint.filter(|s| !s.trim().is_empty()).map(str::to_owned)
        .or_else(|| std::env::var("MEETILY_OLLAMA_URL").ok())
        .unwrap_or_else(|| "http://localhost:11434".into()).trim().trim_end_matches('/').to_string()
}
pub fn hash(text: &str) -> String { format!("{:x}", Sha256::digest(text.as_bytes())) }

pub async fn load_options(pool: &SqlitePool, endpoint: &str, provider: &str, model: &str) -> Result<SummaryOptions, String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT options_json FROM summary_profiles WHERE endpoint=? AND provider=? AND model=?")
        .bind(endpoint).bind(provider).bind(model).fetch_optional(pool).await.map_err(|e| e.to_string())?;
    let options = row.map(|(s,)| serde_json::from_str(&s).map_err(|e| e.to_string())).transpose()?.unwrap_or_default();
    Ok(options)
}
pub async fn save_options(pool: &SqlitePool, endpoint: &str, provider: &str, model: &str, options: &SummaryOptions) -> Result<(), String> {
    options.validate()?;
    sqlx::query("INSERT INTO summary_profiles(endpoint,provider,model,options_json) VALUES(?,?,?,?) ON CONFLICT(endpoint,provider,model) DO UPDATE SET options_json=excluded.options_json")
        .bind(endpoint).bind(provider).bind(model).bind(serde_json::to_string(options).map_err(|e| e.to_string())?)
        .execute(pool).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Clone)]
pub struct Checkpoints { pub pool: SqlitePool, pub meeting_id: String, pub run_key: String }
impl Checkpoints {
    pub async fn progress(&self, stage: &str, completed: usize, total: usize, error_code: Option<&str>) -> Result<(), String> {
        sqlx::query("INSERT INTO summary_progress(meeting_id,stage,completed,total,error_code) VALUES(?,?,?,?,?) ON CONFLICT(meeting_id) DO UPDATE SET stage=excluded.stage,completed=excluded.completed,total=excluded.total,error_code=excluded.error_code,updated_at=CURRENT_TIMESTAMP")
            .bind(&self.meeting_id).bind(stage).bind(completed as i64).bind(total as i64).bind(error_code)
            .execute(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(())
    }
    pub async fn get(&self, step: &str, input: &str) -> Result<Option<String>, String> {
        let row: Option<(String,)> = sqlx::query_as("SELECT output FROM summary_checkpoints WHERE run_key=? AND step_key=? AND input_hash=?")
            .bind(&self.run_key).bind(step).bind(hash(input)).fetch_optional(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(row.map(|(s,)| s))
    }
    pub async fn put(&self, step: &str, input: &str, output: &str) -> Result<(), String> {
        sqlx::query("INSERT OR REPLACE INTO summary_checkpoints(meeting_id,run_key,step_key,input_hash,output) VALUES(?,?,?,?,?)")
            .bind(&self.meeting_id).bind(&self.run_key).bind(step).bind(hash(input)).bind(output).execute(&self.pool).await.map_err(|e| e.to_string())?;
        Ok(())
    }
}

static RUNNING: once_cell::sync::Lazy<std::sync::Mutex<std::collections::HashSet<String>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));
pub struct SummaryRunGuard(String);
impl SummaryRunGuard {
    pub fn acquire(meeting_id: &str) -> Result<Self, String> {
        let mut running = RUNNING.lock().map_err(|_| "summary_lock_failed")?;
        if !running.insert(meeting_id.to_owned()) { return Err("summary_busy: génération déjà en cours".into()); }
        Ok(Self(meeting_id.to_owned()))
    }
}
impl Drop for SummaryRunGuard {
    fn drop(&mut self) { if let Ok(mut running) = RUNNING.lock() { running.remove(&self.0); } }
}
