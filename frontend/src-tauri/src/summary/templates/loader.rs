use super::defaults;
use super::types::Template;
use std::path::PathBuf;
use tracing::{debug, info, warn};
use once_cell::sync::Lazy;
use std::sync::RwLock;

// Global storage for the bundled templates directory path
static BUNDLED_TEMPLATES_DIR: Lazy<RwLock<Option<PathBuf>>> = Lazy::new(|| RwLock::new(None));

/// Get the configured language from environment variable MEETILY_LANGUAGE
/// Defaults to "en" if not set
fn get_language() -> String {
    std::env::var("MEETILY_LANGUAGE").unwrap_or_else(|_| "en".to_string())
}

/// Set the bundled templates directory path (called once at app startup)
pub fn set_bundled_templates_dir(path: PathBuf) {
    info!("Bundled templates directory set to: {:?}", path);
    if let Ok(mut dir) = BUNDLED_TEMPLATES_DIR.write() {
        *dir = Some(path);
    }
}

/// Get the user's custom templates directory path
///
/// Returns the platform-specific application data directory for custom templates:
/// - macOS: ~/Library/Application Support/Meetily/templates/
/// - Windows: %APPDATA%\Meetily\templates\
/// - Linux: ~/.config/Meetily/templates/
fn get_custom_templates_dir() -> Option<PathBuf> {
    let mut path = dirs::data_dir()?;
    path.push("Meetily");
    path.push("templates");
    Some(path)
}

/// Load a template from the bundled resources directory
///
/// This function implements language-aware loading:
/// 1. First tries to load from language-specific subdirectory (e.g., templates/fr/)
/// 2. Falls back to root templates directory
///
/// # Arguments
/// * `template_id` - Template identifier (without .json extension)
///
/// # Returns
/// The template JSON content if found, None otherwise
fn load_bundled_template(template_id: &str) -> Option<String> {
    let bundled_dir = BUNDLED_TEMPLATES_DIR.read().ok()?.clone()?;
    let language = get_language();
    
    // First try language-specific path (e.g., templates/fr/daily_standup.json)
    let lang_template_path = bundled_dir.join(&language).join(format!("{}.json", template_id));
    debug!("Checking for bundled template at language path: {:?}", lang_template_path);
    
    if let Ok(content) = std::fs::read_to_string(&lang_template_path) {
        info!("Loaded bundled template '{}' ({}) from {:?}", template_id, language, lang_template_path);
        return Some(content);
    }
    
    // Fall back to root templates directory
    let template_path = bundled_dir.join(format!("{}.json", template_id));
    debug!("Checking for bundled template at: {:?}", template_path);

    match std::fs::read_to_string(&template_path) {
        Ok(content) => {
            info!("Loaded bundled template '{}' from {:?}", template_id, template_path);
            Some(content)
        }
        Err(e) => {
            debug!("No bundled template '{}' found: {}", template_id, e);
            None
        }
    }
}

/// Load a template from the user's custom templates directory
///
/// This function implements language-aware loading:
/// 1. First tries to load from language-specific subdirectory (e.g., custom/fr/)
/// 2. Falls back to root custom templates directory
///
/// # Arguments
/// * `template_id` - Template identifier (without .json extension)
///
/// # Returns
/// The template JSON content if found, None otherwise
fn load_custom_template(template_id: &str) -> Option<String> {
    let custom_dir = get_custom_templates_dir()?;
    let language = get_language();
    
    // First try language-specific path (e.g., custom/fr/daily_standup.json)
    let lang_template_path = custom_dir.join(&language).join(format!("{}.json", template_id));
    debug!("Checking for custom template at language path: {:?}", lang_template_path);
    
    if let Ok(content) = std::fs::read_to_string(&lang_template_path) {
        info!("Loaded custom template '{}' ({}) from {:?}", template_id, language, lang_template_path);
        return Some(content);
    }
    
    // Fall back to root custom templates directory
    let template_path = custom_dir.join(format!("{}.json", template_id));
    debug!("Checking for custom template at: {:?}", template_path);

    match std::fs::read_to_string(&template_path) {
        Ok(content) => {
            info!("Loaded custom template '{}' from {:?}", template_id, template_path);
            Some(content)
        }
        Err(e) => {
            debug!("No custom template '{}' found: {}", template_id, e);
            None
        }
    }
}

