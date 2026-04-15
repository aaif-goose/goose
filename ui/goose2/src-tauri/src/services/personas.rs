use crate::types::agents::{
    builtin_personas, Avatar, CreatePersonaRequest, Persona, UpdatePersonaRequest,
};
use log::warn;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct PersonaStore {
    personas: Mutex<Vec<Persona>>,
    store_path: PathBuf,
}

/// YAML frontmatter fields parsed from markdown persona files.
#[derive(serde::Deserialize)]
struct MarkdownFrontmatter {
    name: String,
    description: Option<String>,
}

impl PersonaStore {
    pub fn new() -> Self {
        let store_path = Self::store_path();
        let stored = Self::load_from_disk(&store_path);
        let markdown = Self::load_markdown_personas();
        let merged = Self::merge_all(stored, markdown);
        Self {
            personas: Mutex::new(merged),
            store_path,
        }
    }

    fn store_path() -> PathBuf {
        let base = dirs::home_dir().expect("home dir");
        base.join(".goose").join("personas.json")
    }

    /// Path to the avatars directory (~/.goose/avatars/).
    pub fn avatars_dir() -> PathBuf {
        dirs::home_dir()
            .expect("home dir")
            .join(".goose")
            .join("avatars")
    }

    fn load_from_disk(path: &PathBuf) -> Vec<Persona> {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// Merge builtins, JSON custom personas, and markdown personas.
    /// Priority: builtins first, then JSON custom, then markdown.
    /// Deduplication is by display_name (case-insensitive).
    fn merge_all(stored: Vec<Persona>, markdown: Vec<Persona>) -> Vec<Persona> {
        let builtins = builtin_personas();

        let mut result = builtins;
        let mut seen_names: HashSet<String> = result
            .iter()
            .map(|p| p.display_name.to_lowercase())
            .collect();
        let mut seen_ids: HashSet<String> = result.iter().map(|p| p.id.clone()).collect();

        // Add custom (non-builtin) personas from JSON
        for persona in stored {
            if !seen_ids.contains(&persona.id) {
                seen_names.insert(persona.display_name.to_lowercase());
                seen_ids.insert(persona.id.clone());
                result.push(persona);
            }
        }

        // Add markdown personas, skipping any whose name already exists
        for persona in markdown {
            if !seen_names.contains(&persona.display_name.to_lowercase())
                && !seen_ids.contains(&persona.id)
            {
                seen_names.insert(persona.display_name.to_lowercase());
                seen_ids.insert(persona.id.clone());
                result.push(persona);
            }
        }

        result
    }

    /// Directory containing markdown persona files.
    fn agents_dir() -> PathBuf {
        dirs::home_dir()
            .expect("home dir")
            .join(".goose")
            .join("agents")
    }

    /// Scan `~/.goose/agents/*.md` and parse each into a Persona.
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

        // Expect file to start with "---"
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return Err("Missing frontmatter delimiter".to_string());
        }

        // Find the closing "---"
        let after_first = &trimmed[3..];
        let end_idx = after_first
            .find("\n---")
            .ok_or_else(|| "Missing closing frontmatter delimiter".to_string())?;

        let yaml_str = &after_first[..end_idx];
        let body = after_first[end_idx + 4..].trim().to_string();

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
            avatar: None,
            system_prompt,
            provider: None,
            model: None,
            is_builtin: false,
            is_from_disk: true,
            created_at: mod_time.clone(),
            updated_at: mod_time,
        })
    }

    /// Re-scan markdown personas and update the in-memory list.
    /// Returns the full updated persona list.
    pub fn refresh_markdown(&self) -> Vec<Persona> {
        let stored = Self::load_from_disk(&self.store_path);
        let markdown = Self::load_markdown_personas();
        let merged = Self::merge_all(stored, markdown);

        let mut personas = self.personas.lock().unwrap();
        *personas = merged;
        personas.clone()
    }

    fn save_to_disk(&self, personas: &[Persona]) {
        if let Some(parent) = self.store_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Only persist custom personas (not builtins, not from markdown files)
        let custom: Vec<&Persona> = personas
            .iter()
            .filter(|p| !p.is_builtin && !p.is_from_disk)
            .collect();
        if let Ok(json) = serde_json::to_string_pretty(&custom) {
            let _ = std::fs::write(&self.store_path, json);
        }
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
        let persona = Persona {
            id: uuid::Uuid::new_v4().to_string(),
            display_name: req.display_name,
            avatar: req.avatar,
            system_prompt: req.system_prompt,
            provider: req.provider,
            model: req.model,
            is_builtin: false,
            is_from_disk: false,
            created_at: now.clone(),
            updated_at: now,
        };

        let mut personas = self.personas.lock().unwrap();
        personas.push(persona.clone());
        self.save_to_disk(&personas);
        Ok(persona)
    }

    pub fn update(&self, id: &str, req: UpdatePersonaRequest) -> Result<Persona, String> {
        let mut personas = self.personas.lock().unwrap();
        let persona = personas
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("Persona '{}' not found", id))?;

        if persona.is_builtin {
            return Err("Cannot update a built-in persona".to_string());
        }
        if persona.is_from_disk {
            return Err("Cannot update a markdown persona — edit the file directly".to_string());
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
        self.save_to_disk(&personas);
        Ok(updated)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let mut personas = self.personas.lock().unwrap();

        let persona = personas
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("Persona '{}' not found", id))?;

        if persona.is_builtin {
            return Err("Cannot delete a built-in persona".to_string());
        }
        if persona.is_from_disk {
            return Err("Cannot delete a markdown persona — delete the file directly".to_string());
        }

        // Clean up local avatar file if present
        if let Some(Avatar::Local(filename)) = &persona.avatar {
            let path = Self::avatars_dir().join(filename);
            let _ = std::fs::remove_file(path);
        }

        personas.retain(|p| p.id != id);
        self.save_to_disk(&personas);
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
