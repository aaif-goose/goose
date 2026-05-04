use crate::types::agents::{
    builtin_personas, Avatar, CreatePersonaRequest, Persona, UpdatePersonaRequest,
};
use log::warn;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

pub struct PersonaStore {
    personas: Mutex<Vec<Persona>>,
}

/// YAML frontmatter fields parsed from markdown persona files.
#[derive(Clone, Default, Deserialize, Serialize)]
struct MarkdownFrontmatter {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    avatar: Option<Avatar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

impl PersonaStore {
    pub fn new() -> Self {
        let store_path = Self::store_path();
        Self::migrate_legacy_markdown_agents();
        Self::migrate_legacy_personas_json(&store_path);
        Self::ensure_seed_agents();
        let merged = Self::load_markdown_personas();
        Self { personas: Mutex::new(merged) }
    }

    fn store_path() -> PathBuf {
        let base = dirs::home_dir().expect("home dir");
        base.join(".goose").join("personas.json")
    }

    fn migration_marker_path() -> PathBuf {
        Self::agents_dir().join(".personas-json-migrated")
    }

    fn seed_marker_path() -> PathBuf {
        Self::agents_dir().join(".seed-agents-installed")
    }

    fn legacy_markdown_migration_marker_path() -> PathBuf {
        Self::agents_dir().join(".goose-agents-migrated")
    }

    /// Path to the avatars directory (~/.goose/avatars/).
    pub fn avatars_dir() -> PathBuf {
        dirs::home_dir()
            .expect("home dir")
            .join(".goose")
            .join("avatars")
    }

    fn load_legacy_json(path: &PathBuf) -> Vec<Persona> {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// Canonical directory containing global markdown agent files.
    fn agents_dir() -> PathBuf {
        dirs::home_dir()
            .expect("home dir")
            .join(".agents")
            .join("agents")
    }

    /// Previous markdown-agent location. Read only for one-time migration.
    fn legacy_agents_dir() -> PathBuf {
        dirs::home_dir()
            .expect("home dir")
            .join(".goose")
            .join("agents")
    }

    fn ensure_seed_agents() {
        let dir = Self::agents_dir();
        if let Err(err) = std::fs::create_dir_all(&dir) {
            warn!("Failed to create agents directory {:?}: {}", dir, err);
            return;
        }
        if Self::seed_marker_path().exists() {
            return;
        }

        for persona in builtin_personas() {
            let path = dir.join(format!("{}.md", Self::slugify_name(&persona.display_name)));
            if path.exists() {
                continue;
            }
            if let Err(err) = std::fs::write(&path, Self::persona_to_markdown(&persona)) {
                warn!("Failed to seed agent {:?}: {}", path, err);
            }
        }
        let _ = std::fs::write(Self::seed_marker_path(), chrono::Utc::now().to_rfc3339());
    }

    fn migrate_legacy_markdown_agents() {
        let legacy_dir = Self::legacy_agents_dir();
        if !legacy_dir.is_dir() || Self::legacy_markdown_migration_marker_path().exists() {
            return;
        }

        let target_dir = Self::agents_dir();
        if let Err(err) = std::fs::create_dir_all(&target_dir) {
            warn!("Failed to create agents directory {:?}: {}", target_dir, err);
            return;
        }

        let entries = match std::fs::read_dir(&legacy_dir) {
            Ok(entries) => entries,
            Err(err) => {
                warn!("Failed to read legacy agents directory {:?}: {}", legacy_dir, err);
                return;
            }
        };

        for entry in entries.flatten() {
            let source = entry.path();
            if source.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }

            let Some(file_name) = source.file_name() else {
                continue;
            };
            let destination =
                Self::unique_agent_path(&target_dir, file_name.to_string_lossy().as_ref());
            if let Err(err) = std::fs::copy(&source, &destination) {
                warn!(
                    "Failed to migrate legacy agent {:?} to {:?}: {}",
                    source, destination, err
                );
            }
        }
        let _ = std::fs::write(
            Self::legacy_markdown_migration_marker_path(),
            chrono::Utc::now().to_rfc3339(),
        );
    }

