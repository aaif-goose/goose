use std::collections::BTreeMap;

use anyhow::Result;
use rmcp::model::{Role, Tool};

pub use goose_sdk_types::custom_requests::{
    ContextCategory, ContextPart, ContextReportModel, ContextReportResponse, ContextSegment,
};

use crate::agents::extension_manager::get_tool_owner;
use crate::agents::state_machine::enrich_unclaimed_tool_errors;
use crate::agents::Agent;
use crate::conversation::message::{Message, MessageContent};
use crate::conversation::{
    effective_role, fix_conversation, merge_consecutive_messages_for_request, Conversation,
};
use crate::providers::toolshim::{
    convert_tool_messages_to_text, format_tool_info, modify_system_prompt_for_tool_json,
    toolshim_system_prompt_appendix,
};
use crate::token_counter::{TokenCounter, TOKENS_PER_MESSAGE};

const MAX_PREVIEW_CHARS: usize = 2_000;

fn preview(text: &str) -> String {
    let count = text.chars().count();
    if count <= MAX_PREVIEW_CHARS {
        return text.to_string();
    }
    let kept: String = text.chars().take(MAX_PREVIEW_CHARS).collect();
    format!("{kept}… (+{} more chars)", count - MAX_PREVIEW_CHARS)
}

pub(super) const NEXT_PROMPT_STAND_IN: &str = "[next user prompt]";

/// The persisted conversation as the next request will carry it. Both loops
/// repair the agent-visible messages with `fix_conversation` after appending
/// the new user prompt, so a stand-in takes the prompt's place here; without it
/// the repair would read the last reply as a dangling trailing assistant
/// message and drop it.
fn next_request_messages(persisted: &Conversation) -> Vec<Message> {
    let mut messages = persisted.agent_visible_messages();
    messages.push(Message::user().with_text(NEXT_PROMPT_STAND_IN));
    let (fixed, _) = fix_conversation(Conversation::new_unvalidated(messages));
    let mut messages: Vec<Message> = fixed.into_iter().collect();
    if let Some(last) = messages.last_mut() {
        if matches!(
            last.content.last(),
            Some(MessageContent::Text(text)) if text.text == NEXT_PROMPT_STAND_IN
        ) {
            last.content.pop();
        }
        if last.content.is_empty() {
            messages.pop();
        }
    }
    messages
}

fn text_segment(
    category: ContextCategory,
    label: impl Into<String>,
    source: Option<String>,
    text: &str,
    token_counter: &TokenCounter,
) -> ContextSegment {
    ContextSegment {
        category,
        label: label.into(),
        source,
        token_count: token_counter.count_tokens(text) as u64,
        content_preview: Some(preview(text)),
        parts: Vec::new(),
    }
}

