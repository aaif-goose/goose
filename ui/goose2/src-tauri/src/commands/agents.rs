use crate::services::personas::PersonaStore;
use crate::types::agents::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub fn list_personas(store: State<'_, PersonaStore>) -> Vec<Persona> {
    store.list()
}

#[tauri::command]
pub fn create_persona(
    store: State<'_, PersonaStore>,
    request: CreatePersonaRequest,
) -> Result<Persona, String> {
    store.create(request)
}

#[tauri::command]
pub fn update_persona(
    store: State<'_, PersonaStore>,
    id: String,
    request: UpdatePersonaRequest,
) -> Result<Persona, String> {
    store.update(&id, request)
}

#[tauri::command]
pub fn delete_persona(store: State<'_, PersonaStore>, id: String) -> Result<(), String> {
    store.delete(&id)
}

#[tauri::command]
pub fn refresh_personas(store: State<'_, PersonaStore>) -> Vec<Persona> {
    store.refresh_markdown()
}

/// Save avatar from a local file path for a persona.
/// Copies the file into ~/.goose/avatars/{persona_id}.{ext}.
/// Returns the stored filename (e.g. "persona-id.png").
#[tauri::command]
pub fn save_persona_avatar(persona_id: String, source_path: String) -> Result<String, String> {
    PersonaStore::save_avatar_from_path(&persona_id, &source_path)
}

/// Save avatar from raw bytes (for drag-and-drop from the browser).
#[tauri::command]
pub fn save_persona_avatar_bytes(
    persona_id: String,
    bytes: Vec<u8>,
    extension: String,
) -> Result<String, String> {
    PersonaStore::save_avatar_from_bytes(&persona_id, &bytes, &extension)
}

/// Returns the absolute path to the avatars directory (~/.goose/avatars/).
#[tauri::command]
pub fn get_avatars_dir() -> String {
    PersonaStore::avatars_dir().to_string_lossy().to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFileReadResult {
    pub file_bytes: Vec<u8>,
    pub file_name: String,
}

fn validate_import_persona_path(source_path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(source_path);

    if path.as_os_str().is_empty() {
        return Err("Selected file path is empty".to_string());
    }

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| "Unsupported file type. Expected a .md file.".to_string())?;
    if !extension.eq_ignore_ascii_case("md") {
        return Err("Unsupported file type. Expected a .md file.".to_string());
    }

    let metadata = std::fs::metadata(&path)
        .map_err(|err| format!("Failed to access import file '{}': {}", path.display(), err))?;
    if !metadata.is_file() {
        return Err(format!(
            "Selected import path '{}' is not a file",
            path.display()
        ));
    }

    Ok(path)
}

#[tauri::command]
pub fn read_import_persona_file(source_path: String) -> Result<ImportFileReadResult, String> {
    let path = validate_import_persona_path(&source_path)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Selected file is missing a valid filename".to_string())?
        .to_string();
    let file_bytes = std::fs::read(&path)
        .map_err(|err| format!("Failed to read import file '{}': {}", path.display(), err))?;

    Ok(ImportFileReadResult {
        file_bytes,
        file_name,
    })
}

// --- Markdown agent import ---

#[derive(Debug, Deserialize)]
struct MarkdownAgentFrontmatter {
    name: String,
    description: Option<String>,
    avatar: Option<Avatar>,
    provider: Option<String>,
    model: Option<String>,
}

fn parse_markdown_agent(content: &str) -> Result<CreatePersonaRequest, String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err("Missing frontmatter delimiter".to_string());
    }

    let after_first = &trimmed[3..];
    let end_idx = after_first
        .find("\n---")
        .ok_or_else(|| "Missing closing frontmatter delimiter".to_string())?;
    let yaml = &after_first[..end_idx];
    let body = after_first[end_idx + 4..].trim().to_string();
    let frontmatter: MarkdownAgentFrontmatter = serde_yaml::from_str(yaml)
        .map_err(|e| format!("Invalid frontmatter YAML: {}", e))?;

    if frontmatter.name.trim().is_empty() {
        return Err("Agent name cannot be empty".to_string());
    }

    let system_prompt = if body.is_empty() {
        frontmatter
            .description
            .clone()
            .unwrap_or_else(|| format!("You are {}.", frontmatter.name))
    } else {
        body
    };

    if system_prompt.trim().is_empty() {
        return Err("Agent prompt cannot be empty".to_string());
    }

    Ok(CreatePersonaRequest {
        display_name: frontmatter.name,
        avatar: frontmatter.avatar,
        system_prompt,
        provider: frontmatter.provider,
        model: frontmatter.model,
    })
}

/// Import an agent from its canonical Markdown format.
#[tauri::command]
pub fn import_personas(
    store: State<'_, PersonaStore>,
    file_bytes: Vec<u8>,
    file_name: String,
) -> Result<Vec<Persona>, String> {
    if !file_name.to_lowercase().ends_with(".md") {
        return Err("Unsupported file type. Expected a .md file.".to_string());
    }

    let content =
        String::from_utf8(file_bytes).map_err(|_| "File is not valid UTF-8 text".to_string())?;
    let request = parse_markdown_agent(&content)?;
    let persona = store.import_markdown(&request.display_name, &content)?;
    Ok(vec![persona])
}

#[cfg(test)]
mod tests {
    use super::validate_import_persona_path;

    #[test]
    fn validate_import_persona_path_rejects_non_markdown_files() {
        let path = std::env::temp_dir().join("persona-import.json");
        std::fs::write(&path, b"{}").unwrap();

        let result = validate_import_persona_path(path.to_str().unwrap());

        assert!(result.is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn validate_import_persona_path_rejects_directories() {
        let dir = std::env::temp_dir().join(format!("persona-import-dir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let result = validate_import_persona_path(dir.to_str().unwrap());

        assert!(result.is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn validate_import_persona_path_accepts_markdown_files() {
        let path = std::env::temp_dir().join(format!("persona-import-{}.md", std::process::id()));
        std::fs::write(&path, b"{}").unwrap();

        let validated = validate_import_persona_path(path.to_str().unwrap()).unwrap();

        assert_eq!(validated, path);
        let _ = std::fs::remove_file(validated);
    }
}