    fn migrate_legacy_personas_json(path: &PathBuf) {
        if !path.is_file() || Self::migration_marker_path().exists() {
            return;
        }

        let personas = Self::load_legacy_json(path);
        let dir = Self::agents_dir();
        if let Err(err) = std::fs::create_dir_all(&dir) {
            warn!("Failed to create agents directory {:?}: {}", dir, err);
            return;
        }

        if personas.is_empty() {
            let _ = std::fs::write(Self::migration_marker_path(), "");
            return;
        }

        for persona in personas {
            let filename = format!("{}.md", Self::slugify_name(&persona.display_name));
            let path = Self::unique_agent_path(&dir, &filename);
            if let Err(err) = std::fs::write(&path, Self::persona_to_markdown(&persona)) {
                warn!("Failed to migrate persona '{}' to markdown: {}", persona.display_name, err);
            }
        }

        let _ = std::fs::write(Self::migration_marker_path(), chrono::Utc::now().to_rfc3339());
    }

    fn unique_agent_path(dir: &Path, filename: &str) -> PathBuf {
        let path = dir.join(filename);
        if !path.exists() {
            return path;
        }

        let stem = Path::new(filename)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("agent");
        let extension = Path::new(filename)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("md");

        for counter in 2.. {
            let candidate = dir.join(format!("{}-{}.{}", stem, counter, extension));
            if !candidate.exists() {
                return candidate;
            }
        }

        unreachable!("counter loop always returns");
    }

    /// Scan the canonical global agents directory and parse each Markdown file into a Persona.
    fn load_markdown_personas() -> Vec<Persona> {
        let dir = Self::agents_dir();
        if !dir.is_dir() {
            return Vec::new();
        }

        let mut personas = Vec::new();

        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(err) => {
                warn!("Failed to read agents directory {:?}: {}", dir, err);
                return Vec::new();
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }

            match Self::parse_markdown_persona(&path) {
                Ok(persona) => personas.push(persona),
                Err(err) => {
                    warn!("Skipping {:?}: {}", path, err);
                }
            }
        }

        personas
    }

    /// Parse a single markdown file with YAML frontmatter into a Persona.
    fn parse_markdown_persona(path: &std::path::Path) -> Result<Persona, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

        let (yaml_str, body) = Self::split_markdown_persona(&content)?;

        let frontmatter: MarkdownFrontmatter = serde_yaml::from_str(yaml_str)
            .map_err(|e| format!("Invalid frontmatter YAML: {}", e))?;

        // Derive a stable ID from the filename (without extension)
        let slug = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        let id = format!("md-{}", slug);