impl Agent {
    pub async fn build_context_report(
        &self,
        session_id: &str,
        token_counter: &TokenCounter,
        unrolled_agent_loop: bool,
    ) -> Result<ContextReportResponse> {
        let session = self
            .config
            .session_manager
            .get_session(session_id, true)
            .await?;
        let (prepared, turn_context) = if unrolled_agent_loop {
            self.prepare_unrolled_prompt(&session).await?
        } else {
            (
                self.prepare_prompt(session_id, &session.working_dir)
                    .await?,
                self.upcoming_legacy_turn_context(&session).await,
            )
        };
        // The unrolled loop's ProjectOperation already contributes these as a
        // prompt extra; the legacy loop appends them last, after the toolshim
        // appendix.
        let project_instructions = if unrolled_agent_loop {
            None
        } else {
            self.load_project_instructions(&session).await
        };
        let toolshim = prepared.model_config.toolshim;

        let mut system_prompt = prepared.prompt.join();
        if toolshim {
            system_prompt = modify_system_prompt_for_tool_json(&system_prompt, &prepared.tools);
        }
        if let Some(project_instructions) = &project_instructions {
            system_prompt = format!("{system_prompt}\n\n{project_instructions}");
        }

        let persisted = session
            .conversation
            .clone()
            .unwrap_or_else(Conversation::empty);
        let mut messages = next_request_messages(&persisted);
        // The prompt that will follow is unknown, but the turn-context event
        // appended after it is not.
        messages.extend(turn_context);
        // The unrolled loop's inference provider appends the available tool
        // list to unclaimed-tool errors on every request, not just the first.
        if unrolled_agent_loop {
            messages = enrich_unclaimed_tool_errors(&messages, &prepared.tools);
        }
        let messages = Conversation::new_unvalidated(messages);
        let request_messages = merge_consecutive_messages_for_request(messages.messages().clone());
        let (request_tools, request_messages, counted_messages) = if toolshim {
            (
                Vec::new(),
                convert_tool_messages_to_text(&request_messages),
                convert_tool_messages_to_text(messages.messages()),
            )
        } else {
            (
                prepared.tools.clone(),
                Conversation::new_unvalidated(request_messages),
                messages.clone(),
            )
        };
        let request_tokens = token_counter.count_chat_tokens(
            &system_prompt,
            request_messages.messages(),
            &request_tools,
        ) as u64;

        let mut segments = Vec::new();
        segments.push(text_segment(
            ContextCategory::SystemPrompt,
            "Base system prompt",
            Some(if prepared.prompt.base_is_override {
                "override".to_string()
            } else {
                "prompts/system.md".to_string()
            }),
            &prepared.prompt.base,
            token_counter,
        ));
        for (name, instructions) in &prepared.prompt.extension_instructions {
            segments.push(text_segment(
                ContextCategory::ExtensionInstructions,
                name,
                None,
                instructions,
                token_counter,
            ));
        }
        for (key, value) in &prepared.prompt.extras {
            let (category, label, source) = extra_descriptor(key);
            segments.push(text_segment(category, label, source, value, token_counter));
        }
        if let Some(project_instructions) = &project_instructions {
            segments.push(text_segment(
                ContextCategory::AdditionalInstructions,
                "Project instructions",
                session.project_id.clone(),
                project_instructions,
                token_counter,
            ));
        }
        if toolshim {
            segments.push(toolshim_tool_segment(&prepared.tools, token_counter));
        } else {
            segments.extend(tool_segments(&prepared.tools, token_counter));
        }
        segments.extend(message_segments(
            messages.messages(),
            counted_messages.messages(),
            token_counter,
        ));

        // Tokenizing the rows one by one is not additive with tokenizing the
        // joined request: a positive residual is the overhead row, a negative
        // one comes off the prompt rows, so the rows always sum to the request.
        let attributed: u64 = segments.iter().map(|segment| segment.token_count).sum();
        if request_tokens > attributed {
            segments.push(ContextSegment {
                category: ContextCategory::SystemPrompt,
                label: "Prompt overhead".to_string(),
                source: None,
                token_count: request_tokens - attributed,
                content_preview: None,
                parts: Vec::new(),
            });
        } else {
            let mut excess = attributed - request_tokens;
            for segment in &mut segments {
                if excess == 0 {
                    break;
                }
                let taken = segment.token_count.min(excess);
                segment.token_count -= taken;
                excess -= taken;
            }
        }

        let context_limit = match self.provider().await {
            Ok(provider) => crate::context_limit::get_context_limit(
                provider.as_ref(),
                &prepared.model_config.model_name,
            )
            .await
            .unwrap_or_else(|_| prepared.model_config.context_limit()),
            Err(_) => prepared.model_config.context_limit(),
        };

        Ok(ContextReportResponse {
            model: ContextReportModel {
                provider: session.provider_name.clone(),
                model_name: prepared.model_config.model_name.clone(),
                context_limit: context_limit as u64,
            },
            total_tokens: request_tokens,
            segments,
        })
    }
}

/// The unrolled loop contributes the extension block as prompt extras: a header
/// followed by one extra per extension, so those report as extension
/// instructions the way the legacy loop's own per-extension rows do.
fn extra_descriptor(key: &str) -> (ContextCategory, String, Option<String>) {
    if key == "hints" {
        (
            ContextCategory::AdditionalInstructions,
            "Hint files".to_string(),
            None,
        )
    } else if let Some(dir) = key.strip_prefix("subdir_hints:") {
        (
            ContextCategory::AdditionalInstructions,
            "Subdirectory hints".to_string(),
            Some(dir.to_string()),
        )
    } else if key == "extensions" {
        (
            ContextCategory::ExtensionInstructions,
            "Extensions".to_string(),
            None,
        )
    } else if let Some(name) = key.strip_prefix("extension:") {
        (
            ContextCategory::ExtensionInstructions,
            name.to_string(),
            None,
        )
    } else {
        (
            ContextCategory::AdditionalInstructions,
            key.to_string(),
            None,
        )
    }
}

