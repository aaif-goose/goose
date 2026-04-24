use crate::config::search_path::SearchPaths;
use crate::providers::inventory::InventoryIdentityInput;
use anyhow::Result;
use std::path::PathBuf;

pub fn acp_adapter_installed(command: &str) -> bool {
    resolve_acp_command(command).is_ok()
}

pub fn acp_inventory_identity(provider_id: &str, command: &str) -> Result<InventoryIdentityInput> {
    let resolved_command = resolve_acp_command(command)?;
    Ok(InventoryIdentityInput::new(provider_id, provider_id)
        .with_public("command", resolved_command.display().to_string()))
}

fn resolve_acp_command(command: &str) -> Result<PathBuf> {
    SearchPaths::builder().with_npm().resolve(command)
}