/// Load and parse a template by identifier
///
/// This function implements a fallback strategy:
/// 1. Check user's custom templates directory
/// 2. Check bundled resources directory (app templates)
/// 3. Fall back to built-in embedded templates
/// 4. Return error if not found in any location
///
/// # Arguments
/// * `template_id` - Template identifier (e.g., "daily_standup", "standard_meeting")
///
/// # Returns
/// Parsed and validated Template struct
pub fn get_template(template_id: &str) -> Result<Template, String> {
    info!("Loading template: {}", template_id);

    // Try custom template first, then bundled, then built-in
    let json_content = if let Some(custom_content) = load_custom_template(template_id) {
        debug!("Using custom template for '{}'", template_id);
        custom_content
    } else if let Some(bundled_content) = load_bundled_template(template_id) {
        debug!("Using bundled template for '{}'", template_id);
        bundled_content
    } else if let Some(builtin_content) = defaults::get_builtin_template(template_id) {
        debug!("Using built-in template for '{}'", template_id);
        builtin_content.to_string()
    } else {
        return Err(format!(
            "Template '{}' not found. Available templates: {}",
            template_id,
            list_template_ids().join(", ")
        ));
    };

    // Parse and validate
    validate_and_parse_template(&json_content)
}

/// Validate and parse template JSON
///
/// # Arguments
/// * `json_content` - Raw JSON string
///
/// # Returns
/// Parsed and validated Template struct
pub fn validate_and_parse_template(json_content: &str) -> Result<Template, String> {
    let template: Template = serde_json::from_str(json_content)
        .map_err(|e| format!("Failed to parse template JSON: {}", e))?;

    template.validate()?;

    Ok(template)
}

/// Helper function to collect template IDs from a directory
fn collect_template_ids_from_dir(dir: &PathBuf, ids: &mut Vec<String>) {
    if !dir.exists() {
        return;
    }
    
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                if let Some(filename) = entry.file_name().to_str() {
                    if filename.ends_with(".json") {
                        let id = filename.trim_end_matches(".json").to_string();
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                    }
                }
            }
        }
        Err(e) => {
            debug!("Failed to read templates directory {:?}: {}", dir, e);
        }
    }
}

/// List all available template identifiers
///
/// Returns a combined list of:
/// - Built-in template IDs
/// - Bundled template IDs (from app resources, language-specific first)
/// - Custom template IDs (from user's data directory, language-specific first)
pub fn list_template_ids() -> Vec<String> {
    let mut ids: Vec<String> = defaults::list_builtin_template_ids()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    
    let language = get_language();

    // Add bundled templates if directory is set
    if let Ok(bundled_dir_lock) = BUNDLED_TEMPLATES_DIR.read() {
        if let Some(bundled_dir) = bundled_dir_lock.as_ref() {
            // First check language-specific directory
            let lang_dir = bundled_dir.join(&language);
            collect_template_ids_from_dir(&lang_dir, &mut ids);
            
            // Then check root directory
            collect_template_ids_from_dir(bundled_dir, &mut ids);
        }
    }

    // Add custom templates if directory exists
    if let Some(custom_dir) = get_custom_templates_dir() {
        // First check language-specific directory
        let lang_dir = custom_dir.join(&language);
        collect_template_ids_from_dir(&lang_dir, &mut ids);
        
        // Then check root directory
        collect_template_ids_from_dir(&custom_dir, &mut ids);
    }

    ids.sort();
    ids
}

/// List all available templates with their metadata
///
/// Returns a list of (id, name, description) tuples
pub fn list_templates() -> Vec<(String, String, String)> {
    let mut templates = Vec::new();

    for id in list_template_ids() {
        match get_template(&id) {
            Ok(template) => {
                templates.push((id, template.name, template.description));
            }
            Err(e) => {
                warn!("Failed to load template '{}': {}", id, e);
            }
        }
    }

    templates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_builtin_template() {
        let template = get_template("daily_standup");
        assert!(template.is_ok());

        let template = template.unwrap();
        assert_eq!(template.name, "Daily Standup");
        assert!(!template.sections.is_empty());
    }

    #[test]
    fn test_get_nonexistent_template() {
        let result = get_template("nonexistent_template");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_template_ids() {
        let ids = list_template_ids();
        assert!(ids.contains(&"daily_standup".to_string()));
        assert!(ids.contains(&"standard_meeting".to_string()));
    }

    #[test]
    fn test_validate_invalid_json() {
        let result = validate_and_parse_template("invalid json");
        assert!(result.is_err());
    }
}
