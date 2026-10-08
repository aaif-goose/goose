//! Normalisation of MCP tool `input_schema`s for providers whose tool-schema
//! subset rejects top-level combinators.
//!
//! The Anthropic Messages API and Bedrock reject `oneOf`, `allOf` and `anyOf`
//! at the top level of a tool `input_schema` (`input_schema does not support
//! oneOf, allOf, or anyOf at the top level`). A single such tool breaks every
//! request in the session because the tool list is sent on every call. The
//! OpenAI path already rewrites these; this module provides the equivalent
//! normalisation so a schema that is valid JSON Schema but outside a
//! provider's tool-schema subset does not take the whole tool list down.

use serde_json::{Map, Value};

/// Normalise a tool `input_schema` in place so that no top-level combinator
/// (`oneOf` / `anyOf` / `allOf` / `prefixItems`) remains. Returns `true` if the
/// schema changed.
///
/// Nested combinators are left untouched: Anthropic and Bedrock accept them in
/// sub-schema positions, and preserving them keeps the original semantics.
pub fn normalize_tool_input_schema(schema: &mut Value) -> bool {
    let Some(obj) = schema.as_object_mut() else {
        return false;
    };

    let mut changed = false;

    // 1) `oneOf` at the top level is rejected; `anyOf` is the more widely
    //    supported spelling for tool-argument schemas (same as the OpenAI path).
    if !obj.contains_key("anyOf") {
        if let Some(one_of) = obj.remove("oneOf") {
            obj.insert("anyOf".to_string(), one_of);
            changed = true;
        }
    }

    // 2) `anyOf` at the top level: unwrap when the union is provably
    //    equivalent to a single object, otherwise merge the object variants'
    //    `properties` and record the unrepresentable constraints.
    if let Some(any_of) = obj.get("anyOf").cloned() {
        if let Some(variants) = any_of.as_array() {
            if let Some(merged) = merge_top_level_union(obj, variants, &mut changed) {
                *obj = merged;
                obj.remove("anyOf");
                changed = true;
            }
        }
    }

    // 3) `allOf` at the top level: merge every member's object fields.
    if let Some(all_of) = obj.get("allOf").cloned() {
        if let Some(members) = all_of.as_array() {
            if !members.is_empty() && merge_top_level_all_of(obj, members, &mut changed) {
                obj.remove("allOf");
                changed = true;
            }
        }
    }

    // 4) `prefixItems` at the top level: a single member is equivalent to that
    //    schema; multiple members become a plain `items` union.
    if let Some(prefix) = obj.remove("prefixItems") {
        if let Some(members) = prefix.as_array() {
            if members.len() == 1 {
                obj.insert("items".to_string(), members[0].clone());
            } else {
                obj.insert(
                    "items".to_string(),
                    Value::Object(Map::from_iter([("anyOf".to_string(), prefix)])),
                );
            }
        } else {
            obj.insert("items".to_string(), prefix);
        }
        changed = true;
    }

    changed
}

/// Try to express a top-level `anyOf`/`oneOf` union as a single object.
///
/// Returns `Some(merged)` when the union can be collapsed into the current
/// object without lying: nullable unions (`[T, {"type": "null"}]`) unwrap to
/// `T`; single-variant unions unwrap to that variant; a union of object
/// variants merges their `properties` (and `required`) and drops only the
/// unrepresentable constraints, recording them in the description.
///
/// When none of these apply (e.g. a union of scalar alternatives), the union is
/// left in place by returning `None` — callers may still choose to drop it.
fn merge_top_level_union(
    obj: &mut Map<String, Value>,
    variants: &[Value],
    changed: &mut bool,
) -> Option<Map<String, Value>> {
    if variants.is_empty() {
        return None;
    }

    // Nullable union: [T, {"type": "null"}]
    if variants.len() == 2 {
        let is_null = |v: &Value| v.get("type").and_then(Value::as_str) == Some("null");
        let non_null = if is_null(&variants[0]) {
            Some(&variants[1])
        } else if is_null(&variants[1]) {
            Some(&variants[0])
        } else {
            None
        };
        if let Some(replacement) = non_null {
            let mut merged = obj.clone();
            if let Some(replacement_obj) = replacement.as_object() {
                for (k, v) in replacement_obj {
                    merged.entry(k.clone()).or_insert(v.clone());
                }
            }
            *changed = true;
            return Some(merged);
        }
    }

    // Single-variant union: [X]
    if variants.len() == 1 {
        let mut merged = obj.clone();
        if let Some(variant_obj) = variants[0].as_object() {
            for (k, v) in variant_obj {
                merged.entry(k.clone()).or_insert(v.clone());
            }
        } else {
            merged.insert("type".to_string(), variants[0].get("type").cloned().unwrap_or(Value::String("object".to_string())));
        }
        *changed = true;
        return Some(merged);
    }

    // Union of object variants: merge `properties` and `required`; fold the
    // rest into the description so no constraint is silently lost. Only do
    // this when every variant is a genuine object schema (has `properties`);
    // scalar alternatives (e.g. `string` vs `integer`) are left for the caller
    // to drop, since merging them would misrepresent the union.
    if variants.iter().all(|v| v.get("properties").is_some_and(Value::is_object)) {
        let mut merged = obj.clone();
        let mut properties = merged
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut required: Vec<String> = merged
            .get("required")
            .and_then(Value::as_array)
            .and_then(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
                    .into()
            })
            .unwrap_or_default();
        let mut extra_constraints: Vec<String> = Vec::new();

        for variant in variants {
            if let Some(variant_obj) = variant.as_object() {
                if let Some(props) = variant_obj.get("properties").and_then(Value::as_object) {
                    for (k, v) in props {
                        properties.entry(k.clone()).or_insert_with(|| v.clone());
                    }
                }
                if let Some(req) = variant_obj.get("required").and_then(Value::as_array) {
                    for k in req.iter().filter_map(Value::as_str) {
                        if !required.contains(&k.to_string()) {
                            required.push(k.to_string());
                        }
                    }
                }
                for (k, v) in variant_obj {
                    if !matches!(k.as_str(), "properties" | "required" | "type") {
                        extra_constraints.push(format!("{k}: {v}"));
                    }
                }
            }
        }

        if !properties.is_empty() {
            merged.insert("properties".to_string(), Value::Object(properties));
        }
        if !required.is_empty() {
            merged.insert("required".to_string(), Value::Array(
                required.into_iter().map(Value::String).collect(),
            ));
        }
        if !extra_constraints.is_empty() {
            let existing = merged
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_default();
            let note = format!(" (merged union variants: {})", extra_constraints.join(", "));
            merged.insert(
                "description".to_string(),
                Value::String(format!("{existing}{note}")),
            );
        }
        merged.insert("type".to_string(), Value::String("object".to_string()));
        *changed = true;
        return Some(merged);
    }

    None
}