fn tool_content(tool: &Tool) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "name": tool.name,
        "description": tool.description,
        "inputSchema": tool.input_schema,
    }))
    .unwrap_or_default()
}

/// Tools are counted as marginal additions to a growing prefix so the parts
/// sum to exactly what `count_tokens_for_tools` charges the request.
fn tool_segments(tools: &[Tool], token_counter: &TokenCounter) -> Vec<ContextSegment> {
    let mut groups: BTreeMap<String, Vec<&Tool>> = BTreeMap::new();
    for tool in tools {
        let owner = get_tool_owner(tool).unwrap_or_else(|| "ungrouped".to_string());
        groups.entry(owner).or_default().push(tool);
    }

    let mut cumulative: Vec<Tool> = Vec::with_capacity(tools.len());
    let mut cumulative_tokens = 0usize;
    let mut segments = Vec::with_capacity(groups.len());
    for (owner, group) in groups {
        let group_start = cumulative_tokens;
        let mut parts = Vec::with_capacity(group.len());
        for tool in group {
            let before = cumulative_tokens;
            cumulative.push(tool.clone());
            cumulative_tokens = token_counter.count_tokens_for_tools(&cumulative);
            parts.push(ContextPart {
                label: tool.name.to_string(),
                source: None,
                token_count: (cumulative_tokens - before) as u64,
                content_preview: Some(preview(&tool_content(tool))),
            });
        }
        segments.push(ContextSegment {
            category: ContextCategory::ToolDefinitions,
            label: owner,
            source: Some(format!("{} tools", parts.len())),
            token_count: (cumulative_tokens - group_start) as u64,
            content_preview: None,
            parts,
        });
    }
    segments
}

fn toolshim_tool_segment(tools: &[Tool], token_counter: &TokenCounter) -> ContextSegment {
    let mut cumulative: Vec<Tool> = Vec::with_capacity(tools.len());
    let mut appendix = toolshim_system_prompt_appendix(&cumulative);
    let mut cumulative_tokens = token_counter.count_tokens(&appendix);

    let mut parts = Vec::with_capacity(tools.len() + 1);
    parts.push(ContextPart {
        label: "Tool calling instructions".to_string(),
        source: None,
        token_count: cumulative_tokens as u64,
        content_preview: Some(preview(&appendix)),
    });
    for tool in tools {
        cumulative.push(tool.clone());
        appendix = toolshim_system_prompt_appendix(&cumulative);
        let before = cumulative_tokens;
        cumulative_tokens = token_counter.count_tokens(&appendix);
        parts.push(ContextPart {
            label: tool.name.to_string(),
            source: None,
            token_count: (cumulative_tokens - before) as u64,
            content_preview: Some(preview(&format_tool_info(std::slice::from_ref(tool)))),
        });
    }

    ContextSegment {
        category: ContextCategory::ToolDefinitions,
        label: "Toolshim tool instructions".to_string(),
        source: Some(format!("{} tools", tools.len())),
        token_count: cumulative_tokens as u64,
        content_preview: None,
        parts,
    }
}

