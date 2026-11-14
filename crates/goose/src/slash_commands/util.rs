pub fn normalize_command_name(name: &str) -> String {
    name.trim_start_matches('/').to_lowercase()
}
