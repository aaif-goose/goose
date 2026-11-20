use crate::config::paths::Paths;
use crate::conversation::message::ToolRequest;
use anyhow::Result;
use blake3::Hasher;
use chrono::Utc;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolPermissionRecord {
    tool_name: String,
    allowed: bool,
    context_hash: String, // Hash of the tool's arguments/context to differentiate similar calls
    #[serde(skip_serializing_if = "Option::is_none")] // Don't serialize if None
    readable_context: Option<String>,
    timestamp: i64,
    expiry: Option<i64>, // Optional expiry timestamp
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolPermissionStore {
    permissions: HashMap<String, Vec<ToolPermissionRecord>>,
    version: u32, // For future schema migrations
    #[serde(skip)] // Don't serialize this field
    permissions_dir: PathBuf,
}

impl Default for ToolPermissionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolPermissionStore {
    pub fn new() -> Self {
        Self::new_in(Paths::config_dir().join("permissions"))
    }

    fn new_in(permissions_dir: PathBuf) -> Self {
        Self {
            permissions: HashMap::new(),
            version: 1,
            permissions_dir,
        }
    }

    pub fn load() -> Result<Self> {
        let mut store = Self::new().read_current()?;
        if store.prune_expired() {
            store.cleanup_expired()?;
        }
        Ok(store)
    }

    fn read_current(&self) -> Result<Self> {
        let path = self.permissions_dir.join("tool_permissions.json");
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(Self::new_in(self.permissions_dir.clone()));
            }
            Err(error) => return Err(error.into()),
        };
        let mut store: Self = serde_json::from_reader(file)?;
        store.permissions_dir = self.permissions_dir.clone();
        Ok(store)
    }

    fn lock_for_mutation(&self) -> Result<File> {
        std::fs::create_dir_all(&self.permissions_dir)?;
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.permissions_dir.join("tool_permissions.json.lock"))?;
        lock_file.lock_exclusive()?;
        Ok(lock_file)
    }

    fn update(&mut self, mutate: impl FnOnce(&mut Self) -> bool) -> Result<()> {
        let _lock = self.lock_for_mutation()?;
        let mut current = self.read_current()?;
        if mutate(&mut current) {
            current.save()?;
        }
        *self = current;
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let path = self.permissions_dir.join("tool_permissions.json");
        let content = serde_json::to_string_pretty(self)?;
        let mut temp = tempfile::NamedTempFile::new_in(&self.permissions_dir)?;
        temp.write_all(content.as_bytes())?;
        temp.as_file().sync_all()?;
        temp.persist(&path).map_err(|e| e.error)?;

        Ok(())
    }

    pub fn check_permission(&self, tool_request: &ToolRequest) -> Option<bool> {
        let context_hash = self.hash_tool_context(tool_request);
        let tool_call = tool_request.tool_call.as_ref().unwrap();
        let key = format!("{}:{}", tool_call.name, context_hash);

        self.permissions.get(&key).and_then(|records| {
            records
                .iter()
                .rfind(|record| record.expiry.is_none_or(|exp| exp > Utc::now().timestamp()))
                .map(|record| record.allowed)
        })
    }

    pub fn record_permission(
        &mut self,
        tool_request: &ToolRequest,
        allowed: bool,
        expiry_duration: Option<Duration>,
    ) -> Result<()> {
        let context_hash = self.hash_tool_context(tool_request);
        let tool_call = tool_request.tool_call.as_ref().unwrap();
        let key = format!("{}:{}", tool_call.name, context_hash);
        let timestamp = Utc::now().timestamp();
        let record = ToolPermissionRecord {
            tool_name: tool_call.name.to_string(),
            allowed,
            context_hash,
            readable_context: Some(tool_request.to_readable_string()),
            timestamp,
            expiry: expiry_duration.map(|d| timestamp + d.as_secs() as i64),
        };

        self.update(|store| {
            store.permissions.entry(key).or_default().push(record);
            true
        })
    }

    fn hash_tool_context(&self, tool_request: &ToolRequest) -> String {
        // Create a hash of the tool's arguments to differentiate similar calls
        // This helps identify when the same tool is being used in a different context
        let mut hasher = Hasher::new();
        hasher.update(
            serde_json::to_string(&tool_request.tool_call.as_ref().unwrap().arguments)
                .unwrap_or_default()
                .as_bytes(),
        );
        hasher.finalize().to_hex().to_string()
    }

    pub fn cleanup_expired(&mut self) -> Result<()> {
        self.update(Self::prune_expired)
    }

    fn prune_expired(&mut self) -> bool {
        let now = Utc::now().timestamp();
        let mut changed = false;

        self.permissions.retain(|_, records| {
            records.retain(|record| record.expiry.is_none_or(|exp| exp > now));
            changed = changed || records.is_empty();
            !records.is_empty()
        });

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::CallToolRequestParams;

    fn request(name: &str) -> ToolRequest {
        ToolRequest {
            id: name.to_owned(),
            tool_call: Ok(CallToolRequestParams::new(name.to_owned())),
            metadata: None,
            tool_meta: None,
        }
    }

    #[test]
    fn records_from_independently_loaded_stores_are_preserved() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut first = ToolPermissionStore::new_in(dir.path().to_owned()).read_current()?;
        let mut second = first.read_current()?;
        let allowed = request("allowed_tool");
        let denied = request("denied_tool");

        first.record_permission(&allowed, true, None)?;
        second.record_permission(&denied, false, None)?;

        let stored = first.read_current()?;
        assert_eq!(stored.check_permission(&allowed), Some(true));
        assert_eq!(stored.check_permission(&denied), Some(false));
        Ok(())
    }

    #[test]
    fn cleanup_preserves_newer_records() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut writer = ToolPermissionStore::new_in(dir.path().to_owned());
        let expired = request("expired_tool");
        let current = request("current_tool");

        writer.record_permission(&expired, true, Some(Duration::ZERO))?;
        let mut cleaner = writer.read_current()?;
        writer.record_permission(&current, false, None)?;
        cleaner.cleanup_expired()?;

        let stored = writer.read_current()?;
        assert_eq!(stored.permissions.len(), 1);
        assert_eq!(stored.check_permission(&current), Some(false));
        Ok(())
    }
}
