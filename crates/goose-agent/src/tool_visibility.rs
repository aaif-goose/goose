use rmcp::model::Tool;

/// Check whether a tool should be callable by an app based on MCP Apps visibility metadata.
///
/// Per the MCP Apps spec (2026-01-26), if `_meta.ui.visibility` is present and does not
/// include `"app"`, the tool is model-only and must not be callable by app UIs.
/// If the field is absent, the tool defaults to visible to both model and app.
pub fn is_tool_visible_to_app(tool: &Tool) -> bool {
    let Some(meta) = &tool.meta else {
        return true;
    };
    let Some(ui) = meta.0.get("ui") else {
        return true;
    };
    let Some(visibility) = ui.get("visibility") else {
        return true;
    };
    let Some(arr) = visibility.as_array() else {
        return false;
    };
    arr.iter().any(|v| v.as_str() == Some("app"))
}

/// Check whether a tool should be visible to the model based on MCP Apps visibility metadata.
///
/// Per the MCP Apps spec (2026-01-26), tools may declare `_meta.ui.visibility` as an array
/// of `"model"` and/or `"app"`. If the field is absent, the tool defaults to visible to both.
/// If present and does not include `"model"`, the tool is app-only and must not be sent to the LLM.
pub fn is_tool_visible_to_model(tool: &Tool) -> bool {
    let Some(meta) = &tool.meta else {
        return true;
    };
    let Some(ui) = meta.0.get("ui") else {
        return true;
    };
    let Some(visibility) = ui.get("visibility") else {
        return true;
    };
    let Some(arr) = visibility.as_array() else {
        return false;
    };
    arr.iter().any(|v| v.as_str() == Some("model"))
}