/// Merge the members of a top-level `allOf` into the current object.
///
/// `properties` maps are merged key-wise, `required` arrays are unioned, and
/// scalar keys (`type`, `description`, `title`, ...) take the first non-null
/// value. Returns `false` (no change) only when merging would be a lie, which
/// for `allOf` (logical conjunction) it never is — every member must hold, and
/// merging their object constraints preserves that.
fn merge_top_level_all_of(
    obj: &mut Map<String, Value>,
    members: &[Value],
    changed: &mut bool,
) -> bool {
    let mut properties = obj
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut required: Vec<String> = obj
        .get("required")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
                .into()
        })
        .unwrap_or_default();
    let mut merged_any = false;

    for member in members {
        let Some(member_obj) = member.as_object() else {
            continue;
        };
        if let Some(props) = member_obj.get("properties").and_then(Value::as_object) {
            for (k, v) in props {
                if !properties.contains_key(k) {
                    properties.insert(k.clone(), v.clone());
                    merged_any = true;
                }
            }
        }
        if let Some(req) = member_obj.get("required").and_then(Value::as_array) {
            for k in req.iter().filter_map(Value::as_str) {
                if !required.contains(&k.to_string()) {
                    required.push(k.to_string());
                    merged_any = true;
                }
            }
        }
        for (k, v) in member_obj {
            if matches!(k.as_str(), "properties" | "required") {
                continue;
            }
            if !obj.contains_key(k) {
                obj.insert(k.clone(), v.clone());
                merged_any = true;
            }
        }
    }

    if !properties.is_empty() {
        obj.insert("properties".to_string(), Value::Object(properties));
    }
    if !required.is_empty() {
        obj.insert(
            "required".to_string(),
            Value::Array(required.into_iter().map(Value::String).collect()),
        );
    }
    obj.entry("type".to_string())
        .or_insert_with(|| Value::String("object".to_string()));
    if merged_any {
        *changed = true;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn normalize(mut v: Value) -> Value {
        normalize_tool_input_schema(&mut v);
        v
    }

    #[test]
    fn top_level_all_of_is_merged() {
        let schema = json!({
            "type": "object",
            "allOf": [
                {"properties": {"book_id": {"type": "integer"}}, "required": ["book_id"]},
                {"properties": {"html": {"type": "string"}}}
            ]
        });
        let out = normalize(schema);
        assert!(out.get("allOf").is_none());
        let props = out["properties"].as_object().unwrap();
        assert!(props.contains_key("book_id"));
        assert!(props.contains_key("html"));
        assert_eq!(out["type"], json!("object"));
    }

    #[test]
    fn top_level_one_of_becomes_any_of_then_unwrapped_single() {
        let schema = json!({"type": "object", "oneOf": [{"properties": {"a": {"type": "string"}}}]});
        let out = normalize(schema);
        assert!(out.get("oneOf").is_none());
        assert!(out.get("anyOf").is_none());
        assert!(out["properties"].as_object().unwrap().contains_key("a"));
    }

    #[test]
    fn nullable_union_unwraps() {
        let schema = json!({"anyOf": [{"type": "string"}, {"type": "null"}], "description": "d"});
        let out = normalize(schema);
        assert!(out.get("anyOf").is_none());
        assert_eq!(out["type"], json!("string"));
        assert_eq!(out["description"], json!("d"));
    }

    #[test]
    fn multi_object_union_merges_properties() {
        let schema = json!({
            "anyOf": [
                {"properties": {"x": {"type": "integer"}}, "required": ["x"]},
                {"properties": {"y": {"type": "string"}}}
            ]
        });
        let out = normalize(schema);
        assert!(out.get("anyOf").is_none());
        let props = out["properties"].as_object().unwrap();
        assert!(props.contains_key("x") && props.contains_key("y"));
        assert_eq!(out["type"], json!("object"));
    }

    #[test]
    fn prefix_items_single_becomes_items() {
        let schema = json!({"prefixItems": [{"type": "string"}]});
        let out = normalize(schema);
        assert!(out.get("prefixItems").is_none());
        assert_eq!(out["items"], json!({"type": "string"}));
    }

    #[test]
    fn scalar_union_is_dropped_by_caller() {
        // A scalar alternative union cannot be expressed as a single object;
        // normalize leaves it untouched so the caller can decide.
        let schema = json!({"anyOf": [{"type": "string"}, {"type": "integer"}]});
        let mut v = schema.clone();
        let changed = normalize_tool_input_schema(&mut v);
        assert!(!changed);
        assert!(v.get("anyOf").is_some());
    }
}
