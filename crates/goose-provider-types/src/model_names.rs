#[cfg(not(feature = "std"))]
use crate::prelude::*;

pub fn extract_reasoning_effort(model_name: &str) -> (String, Option<String>) {
    if let Some((base, effort)) = model_name.rsplit_once('-') {
        if !base.is_empty()
            && ["none", "low", "medium", "high", "xhigh"]
                .iter()
                .any(|e| effort.eq_ignore_ascii_case(e))
        {
            return (base.to_string(), Some(effort.to_ascii_lowercase()));
        }
    }
    (model_name.to_string(), None)
}

pub fn is_openai_responses_model(model_name: &str) -> bool {
    let lower = model_name.to_ascii_lowercase();
    core::iter::once(0)
        .chain(lower.match_indices(['-', '/']).map(|(i, _)| i + 1))
        .any(|i| {
            let tail = lower.get(i..).unwrap();
            if let Some(version) = tail.strip_prefix("gpt-") {
                return version
                    .strip_prefix(['5', '6'])
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with(['-', '.']));
            }
            tail.strip_prefix('o').is_some_and(|version| {
                let count = version.bytes().take_while(u8::is_ascii_digit).count();
                count > 0 && (count == version.len() || version.as_bytes()[count] == b'-')
            })
        })
}

pub fn supports_xai_reasoning_effort(model_name: &str) -> bool {
    let model_name = model_name.to_ascii_lowercase();

    model_name.starts_with("grok-4.5")
        || model_name.starts_with("grok-4.3")
        || model_name.starts_with("grok-3-mini")
}

pub fn is_xai_reasoning_model(model_name: &str) -> bool {
    let model_name = model_name.to_ascii_lowercase();

    if model_name.contains("non-reasoning") || model_name.contains("non_reasoning") {
        return false;
    }

    supports_xai_reasoning_effort(&model_name)
        || model_name.starts_with("grok-4.20")
        || model_name.starts_with("grok-4-0709")
        || model_name.starts_with("grok-4-fast-reasoning")
        || model_name.starts_with("grok-4-1-fast-reasoning")
}
