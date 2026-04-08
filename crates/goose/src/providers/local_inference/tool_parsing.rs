use rmcp::model::Tool;
use serde_json::{json, Value};

pub(super) fn compact_tools_json(tools: &[Tool]) -> Option<String> {
    let compact: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description.as_ref().map(|d| d.as_ref()).unwrap_or(""),
                }
            })
        })
        .collect();
    serde_json::to_string(&compact).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_tools_json_produces_minimal_output() {
        use rmcp::model::Tool;
        use rmcp::object;

        let tools = vec![Tool::new(
            "developer__shell".to_string(),
            "Run shell commands".to_string(),
            object!({"type": "object", "properties": {"command": {"type": "string"}}}),
        )];
        let result = compact_tools_json(&tools);
        assert!(result.is_some());
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed.len(), 1);
        let func = &parsed[0]["function"];
        assert_eq!(func["name"], "developer__shell");
        assert_eq!(func["description"], "Run shell commands");
        // Should not contain full parameter schemas
        assert!(func.get("parameters").is_none());
    }

    #[test]
    fn test_compact_tools_json_empty() {
        let result = compact_tools_json(&[]);
        assert!(result.is_some());
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(parsed.is_empty());
    }
}
