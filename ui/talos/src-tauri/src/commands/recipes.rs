use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Minimal recipe file shape — only the fields we display or execute.
#[derive(Debug, Deserialize)]
struct RecipeFile {
    title: Option<String>,
    description: Option<String>,
    prompt: Option<String>,
    instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeInfo {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub path: String,
}

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Ok(env_val) = std::env::var("GOOSE_RECIPE_PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for part in env_val.split(sep) {
            if !part.is_empty() {
                dirs.push(PathBuf::from(part));
            }
        }
    }

    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".config").join("goose").join("recipes"));
        dirs.push(home.join(".agents").join("recipes"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join(".goose").join("recipes"));
        dirs.push(cwd.join(".agents").join("recipes"));
    }

    dirs
}

fn parse_recipe(path: &std::path::Path) -> Option<RecipeFile> {
    let text = fs::read_to_string(path).ok()?;
    serde_yaml::from_str::<RecipeFile>(&text).ok()
}

#[tauri::command]
pub async fn list_recipes() -> Result<Vec<RecipeInfo>, String> {
    let mut by_name: HashMap<String, RecipeInfo> = HashMap::new();

    for dir in candidate_dirs() {
        if !dir.exists() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(OsStr::to_str).unwrap_or("");
            if ext != "yaml" && ext != "yml" {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if by_name.contains_key(stem) {
                // Earlier candidates win (mirrors goose CLI precedence).
                continue;
            }
            let parsed = parse_recipe(&path);
            let info = RecipeInfo {
                id: stem.to_string(),
                name: stem.to_string(),
                title: parsed.as_ref().and_then(|r| r.title.clone()),
                description: parsed.as_ref().and_then(|r| r.description.clone()),
                path: path.display().to_string(),
            };
            by_name.insert(stem.to_string(), info);
        }
    }

    let mut out: Vec<RecipeInfo> = by_name.into_values().collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

/// Load a recipe file by path and return the prompt text to send as-is.
#[tauri::command]
pub async fn load_recipe_prompt(path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    let text = fs::read_to_string(&p).map_err(|e| format!("Failed to read {path}: {e}"))?;
    let parsed: RecipeFile =
        serde_yaml::from_str(&text).map_err(|e| format!("Failed to parse {path}: {e}"))?;

    // prompt wins; fall back to instructions; fall back to the raw file.
    if let Some(p) = parsed.prompt {
        return Ok(p);
    }
    if let Some(i) = parsed.instructions {
        return Ok(i);
    }
    Ok(text)
}
