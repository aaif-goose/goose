use std::collections::HashMap;

use anyhow::{bail, Result};
use rmcp::model::Tool;

/// Duplicate public names are rejected unless the host explicitly opts into legacy behavior.
#[derive(Clone, Copy, Default)]
pub enum CollisionPolicy {
    #[default]
    Reject,
    KeepFirst,
}

pub struct CatalogEntry<T> {
    pub tool: Tool,
    pub owner: T,
    pub server_name: String,
}

/// Immutable public-to-server routing. Ownership is retained with every advertised schema.
pub struct Catalog<T> {
    pub entries: Vec<CatalogEntry<T>>,
    by_name: HashMap<String, usize>,
}

impl<T> Catalog<T> {
    pub fn new(
        entries: impl IntoIterator<Item = CatalogEntry<T>>,
        policy: CollisionPolicy,
    ) -> Result<Self> {
        let mut catalog = Self {
            entries: Vec::new(),
            by_name: HashMap::new(),
        };
        for mut entry in entries {
            let name = provider_safe_tool_name(&entry.tool.name);
            if catalog.by_name.contains_key(&name) {
                match policy {
                    CollisionPolicy::Reject => bail!("duplicate MCP tool name: {name}"),
                    CollisionPolicy::KeepFirst => continue,
                }
            }
            entry.tool.name = name.clone().into();
            catalog.by_name.insert(name, catalog.entries.len());
            catalog.entries.push(entry);
        }
        Ok(catalog)
    }

    pub fn get(&self, name: &str) -> Option<&CatalogEntry<T>> {
        self.by_name.get(name).map(|&index| &self.entries[index])
    }
}

/// Preserve already-safe names; hash rewritten names to avoid lossy-conversion collisions.
pub fn provider_safe_tool_name(name: &str) -> String {
    let is_safe = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    if !name.is_empty() && name.len() <= 64 && name.chars().all(is_safe) {
        return name.to_string();
    }
    let sanitized: String = name
        .chars()
        .map(|c| if is_safe(c) { c } else { '_' })
        .take(55)
        .collect();
    let hash = blake3::hash(name.as_bytes()).to_hex();
    format!("{sanitized}_{:.8}", hash.as_str())
}
