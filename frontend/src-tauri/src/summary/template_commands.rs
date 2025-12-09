use crate::summary::templates;
use serde::{Deserialize, Serialize};
use tauri::Runtime;
use tracing::{info, warn};

/// Template metadata for UI display
#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateInfo {
    /// Template identifier (e.g., "daily_standup", "standard_meeting")
    pub id: String,

    /// Display name for the template
    pub name: String,

    /// Brief description of the template's purpose
    pub description: String,
}

/// Detailed template structure for preview/debugging
#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateDetails {
    /// Template identifier
    pub id: String,

    /// Display name
    pub name: String,

    /// Description
    pub description: String,

    /// List of section titles in order
    pub sections: Vec<String>,
}

/// Lists all available templates
///
/// Returns templates from both built-in (embedded) and custom (user data directory) sources.
/// Templates are automatically discovered - no code changes needed to add new templates.
///
/// # Returns
/// Vector of TemplateInfo with id, name, and description for each template
#[tauri::command]
pub async fn api_list_templates<R: Runtime>(
    _app: tauri::AppHandle<R>,
) -> Result<Vec<TemplateInfo>, String> {
    info!("api_list_templates called");

    let templates = templates::list_templates();

    let template_infos: Vec<TemplateInfo> = templates
        .into_iter()
        .map(|(id, name, description)| TemplateInfo {
            id,
            name,
            description,
        })
        .collect();

    info!("Found {} available templates", template_infos.len());

    Ok(template_infos)
}

/// Gets detailed information about a specific template
///
/// # Arguments
/// * `template_id` - Template identifier (e.g., "daily_standup")
///
/// # Returns
/// TemplateDetails with full template structure
#[tauri::command]
pub async fn api_get_template_details<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<TemplateDetails, String> {
    info!("api_get_template_details called for template_id: {}", template_id);

    let template = templates::get_template(&template_id)?;

    let section_titles: Vec<String> = template
        .sections
        .iter()
        .map(|section| section.title.clone())
        .collect();

    let details = TemplateDetails {
        id: template_id,
        name: template.name,
        description: template.description,
        sections: section_titles,
    };

    info!("Retrieved template details for '{}'", details.name);

    Ok(details)
}

/// Validates a custom template JSON string
///
/// Useful for template editor UI or validation before saving custom templates
///
/// # Arguments
/// * `template_json` - Raw JSON string of the template
///
/// # Returns
/// Ok(template_name) if valid, Err(error_message) if invalid
#[tauri::command]
pub async fn api_validate_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_json: String,
) -> Result<String, String> {
    info!("api_validate_template called");

    match templates::validate_and_parse_template(&template_json) {
        Ok(template) => {
            info!("Template '{}' validated successfully", template.name);
            Ok(template.name)
        }
        Err(e) => {
            warn!("Template validation failed: {}", e);
            Err(e)
        }
    }
}

/// Get the directory path for custom templates
fn get_custom_templates_dir() -> Result<std::path::PathBuf, String> {
    let mut path = dirs::data_dir()
        .ok_or_else(|| "Could not find data directory".to_string())?;
    path.push("Meetily");
    path.push("templates");
    Ok(path)
}

/// Saves a custom template to the user's templates directory
///
/// # Arguments
/// * `template_id` - Template identifier (will be filename without .json)
/// * `template_json` - Full template JSON content
///
/// # Returns
/// Ok(template_name) if saved successfully
#[tauri::command]
pub async fn api_save_custom_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
    template_json: String,
) -> Result<String, String> {
    info!("api_save_custom_template called for template_id: {}", template_id);

    // Validate template first
    let template = templates::validate_and_parse_template(&template_json)?;

    // Ensure custom templates directory exists
    let custom_dir = get_custom_templates_dir()?;
    std::fs::create_dir_all(&custom_dir)
        .map_err(|e| format!("Failed to create templates directory: {}", e))?;

    // Save template file
    let template_path = custom_dir.join(format!("{}.json", template_id));
    std::fs::write(&template_path, &template_json)
        .map_err(|e| format!("Failed to save template: {}", e))?;

    info!("Saved custom template '{}' to {:?}", template.name, template_path);
    Ok(template.name)
}

/// Deletes a custom template from the user's templates directory
///
/// # Arguments
/// * `template_id` - Template identifier to delete
///
/// # Returns
/// Ok(()) if deleted successfully, Err if not found or built-in template
#[tauri::command]
pub async fn api_delete_custom_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<(), String> {
    info!("api_delete_custom_template called for template_id: {}", template_id);

    let custom_dir = get_custom_templates_dir()?;
    let template_path = custom_dir.join(format!("{}.json", template_id));

    if !template_path.exists() {
        return Err(format!("Custom template '{}' not found. Built-in templates cannot be deleted.", template_id));
    }

    std::fs::remove_file(&template_path)
        .map_err(|e| format!("Failed to delete template: {}", e))?;

    info!("Deleted custom template '{}'", template_id);
    Ok(())
}

/// Gets the raw JSON content of a template for editing
///
/// # Arguments
/// * `template_id` - Template identifier
///
/// # Returns
/// The raw JSON content of the template
#[tauri::command]
pub async fn api_get_template_json<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<String, String> {
    info!("api_get_template_json called for template_id: {}", template_id);

    // Try custom template first
    let custom_dir = get_custom_templates_dir()?;
    let custom_path = custom_dir.join(format!("{}.json", template_id));
    
    if custom_path.exists() {
        let content = std::fs::read_to_string(&custom_path)
            .map_err(|e| format!("Failed to read template: {}", e))?;
        return Ok(content);
    }

    // Try bundled/built-in templates
    let template = templates::get_template(&template_id)?;
    let json = serde_json::to_string_pretty(&template)
        .map_err(|e| format!("Failed to serialize template: {}", e))?;
    
    Ok(json)
}

/// Gets the custom templates directory path
#[tauri::command]
pub fn api_get_templates_directory() -> Result<String, String> {
    let dir = get_custom_templates_dir()?;
    Ok(dir.to_string_lossy().to_string())
}

/// Checks if a template is a custom (user) template or built-in
#[tauri::command]
pub async fn api_is_custom_template<R: Runtime>(
    _app: tauri::AppHandle<R>,
    template_id: String,
) -> Result<bool, String> {
    let custom_dir = get_custom_templates_dir()?;
    let custom_path = custom_dir.join(format!("{}.json", template_id));
    Ok(custom_path.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_templates() {
        // This test requires the templates to be embedded/available
        // In a real test environment, you might want to mock the templates module

        // For now, just verify the function compiles and runs
        // You can expand this with more specific assertions
    }

    #[tokio::test]
    async fn test_validate_template_valid() {
        let valid_json = r#"
        {
            "name": "Test Template",
            "description": "A test template",
            "sections": [
                {
                    "title": "Summary",
                    "instruction": "Provide a summary",
                    "format": "paragraph"
                }
            ]
        }"#;

        // Mock app handle would be needed for actual testing
        // For now, test the validation logic directly
        let result = templates::validate_and_parse_template(valid_json);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_template_invalid() {
        let invalid_json = "invalid json";

        let result = templates::validate_and_parse_template(invalid_json);
        assert!(result.is_err());
    }
}