        // Use the file modification time for timestamps, fall back to now
        let mod_time = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| {
                let duration = t.duration_since(std::time::UNIX_EPOCH).ok()?;
                let dt = chrono::DateTime::from_timestamp(
                    duration.as_secs() as i64,
                    duration.subsec_nanos(),
                )?;
                Some(dt.to_rfc3339())
            })
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

        // Use the body as system prompt. If body is empty, use description or a fallback.
        let system_prompt = if body.is_empty() {
            frontmatter
                .description
                .clone()
                .unwrap_or_else(|| format!("You are {}.", frontmatter.name))
        } else {
            body
        };

        Ok(Persona {
            id,
            display_name: frontmatter.name,
            avatar: frontmatter.avatar,
            system_prompt,
            provider: frontmatter.provider,
            model: frontmatter.model,
            is_builtin: false,
            is_from_disk: true,
            source_path: Some(path.to_string_lossy().to_string()),
            created_at: mod_time.clone(),
            updated_at: mod_time,
        })
    }

    fn split_markdown_persona(content: &str) -> Result<(&str, String), String> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return Err("Missing frontmatter delimiter".to_string());
        }

        let after_first = &trimmed[3..];
        let end_idx = after_first
            .find("\n---")
            .ok_or_else(|| "Missing closing frontmatter delimiter".to_string())?;

        let yaml_str = &after_first[..end_idx];
        let body = after_first[end_idx + 4..].trim().to_string();

        Ok((yaml_str, body))
    }

    fn slugify_name(name: &str) -> String {
        let mut slug = String::new();
        let mut previous_hyphen = false;

        for ch in name.to_lowercase().chars() {
            let next = if ch.is_ascii_alphanumeric() {
                Some(ch)
            } else if ch.is_whitespace() || ch == '-' || ch == '_' {
                Some('-')
            } else {
                None
            };

            if let Some(ch) = next {
                if ch == '-' {
                    if !previous_hyphen && !slug.is_empty() {
                        slug.push(ch);
                    }
                    previous_hyphen = true;
                } else {
                    slug.push(ch);
                    previous_hyphen = false;
                }
            }
        }

        let slug = slug.trim_matches('-');
        if slug.is_empty() {
            "agent".to_string()
        } else {
            slug.chars().take(64).collect()
        }
    }

    fn persona_to_frontmatter(persona: &Persona) -> MarkdownFrontmatter {
        MarkdownFrontmatter {
            name: persona.display_name.clone(),
            description: None,
            avatar: persona.avatar.clone(),
            provider: persona.provider.clone(),
            model: persona.model.clone(),
        }
    }

    fn markdown_from_parts(frontmatter: &MarkdownFrontmatter, body: &str) -> Result<String, String> {
        let yaml = serde_yaml::to_string(frontmatter)
            .map_err(|e| format!("Failed to serialize frontmatter: {}", e))?;
        let body = body.trim();
        if body.is_empty() {
            Ok(format!("---\n{}---\n", yaml))
        } else {
            Ok(format!("---\n{}---\n\n{}\n", yaml, body))
        }
    }

    fn persona_to_markdown(persona: &Persona) -> String {
        Self::markdown_from_parts(
            &Self::persona_to_frontmatter(persona),
            &persona.system_prompt,
        )
        .unwrap_or_else(|_| {
            format!(
                "---\nname: {}\n---\n\n{}\n",
                persona.display_name, persona.system_prompt
            )
        })
    }

    fn update_markdown_persona_file(
        id: &str,
        req: &UpdatePersonaRequest,
    ) -> Result<Persona, String> {
        let path = Self::markdown_persona_path(id)?;
        let content =
            std::fs::read_to_string(&path).map_err(|e| format!("Failed to read file: {}", e))?;
        let (yaml_str, current_body) = Self::split_markdown_persona(&content)?;

        let original_frontmatter: MarkdownFrontmatter = serde_yaml::from_str(yaml_str)
            .map_err(|e| format!("Invalid frontmatter YAML: {}", e))?;
        let mut frontmatter = original_frontmatter.clone();

        if let Some(name) = &req.display_name {
            frontmatter.name = name.clone();
        }
        if let Some(avatar) = &req.avatar {
            frontmatter.avatar = avatar.clone();
        }
        if let Some(provider) = &req.provider {
            frontmatter.provider = Some(provider.clone());
        }
        if let Some(model) = &req.model {
            frontmatter.model = Some(model.clone());
        }

        let current_system_prompt = if current_body.is_empty() {
            original_frontmatter
                .description
                .clone()
                .unwrap_or_else(|| format!("You are {}.", original_frontmatter.name))
        } else {
            current_body.clone()
        };
        let body = match &req.system_prompt {
            Some(prompt) if current_body.is_empty() && prompt.trim() == current_system_prompt => {
                String::new()
            }
            Some(prompt) => prompt.trim().to_string(),
            None => current_body,
        };
        let next_content = Self::markdown_from_parts(&frontmatter, &body)?;

        std::fs::write(&path, next_content)
            .map_err(|e| format!("Failed to write file '{}': {}", path.display(), e))?;

        Self::parse_markdown_persona(&path)
    }

    fn markdown_persona_path(id: &str) -> Result<PathBuf, String> {
        let slug = id
            .strip_prefix("md-")
            .ok_or_else(|| format!("Persona '{}' is not a file-backed persona", id))?;
        Self::validate_markdown_persona_slug(slug)?;
        Ok(Self::agents_dir().join(format!("{}.md", slug)))
    }

    fn validate_markdown_persona_slug(slug: &str) -> Result<(), String> {
        if slug.chars().any(|c| matches!(c, '/' | '\\')) {
            return Err(format!("Persona '{}' has an invalid file-backed ID", slug));
        }

        let mut components = Path::new(slug).components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(_)), None) => Ok(()),
            _ => Err(format!("Persona '{}' has an invalid file-backed ID", slug)),
        }
    }

    /// Re-scan markdown personas and update the in-memory list.
    /// Returns the full updated persona list.
    pub fn refresh_markdown(&self) -> Vec<Persona> {
        let markdown = Self::load_markdown_personas();

        let mut personas = self.personas.lock().unwrap();
        *personas = markdown;
        personas.clone()
    }

    pub fn list(&self) -> Vec<Persona> {
        let personas = self.personas.lock().unwrap();
        personas.clone()
    }

    #[allow(dead_code)]
    pub fn get(&self, id: &str) -> Option<Persona> {
        let personas = self.personas.lock().unwrap();
        personas.iter().find(|p| p.id == id).cloned()
    }

    pub fn create(&self, req: CreatePersonaRequest) -> Result<Persona, String> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut persona = Persona {
            id: uuid::Uuid::new_v4().to_string(),
            display_name: req.display_name,
            avatar: req.avatar,
            system_prompt: req.system_prompt,
            provider: req.provider,
            model: req.model,
            is_builtin: false,
            is_from_disk: true,
            source_path: None,
            created_at: now.clone(),
            updated_at: now,
        };
        let agents_dir = Self::agents_dir();
        std::fs::create_dir_all(&agents_dir)
            .map_err(|e| format!("Failed to create agents directory: {}", e))?;
        let filename = format!("{}.md", Self::slugify_name(&persona.display_name));
        let path = Self::unique_agent_path(&agents_dir, &filename);
        std::fs::write(&path, Self::persona_to_markdown(&persona))
            .map_err(|e| format!("Failed to write agent file '{}': {}", path.display(), e))?;
        persona = Self::parse_markdown_persona(&path)?;

        let mut personas = self.personas.lock().unwrap();
        personas.push(persona.clone());
        Ok(persona)
    }

    pub fn import_markdown(
        &self,
        display_name: &str,
        markdown: &str,
    ) -> Result<Persona, String> {
        let agents_dir = Self::agents_dir();
        std::fs::create_dir_all(&agents_dir)
            .map_err(|e| format!("Failed to create agents directory: {}", e))?;
        let filename = format!("{}.md", Self::slugify_name(display_name));
        let path = Self::unique_agent_path(&agents_dir, &filename);
        std::fs::write(&path, markdown)
            .map_err(|e| format!("Failed to write agent file '{}': {}", path.display(), e))?;
        let persona = Self::parse_markdown_persona(&path)?;

        let mut personas = self.personas.lock().unwrap();
        personas.push(persona.clone());
        Ok(persona)
    }

    pub fn update(&self, id: &str, req: UpdatePersonaRequest) -> Result<Persona, String> {
        let mut personas = self.personas.lock().unwrap();
        let persona = personas
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("Persona '{}' not found", id))?;

        if persona.is_from_disk {
            let updated = Self::update_markdown_persona_file(id, &req)?;
            *persona = updated.clone();
            return Ok(updated);
        }

        if let Some(name) = req.display_name {
            persona.display_name = name;
        }
        if let Some(avatar_value) = req.avatar {
            // Some(None) → clear, Some(Some(a)) → set
            persona.avatar = avatar_value;
        }
        if let Some(prompt) = req.system_prompt {
            persona.system_prompt = prompt;
        }
        if let Some(provider) = req.provider {
            persona.provider = Some(provider);
        }
        if let Some(model) = req.model {
            persona.model = Some(model);
        }
        persona.updated_at = chrono::Utc::now().to_rfc3339();

        let updated = persona.clone();
        Ok(updated)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut personas = self.personas.lock().unwrap();

        let persona = personas
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| format!("Persona '{}' not found", id))?;

        if persona.is_from_disk {
            let path = Self::markdown_persona_path(id)?;
            match std::fs::remove_file(&path) {
                Ok(_) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(format!(
                        "Failed to delete file-backed persona '{}': {}",
                        path.display(),
                        err
                    ));
                }
            }

            personas.retain(|p| p.id != id);
            return Ok(());
        }

        // Clean up local avatar file if present
        if let Some(Avatar::Local(filename)) = &persona.avatar {
            let path = Self::avatars_dir().join(filename);
            let _ = std::fs::remove_file(path);
        }

        personas.retain(|p| p.id != id);
        Ok(())
    }

    /// Copy an avatar image from a source path to ~/.goose/avatars/{persona_id}.{ext}.
    /// Returns the filename (not full path).
    pub fn save_avatar_from_path(persona_id: &str, source_path: &str) -> Result<String, String> {
        let avatars_dir = Self::avatars_dir();
        std::fs::create_dir_all(&avatars_dir)
            .map_err(|e| format!("Failed to create avatars directory: {}", e))?;

        let source = std::path::Path::new(source_path);

        // Extract extension from source filename
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_lowercase();

        let stored_name = format!("{}.{}", persona_id, ext);
        let dest = avatars_dir.join(&stored_name);

        // Remove any existing avatar for this persona (different extension)
        if let Ok(entries) = std::fs::read_dir(&avatars_dir) {
            let prefix = format!("{}.", persona_id);
            for entry in entries.flatten() {
                let name = entry.file_name();
                if let Some(name_str) = name.to_str() {
                    if name_str.starts_with(&prefix) && name_str != stored_name {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }

        std::fs::copy(source, &dest).map_err(|e| format!("Failed to copy avatar file: {}", e))?;

        Ok(stored_name)
    }

    /// Write avatar image bytes to ~/.goose/avatars/{persona_id}.{ext}.
    /// Returns the filename (not full path).
    pub fn save_avatar_from_bytes(
        persona_id: &str,
        bytes: &[u8],
        extension: &str,
    ) -> Result<String, String> {
        let avatars_dir = Self::avatars_dir();
        std::fs::create_dir_all(&avatars_dir)
            .map_err(|e| format!("Failed to create avatars directory: {}", e))?;

        let ext = extension.to_lowercase();
        let stored_name = format!("{}.{}", persona_id, ext);
        let dest = avatars_dir.join(&stored_name);

        // Remove any existing avatar for this persona (different extension)
        if let Ok(entries) = std::fs::read_dir(&avatars_dir) {
            let prefix = format!("{}.", persona_id);
            for entry in entries.flatten() {
                let name = entry.file_name();
                if let Some(name_str) = name.to_str() {
                    if name_str.starts_with(&prefix) && name_str != stored_name {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }

        std::fs::write(&dest, bytes).map_err(|e| format!("Failed to write avatar file: {}", e))?;

        Ok(stored_name)
    }

    /// Delete avatar file for a persona.
    #[allow(dead_code)]
    pub fn delete_avatar_file(filename: &str) {
        let path = Self::avatars_dir().join(filename);
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
#[path = "personas_tests.rs"]
mod tests;
