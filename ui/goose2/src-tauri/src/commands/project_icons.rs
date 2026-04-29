use base64::{engine::general_purpose, Engine as _};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_ICON_CANDIDATES: usize = 18;
const MAX_PROJECT_ICON_BYTES: u64 = 512 * 1024;

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIconCandidate {
    pub id: String,
    pub label: String,
    pub path: String,
    pub icon: String,
    pub source_dir: String,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIconData {
    pub icon: String,
}

fn is_project_icon_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("svg" | "png" | "ico" | "jpg" | "jpeg" | "webp")
    )
}

fn is_ignored_icon_search_dir(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        matches!(
            name.as_str(),
            "node_modules" | "target" | "dist" | "build" | ".git" | ".next" | ".turbo"
        )
    })
}

fn is_generated_icon_variant(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let normalized = file_name.to_ascii_lowercase();
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mostly_size_token = stem
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, 'x' | '@' | '-' | '_'));

    normalized.starts_with("appicon-")
        || normalized.starts_with("square")
        || normalized.starts_with("storelogo")
        || normalized.contains("template")
        || normalized.contains("@2x")
        || normalized.contains("@3x")
        || mostly_size_token
        || stem
            .strip_prefix("icon-")
            .is_some_and(|suffix| suffix.chars().all(|c| c.is_ascii_digit()))
        || stem
            .strip_prefix("icon@")
            .is_some_and(|suffix| suffix.ends_with('x'))
}

fn is_likely_project_icon(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let normalized = file_name.to_ascii_lowercase();
    normalized == "favicon.ico"
        || normalized == "favicon.svg"
        || normalized == "favicon.png"
        || normalized.starts_with("apple-touch-icon")
        || normalized.starts_with("mstile-")
        || normalized.contains("logo")
        || normalized.contains("brand")
        || normalized.contains("wordmark")
        || normalized.contains("app-icon")
        || normalized.contains("appicon")
        || normalized.contains("icon")
}

fn project_icon_score(root: &Path, path: &Path) -> i32 {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_ascii_lowercase();

    let mut score = 100;
    if file_name.starts_with("favicon") {
        score -= 35;
    }
    if file_name.contains("logo") {
        score -= 30;
    }
    if file_name.contains("brand") || file_name.contains("wordmark") {
        score -= 25;
    }
    if relative.starts_with("public/")
        || relative.starts_with("static/")
        || relative.starts_with("assets/")
        || relative.starts_with("src/assets/")
        || relative.starts_with("src/images/")
    {
        score -= 20;
    }
    score + relative.matches('/').count() as i32
}

fn project_icon_group_key(path: &Path) -> String {
    let file_stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let normalized = file_stem
        .replace("goose-logo", "logo")
        .replace("logo-codename-goose", "logo")
        .replace("codename-goose", "logo")
        .replace("favicon", "favicon");

    if normalized.contains("favicon") {
        "favicon".to_string()
    } else if normalized.contains("wordmark") {
        "wordmark".to_string()
    } else if normalized.contains("brand") {
        "brand".to_string()
    } else if normalized.contains("logo") {
        "logo".to_string()
    } else if normalized.contains("app-icon") || normalized.contains("appicon") {
        "app-icon".to_string()
    } else {
        normalized
    }
}

fn read_project_icon_data_url(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|e| format!("Failed to inspect icon: {}", e))?;
    if !metadata.is_file() {
        return Err("Icon path is not a file".to_string());
    }
    if metadata.len() > MAX_PROJECT_ICON_BYTES {
        return Err("Icon file is too large".to_string());
    }

    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_string();
    if !matches!(
        mime.as_str(),
        "image/svg+xml"
            | "image/png"
            | "image/x-icon"
            | "image/vnd.microsoft.icon"
            | "image/jpeg"
            | "image/webp"
    ) {
        return Err("Icon file type is not supported".to_string());
    }

    let bytes = fs::read(path).map_err(|e| format!("Failed to read icon: {}", e))?;
    Ok(format!(
        "data:{};base64,{}",
        mime,
        general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub fn scan_project_icons(working_dirs: Vec<String>) -> Result<Vec<ProjectIconCandidate>, String> {
    let mut candidates: Vec<(i32, ProjectIconCandidate)> = Vec::new();
    let mut seen = HashSet::new();
    let mut seen_groups = HashSet::new();

    for dir in working_dirs {
        if candidates.len() >= MAX_ICON_CANDIDATES {
            break;
        }

        let root = PathBuf::from(dir.trim());
        if !root.is_dir() {
            continue;
        }

        let source_dir = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();

        let walker = ignore::WalkBuilder::new(&root)
            .max_depth(Some(6))
            .standard_filters(true)
            .build();

        for entry in walker.flatten() {
            if candidates.len() >= MAX_ICON_CANDIDATES {
                break;
            }

            let path = entry.path();
            if !path.is_file()
                || is_ignored_icon_search_dir(path)
                || is_generated_icon_variant(path)
                || !is_project_icon_extension(path)
                || !is_likely_project_icon(path)
            {
                continue;
            }

            let path_string = path.to_string_lossy().into_owned();
            if !seen.insert(path_string.clone()) {
                continue;
            }

            let group_key = format!("{}:{}", source_dir, project_icon_group_key(path));
            if !seen_groups.insert(group_key) {
                continue;
            }

            let icon = match read_project_icon_data_url(path) {
                Ok(icon) => icon,
                Err(_) => continue,
            };

            let relative = path.strip_prefix(&root).unwrap_or(path);
            let label = relative.to_string_lossy().into_owned();
            let score = project_icon_score(&root, path);
            candidates.push((
                score,
                ProjectIconCandidate {
                    id: path_string.clone(),
                    label,
                    path: path_string,
                    icon,
                    source_dir: source_dir.clone(),
                },
            ));
        }
    }

    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.label.cmp(&b.1.label)));
    Ok(candidates
        .into_iter()
        .take(MAX_ICON_CANDIDATES)
        .map(|(_, candidate)| candidate)
        .collect())
}

#[tauri::command]
pub fn read_project_icon(path: String) -> Result<ProjectIconData, String> {
    let path = PathBuf::from(path.trim());
    if !is_project_icon_extension(&path) {
        return Err("Icon file type is not supported".to_string());
    }
    let icon = read_project_icon_data_url(&path)?;
    Ok(ProjectIconData { icon })
}
