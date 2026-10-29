use anyhow::Result;
use goose_agent::extension::ExtensionConfig;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct BundledExtensionEntry {
    id: String,
    name: String,
    #[serde(rename = "type")]
    extension_type: String,
    #[allow(dead_code)]
    #[serde(default)]
    enabled: bool,
}

pub fn validate_bundled_extensions(path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path)?;
    let raw_entries: Vec<serde_json::Value> = serde_json::from_str(&content)?;
    let total = raw_entries.len();
    let mut errors: Vec<String> = Vec::new();

    for (index, entry) in raw_entries.iter().enumerate() {
        let meta: BundledExtensionEntry = match serde_json::from_value(entry.clone()) {
            Ok(m) => m,
            Err(e) => {
                let id = entry
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let name = entry
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                errors.push(format!(
                    "[{index}] {name} (id={id}): missing required metadata fields: {e}"
                ));
                continue;
            }
        };

        // Check for common field name mistakes before full deserialization
        if meta.extension_type == "streamable_http"
            && entry.get("url").is_some()
            && entry.get("uri").is_none()
        {
            errors.push(format!(
                "[{index}] {} (id={}): has \"url\" field but streamable_http expects \"uri\" — did you mean \"uri\"?",
                meta.name, meta.id
            ));
            continue;
        }

        if meta.extension_type == "stdio" && entry.get("cmd").is_none() {
            errors.push(format!(
                "[{index}] {} (id={}): stdio extension is missing required \"cmd\" field",
                meta.name, meta.id
            ));
            continue;
        }

        if let Err(e) = serde_json::from_value::<ExtensionConfig>(entry.clone()) {
            errors.push(format!("[{index}] {} (id={}): {e}", meta.name, meta.id));
        }
    }

    if errors.is_empty() {
        Ok(format!("✓ All {total} extensions validated successfully."))
    } else {
        let mut output = format!("✗ Found {} error(s) in {total} extensions:\n", errors.len());
        for error in &errors {
            output.push_str(&format!("\n  {error}"));
        }
        anyhow::bail!("{output}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_json(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn test_valid_builtin() {
        let f = write_json(
            r#"[{
            "id": "developer",
            "name": "developer",
            "display_name": "Developer",
            "description": "Dev tools",
            "enabled": true,
            "type": "builtin",
            "timeout": 300,
            "bundled": true
        }]"#,
        );
        let result = validate_bundled_extensions(f.path());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("1 extensions validated"));
    }

    #[test]
    fn test_valid_stdio() {
        let f = write_json(
            r#"[{
            "id": "googledrive",
            "name": "Google Drive",
            "description": "Google Drive integration",
            "enabled": false,
            "type": "stdio",
            "cmd": "uvx",
            "args": ["mcp_gdrive@latest"],
            "env_keys": [],
            "timeout": 300,
            "bundled": true
        }]"#,
        );
        let result = validate_bundled_extensions(f.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_valid_streamable_http() {
        let f = write_json(
            r#"[{
            "id": "asana",
            "name": "Asana",
            "display_name": "Asana",
            "description": "Manage Asana tasks",
            "enabled": false,
            "type": "streamable_http",
            "uri": "https://mcp.asana.com/mcp",
            "env_keys": [],
            "timeout": 300,
            "bundled": true
        }]"#,
        );
        let result = validate_bundled_extensions(f.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_type_http() {
        let f = write_json(
            r#"[{
            "id": "asana",
            "name": "Asana",
            "description": "Manage Asana tasks",
            "enabled": false,
            "type": "http",
            "uri": "https://mcp.asana.com/mcp",
            "timeout": 300,
            "bundled": true
        }]"#,
        );
        let result = validate_bundled_extensions(f.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Asana"));
        assert!(err.contains("unknown variant `http`"));
    }

    #[test]
    fn test_url_instead_of_uri() {
        let f = write_json(
            r#"[{
            "id": "neighborhood",
            "name": "Neighborhood",
            "description": "Neighborhood tools",
            "enabled": false,
            "type": "streamable_http",
            "url": "https://example.com/mcp",
            "timeout": 300,
            "bundled": true
        }]"#,
        );
        let result = validate_bundled_extensions(f.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("uri"));
    }

    #[test]
    fn test_missing_cmd_for_stdio() {
        let f = write_json(
            r#"[{
            "id": "test",
            "name": "Test",
            "description": "Test extension",
            "enabled": false,
            "type": "stdio",
            "args": [],
            "timeout": 300,
            "bundled": true
        }]"#,
        );
        let result = validate_bundled_extensions(f.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cmd"));
    }

    #[test]
    fn test_valid_entries_before_invalid_still_pass() {
        let f = write_json(
            r#"[
            {
                "id": "developer",
                "name": "developer",
                "description": "Dev tools",
                "enabled": true,
                "type": "builtin",
                "timeout": 300,
                "bundled": true
            },
            {
                "id": "bad",
                "name": "Bad Extension",
                "description": "This one is broken",
                "enabled": false,
                "type": "http",
                "uri": "https://example.com",
                "timeout": 300,
                "bundled": true
            }
        ]"#,
        );
        let result = validate_bundled_extensions(f.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("1 error(s)"));
        assert!(err.contains("Bad Extension"));
    }

    #[test]
    fn test_empty_array_is_valid() {
        let f = write_json("[]");
        let result = validate_bundled_extensions(f.path());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("0 extensions validated"));
    }
}
