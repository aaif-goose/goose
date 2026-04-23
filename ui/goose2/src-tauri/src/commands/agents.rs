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
        .ok_or_else(|| "Unsupported file type. Expected a .json file.".to_string())?;
    if !extension.eq_ignore_ascii_case("json") {
        return Err("Unsupported file type. Expected a .json file.".to_string());
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

// --- Sprout-compatible persona import/export ---

/// Sprout-compatible persona export format (version 1, camelCase keys).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersonaExportV1 {
    version: u32,
    display_name: String,
    system_prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    avatar: Option<Avatar>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

/// Result returned by export_persona containing the JSON string and a suggested filename.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub json: String,
    pub suggested_filename: String,
}

/// Convert a display name into a filesystem-safe slug.
/// Lowercase, replace non-alphanumeric with hyphens, collapse runs, trim, max 50 chars.
pub fn slugify(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();

    // Collapse consecutive hyphens
    let mut collapsed = String::with_capacity(slug.len());
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen {
                collapsed.push('-');
            }
            prev_hyphen = true;
        } else {
            collapsed.push(c);
            prev_hyphen = false;
        }
    }

    let trimmed = collapsed.trim_matches('-');
    let result = if trimmed.len() > 50 {
        // Cut at 50 chars without splitting mid-char, then trim trailing hyphens
        trimmed[..50].trim_end_matches('-').to_string()
    } else {
        trimmed.to_string()
    };

    if result.is_empty() {
        "persona".to_string()
    } else {
        result
    }
}

/// Export a persona as sprout-compatible JSON (version 1).
/// Returns the JSON string and a suggested filename.
#[tauri::command]
pub fn export_persona(store: State<'_, PersonaStore>, id: String) -> Result<ExportResult, String> {
    let persona = store
        .get(&id)
        .ok_or_else(|| format!("Persona '{}' not found", id))?;

    // For export, only include URL avatars (local files aren't portable)
    let export_avatar = match &persona.avatar {
        Some(Avatar::Url(url)) => Some(Avatar::Url(url.clone())),
        _ => None,
    };

    let export = PersonaExportV1 {
        version: 1,
        display_name: persona.display_name.clone(),
        system_prompt: persona.system_prompt,
        avatar: export_avatar,
        provider: persona.provider,
        model: persona.model,
    };

    let json = serde_json::to_string_pretty(&export)
        .map_err(|e| format!("Failed to serialize persona: {}", e))?;

    let slug = slugify(&persona.display_name);
    let suggested_filename = format!("{}.persona.json", slug);

    Ok(ExportResult {
        json,
        suggested_filename,
    })
}

/// Import personas from sprout-compatible JSON (version 1).
/// Accepts raw file bytes and the original filename.
/// Returns the list of newly created personas.
#[tauri::command]
pub fn import_personas(
    store: State<'_, PersonaStore>,
    file_bytes: Vec<u8>,
    file_name: String,
) -> Result<Vec<Persona>, String> {
    // Validate file extension
    if !file_name.ends_with(".persona.json") && !file_name.ends_with(".json") {
        return Err("Unsupported file type. Expected a .persona.json or .json file.".to_string());
    }

    // Parse the bytes as UTF-8
    let content =
        String::from_utf8(file_bytes).map_err(|_| "File is not valid UTF-8 text".to_string())?;

    // Parse as JSON
    let export: PersonaExportV1 =
        serde_json::from_str(&content).map_err(|e| format!("Invalid persona JSON: {}", e))?;

    // Validate version
    if export.version != 1 {
        return Err(format!(
            "Unsupported persona format version {}. Expected version 1.",
            export.version
        ));
    }

    // Validate required fields
    if export.display_name.trim().is_empty() {
        return Err("Persona displayName cannot be empty".to_string());
    }
    if export.system_prompt.trim().is_empty() {
        return Err("Persona systemPrompt cannot be empty".to_string());
    }

    // Create the persona via the store
    let request = CreatePersonaRequest {
        display_name: export.display_name,
        avatar: export.avatar,
        system_prompt: export.system_prompt,
        provider: export.provider,
        model: export.model,
    };

    let persona = store.create(request)?;
    Ok(vec![persona])
}

#[cfg(test)]
mod tests {
    use super::validate_import_persona_path;

    #[test]
    fn validate_import_persona_path_rejects_non_json_files() {
        let path = std::env::temp_dir().join("persona-import.txt");
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
    fn validate_import_persona_path_accepts_json_files() {
        let path = std::env::temp_dir().join(format!("persona-import-{}.json", std::process::id()));
        std::fs::write(&path, b"{}").unwrap();

        let validated = validate_import_persona_path(path.to_str().unwrap()).unwrap();

        assert_eq!(validated, path);
        let _ = std::fs::remove_file(validated);
    }
}
