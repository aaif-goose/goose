use serde::Serialize;
use std::path::PathBuf;

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
