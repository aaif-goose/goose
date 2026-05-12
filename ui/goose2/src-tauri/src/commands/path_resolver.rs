use std::path::PathBuf;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvePathRequest {
    pub parts: Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvePathResponse {
    pub path: String,
}

fn trim_part(part: &str) -> Option<&str> {
    let trimmed = part.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn expand_home_prefix(part: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match part {
        "~" => Some(home),
        _ => part
            .strip_prefix("~/")
            .or_else(|| part.strip_prefix("~\\"))
            .map(|relative| home.join(relative)),
    }
}

fn resolve_path_parts(parts: Vec<String>) -> Result<String, String> {
    let mut normalized_parts = parts.iter().filter_map(|part| trim_part(part)).peekable();

    let first = normalized_parts
        .next()
        .ok_or_else(|| "Path parts must include at least one non-empty segment".to_string())?;
    let mut path = expand_home_prefix(first).unwrap_or_else(|| PathBuf::from(first));

    for part in normalized_parts {
        path.push(part);
    }

    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn resolve_path(request: ResolvePathRequest) -> Result<ResolvePathResponse, String> {
    Ok(ResolvePathResponse {
        path: resolve_path_parts(request.parts)?,
    })
}

#[cfg(test)]
mod tests {
    use super::resolve_path_parts;

    #[test]
    fn joins_absolute_path_and_subpath() {
        assert_eq!(
            resolve_path_parts(vec!["/tmp/project".to_string(), "src".to_string()]),
            Ok("/tmp/project/src".to_string())
        );
    }

    #[test]
    fn ignores_empty_parts() {
        assert_eq!(
            resolve_path_parts(vec!["  ".to_string(), "/tmp/project".to_string()]),
            Ok("/tmp/project".to_string())
        );
    }

    #[test]
    fn expands_home_segments() {
        let Some(home) = dirs::home_dir() else {
            return;
        };

        assert_eq!(
            resolve_path_parts(vec!["~".to_string()]),
            Ok(home.to_string_lossy().into_owned())
        );
        assert_eq!(
            resolve_path_parts(vec!["~/Documents".to_string()]),
            Ok(home.join("Documents").to_string_lossy().into_owned())
        );
        assert_eq!(
            resolve_path_parts(vec!["~\\Documents".to_string()]),
            Ok(home.join("Documents").to_string_lossy().into_owned())
        );
    }

    #[test]
    fn errors_when_no_non_empty_parts_exist() {
        assert_eq!(
            resolve_path_parts(vec!["  ".to_string(), "".to_string()]),
            Err("Path parts must include at least one non-empty segment".to_string())
        );
    }
}