fn message_text(message: &Message) -> String {
    message
        .content
        .iter()
        .filter_map(|content| {
            if let Some(text) = content.as_text() {
                Some(text.to_string())
            } else if let Some(request) = content.as_tool_request() {
                request.tool_call.as_ref().ok().map(|call| {
                    format!(
                        "{}({})",
                        call.name,
                        serde_json::to_string(&call.arguments).unwrap_or_default()
                    )
                })
            } else {
                content.as_tool_response_text()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum MessageKind {
    TurnContext,
    CompactionSummary,
    User,
    Assistant,
    ToolCalls,
    ToolResults,
}

impl MessageKind {
    fn classify(message: &Message) -> Self {
        if message.is_turn_context() {
            MessageKind::TurnContext
        } else if crate::context_mgmt::is_compaction_summary(message) {
            MessageKind::CompactionSummary
        } else if message
            .content
            .iter()
            .any(|c| c.as_tool_response().is_some())
        {
            MessageKind::ToolResults
        } else if message
            .content
            .iter()
            .any(|c| c.as_tool_request().is_some())
        {
            MessageKind::ToolCalls
        } else if message.role == Role::Assistant {
            MessageKind::Assistant
        } else {
            MessageKind::User
        }
    }

    fn category(self) -> ContextCategory {
        match self {
            MessageKind::TurnContext => ContextCategory::TurnContext,
            MessageKind::CompactionSummary => ContextCategory::CompactionSummary,
            _ => ContextCategory::Messages,
        }
    }

    fn label(self) -> &'static str {
        match self {
            MessageKind::TurnContext => "Turn context",
            MessageKind::CompactionSummary => "Conversation summary",
            MessageKind::User => "User messages",
            MessageKind::Assistant => "Assistant messages",
            MessageKind::ToolCalls => "Tool calls",
            MessageKind::ToolResults => "Tool results",
        }
    }
}

/// Splits on `##` headings outside fenced blocks, since code samples can
/// contain heading lines.
fn summary_sections(text: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut open_fence: Option<usize> = None;

    for line in text.lines() {
        let backticks = line.trim_start().chars().take_while(|c| *c == '`').count();
        match open_fence {
            Some(len) if backticks >= len => open_fence = None,
            Some(_) => {}
            None if backticks >= 3 => open_fence = Some(backticks),
            None => {
                if let Some(heading) = line.strip_prefix("## ") {
                    sections.push((heading.trim().to_string(), String::new()));
                    continue;
                }
            }
        }
        if let Some((_, body)) = sections.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }

    sections.retain_mut(|(_, body)| {
        *body = body.trim().to_string();
        !body.is_empty()
    });
    sections
}

/// `counted_messages` is `messages` after any toolshim conversion, so previews
/// come from the original while token counts come from what is sent.
fn message_segments(
    messages: &[Message],
    counted_messages: &[Message],
    token_counter: &TokenCounter,
) -> Vec<ContextSegment> {
    let mut grouped: BTreeMap<MessageKind, Vec<(ContextPart, String)>> = BTreeMap::new();
    for (index, message) in messages.iter().enumerate() {
        let counted = counted_messages.get(index).unwrap_or(message);
        let kind = MessageKind::classify(message);
        let text = message_text(message);
        // The request merges same-role neighbours into one message, which pays
        // the per-message framing once; charge the follow-on messages only for
        // their content so the rows sum to what the request pays.
        let merged_into_previous = index
            .checked_sub(1)
            .is_some_and(|previous| effective_role(&messages[previous]) == effective_role(message));
        let mut token_count = token_counter.count_message_tokens(counted);
        if merged_into_previous {
            token_count -= TOKENS_PER_MESSAGE;
        }
        let part = ContextPart {
            label: format!("#{}", index + 1),
            source: None,
            token_count: token_count as u64,
            content_preview: Some(preview(&text)),
        };
        grouped.entry(kind).or_default().push((part, text));
    }

    grouped
        .into_iter()
        .map(|(kind, parts)| {
            let token_count = parts.iter().map(|(part, _)| part.token_count).sum();
            let parts = match (kind, parts.as_slice()) {
                (MessageKind::CompactionSummary, [(_, text)]) => {
                    compaction_summary_parts(text, token_counter)
                        .unwrap_or_else(|| parts.into_iter().map(|(part, _)| part).collect())
                }
                _ => parts.into_iter().map(|(part, _)| part).collect(),
            };
            ContextSegment {
                category: kind.category(),
                label: kind.label().to_string(),
                source: None,
                token_count,
                content_preview: None,
                parts,
            }
        })
        .collect()
}

/// Section counts are measured on the text alone, so they sum to just under
/// the segment, which also carries the message framing.
fn compaction_summary_parts(
    summary: &str,
    token_counter: &TokenCounter,
) -> Option<Vec<ContextPart>> {
    let sections = summary_sections(summary);
    if sections.is_empty() {
        return None;
    }
    Some(
        sections
            .into_iter()
            .map(|(heading, body)| ContextPart {
                label: heading,
                source: None,
                token_count: token_counter.count_tokens(&body) as u64,
                content_preview: Some(preview(&body)),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::MessageMetadata;
    use std::sync::Arc;

    fn test_tool(name: &str, owner: Option<&str>) -> Tool {
        let mut tool = Tool::new(
            name.to_string(),
            format!("{name} does a thing."),
            Arc::new(rmcp::object!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to operate on." },
                    "mode": { "type": "string", "enum": ["read", "write"] }
                }
            })),
        );
        if let Some(owner) = owner {
            tool.meta = Some(rmcp::model::MetaObject(
                rmcp::object!({ "goose_extension": owner }),
            ));
        }
        tool
    }

    fn part_total(segment: &ContextSegment) -> u64 {
        segment.parts.iter().map(|part| part.token_count).sum()
    }

    #[tokio::test]
    async fn tool_segments_account_for_every_tool_token_exactly() {
        let token_counter = TokenCounter::new().await.unwrap();
        let tools = vec![
            test_tool("alpha__read", Some("alpha")),
            test_tool("alpha__write", Some("alpha")),
            test_tool("beta__search", Some("beta")),
            test_tool("orphan", None),
        ];

        let segments = tool_segments(&tools, &token_counter);

        assert_eq!(segments.len(), 3);
        for segment in &segments {
            assert_eq!(segment.category, ContextCategory::ToolDefinitions);
            assert_eq!(segment.token_count, part_total(segment));
        }
        let segment_total: u64 = segments.iter().map(|segment| segment.token_count).sum();
        assert_eq!(
            segment_total,
            token_counter.count_tokens_for_tools(&tools) as u64
        );
    }

    const RENDERED_SUMMARY: &str = "# Conversation Summary

## User Intent
- Fix the parser bug

## Files + Code
### src/parser.rs
Fixed off-by-one in scan loop
````
## heading inside a fence
fn scan() {}
````

## Next Step
Finish the regression test
";

    #[test]
    fn extension_extras_report_as_extension_instructions() {
        // Instructions can carry their own markdown headings, so the key is
        // what names the extension.
        assert_eq!(
            extra_descriptor("extension:autovisualiser"),
            (
                ContextCategory::ExtensionInstructions,
                "autovisualiser".to_string(),
                None
            )
        );
        assert_eq!(
            extra_descriptor("recipe"),
            (
                ContextCategory::AdditionalInstructions,
                "recipe".to_string(),
                None
            )
        );
    }

    #[tokio::test]
    async fn message_segments_group_by_kind_and_sum_to_the_messages() {
        let token_counter = TokenCounter::new().await.unwrap();
        let messages = vec![
            Message::user()
                .with_text(RENDERED_SUMMARY)
                .with_metadata(MessageMetadata::agent_only().with_compaction_summary()),
            Message::assistant().with_text("continuing where we left off"),
            Message::user().with_text("now add the test"),
            Message::user()
                .with_text("<turn-context>\n<current-time>now</current-time>\n</turn-context>")
                .with_metadata(MessageMetadata::agent_only().with_turn_context()),
            Message::assistant().with_tool_request(
                "call_1",
                Ok(rmcp::model::CallToolRequestParams::new("read_file")
                    .with_arguments(rmcp::object!({"path": "a.txt"}))),
            ),
            Message::user().with_tool_response(
                "call_1",
                Ok(rmcp::model::CallToolResult::success(vec![
                    rmcp::model::ContentBlock::text("file contents"),
                ])),
            ),
        ];

        let segments = message_segments(&messages, &messages, &token_counter);

        let labels: Vec<&str> = segments
            .iter()
            .map(|segment| segment.label.as_str())
            .collect();
        assert_eq!(
            labels,
            vec![
                "Turn context",
                "Conversation summary",
                "User messages",
                "Assistant messages",
                "Tool calls",
                "Tool results",
            ]
        );
        let summary = &segments[1];
        assert_eq!(summary.category, ContextCategory::CompactionSummary);
        let headings: Vec<&str> = summary
            .parts
            .iter()
            .map(|part| part.label.as_str())
            .collect();
        assert_eq!(headings, vec!["User Intent", "Files + Code", "Next Step"]);
        assert!(summary.parts[1]
            .content_preview
            .as_deref()
            .unwrap()
            .contains("## heading inside a fence"));
        assert!(part_total(summary) <= summary.token_count);
        assert_eq!(segments[0].category, ContextCategory::TurnContext);
        assert_eq!(segments[2].category, ContextCategory::Messages);
        assert!(segments[4].parts[0]
            .content_preview
            .as_deref()
            .unwrap()
            .contains("read_file"));

        // The prompt and its turn-context event merge into one request message.
        let expected_total: u64 = merge_consecutive_messages_for_request(messages.clone())
            .iter()
            .map(|message| token_counter.count_message_tokens(message) as u64)
            .sum();
        let segment_total: u64 = segments.iter().map(|segment| segment.token_count).sum();
        assert_eq!(segment_total, expected_total);
    }

    #[tokio::test]
    async fn unclaimed_tool_errors_count_the_tool_list_the_request_appends() {
        let token_counter = TokenCounter::new().await.unwrap();
        let mut response = Message::user();
        response.add_tool_response_with_metadata(
            "call_1",
            Ok(rmcp::model::CallToolResult::error(vec![
                rmcp::model::ContentBlock::text("Tool missing__tool is not available."),
            ])),
            Some(&serde_json::Map::from_iter([(
                crate::agents::state_machine::UNCLAIMED_TOOL_ERROR.to_string(),
                true.into(),
            )])),
        );
        let messages = vec![response];
        let tools = vec![
            test_tool("alpha__read", Some("alpha")),
            test_tool("beta__write", Some("beta")),
        ];

        let persisted = message_segments(&messages, &messages, &token_counter);
        let enriched = enrich_unclaimed_tool_errors(&messages, &tools);
        let requested = message_segments(&enriched, &enriched, &token_counter);

        assert!(requested[0].token_count > persisted[0].token_count);
        assert!(requested[0].parts[0]
            .content_preview
            .as_deref()
            .unwrap()
            .contains("Available tools: [alpha__read, beta__write]."));
    }

    #[test]
    fn next_request_keeps_the_last_reply_and_drops_unanswered_tool_requests() {
        let persisted = Conversation::new_unvalidated(vec![
            Message::user().with_text("read a.txt"),
            Message::assistant()
                .with_tool_request(
                    "answered",
                    Ok(rmcp::model::CallToolRequestParams::new("read_file")),
                )
                .with_tool_request(
                    "unanswered",
                    Ok(rmcp::model::CallToolRequestParams::new("read_file")),
                ),
            Message::user().with_tool_response(
                "answered",
                Ok(rmcp::model::CallToolResult::success(vec![
                    rmcp::model::ContentBlock::text("a"),
                ])),
            ),
            Message::assistant().with_text("done"),
        ]);

        let messages = next_request_messages(&persisted);

        assert_eq!(messages.len(), 4);
        let requests: Vec<&str> = messages[1]
            .content
            .iter()
            .filter_map(|content| content.as_tool_request())
            .map(|request| request.id.as_str())
            .collect();
        assert_eq!(requests, ["answered"]);
        assert!(matches!(
            messages[3].content.as_slice(),
            [MessageContent::Text(text)] if text.text == "done"
        ));
    }

    #[test]
    fn next_request_stand_in_leaves_no_trace_in_a_trailing_prompt() {
        let persisted = Conversation::new_unvalidated(vec![
            Message::user().with_text("hello"),
            Message::assistant().with_text("hi"),
            Message::user().with_text("interrupted before a reply"),
        ]);

        let messages = next_request_messages(&persisted);

        assert_eq!(messages.len(), 3);
        assert!(matches!(
            messages[2].content.as_slice(),
            [MessageContent::Text(text)] if text.text == "interrupted before a reply"
        ));
    }
}
