//! The conversation layout left behind by a compaction: the originals stay for
//! the user but leave the agent's context, which continues from the summary.

use goose_provider_types::conversation::message::{Message, MessageContent, MessageMetadata};
use goose_provider_types::conversation::{merge_consecutive_messages, Conversation};
use rmcp::model::Role;

pub const CONVERSATION_CONTINUATION_TEXT: &str =
    "Your context was compacted. The previous message contains a summary of the conversation so far.
Do not mention that you read a summary or that conversation summarization occurred.
Just continue the conversation naturally based on the summarized context.";

pub const TOOL_LOOP_CONTINUATION_TEXT: &str =
    "Your context was compacted. The previous message contains a summary of the conversation so far.
Do not mention that you read a summary or that conversation summarization occurred.
Continue calling tools as necessary to complete the task.";

pub const MANUAL_COMPACT_CONTINUATION_TEXT: &str =
    "Your context was compacted at the user's request. The previous message contains a summary of the conversation so far.
Do not mention that you read a summary or that conversation summarization occurred.
Just continue the conversation naturally based on the summarized context.";

fn text_only(message: &Message) -> bool {
    let has_text = message
        .content
        .iter()
        .any(|content| matches!(content, MessageContent::Text(_)));
    let has_tool_content = message.content.iter().any(|content| {
        matches!(
            content,
            MessageContent::ToolRequest(_) | MessageContent::ToolResponse(_)
        )
    });
    has_text && !has_tool_content
}

/// The latest agent-visible user prompt in `messages`: the message to re-issue
/// after the summary so the model answers it against the compacted context.
/// Turn-context events and agent-only requests (whose reply is hidden) never
/// qualify.
pub fn latest_prompt(messages: &[Message]) -> Option<usize> {
    messages.iter().rposition(|message| {
        message.is_agent_visible()
            && !message.is_turn_context()
            && !message.metadata.hide_response
            && message.role == Role::User
            && text_only(&message.agent_visible_content())
    })
}

fn reissued(prompt: &Message) -> Message {
    prompt
        .agent_visible_content()
        .content
        .into_iter()
        .filter(|content| matches!(content, MessageContent::Text(_)))
        .fold(
            Message::user().with_metadata(MessageMetadata::agent_only()),
            Message::with_content,
        )
}

/// Lays out the compacted conversation: every message becomes agent-invisible,
/// followed by the agent-only `summary`, a continuation instruction, and the
/// re-issued `prompt` with its turn context. `messages[..end]` is what the
/// summary covers; anything after it (an in-band request and its reply) is
/// archived without a say in the continuation.
pub fn compacted_conversation(
    messages: &[Message],
    end: usize,
    summary: Message,
    prompt: Option<usize>,
    manual: bool,
) -> Conversation {
    let prompt_is_latest = prompt.is_some_and(|index| {
        messages[index + 1..end]
            .iter()
            .all(Message::is_turn_context)
    });

    let mut compacted: Vec<Message> = messages
        .iter()
        .map(|message| {
            message
                .clone()
                .with_metadata(message.metadata.clone().with_agent_invisible())
        })
        .collect();

    let continuation_text = if manual {
        MANUAL_COMPACT_CONTINUATION_TEXT
    } else if prompt_is_latest {
        CONVERSATION_CONTINUATION_TEXT
    } else {
        TOOL_LOOP_CONTINUATION_TEXT
    };
    let continuation = Message::assistant()
        .with_text(continuation_text)
        .with_metadata(MessageMetadata::agent_only());
    let continuation_created = continuation.created;
    let (merged, _issues) = merge_consecutive_messages(vec![summary, continuation]);
    compacted.extend(merged);

    if let Some(index) = prompt {
        let mut reissued = reissued(&messages[index]);
        reissued.created = continuation_created;
        compacted.push(reissued);

        // Carry the turn's own context event (it follows the prompt) so a
        // mid-turn retry keeps it; anything earlier belongs to a previous turn.
        if let Some(turn_context) = messages[index + 1..]
            .iter()
            .rev()
            .find(|message| message.is_turn_context() && message.is_agent_visible())
        {
            let mut carried = turn_context.clone();
            carried.id = None;
            // Storage reloads order by created timestamp; the copy must keep its
            // appended position, not resurface at the original event's time.
            if let Some(latest) = compacted.iter().map(|message| message.created).max() {
                carried.created = carried.created.max(latest);
            }
            compacted.push(carried);
        }
    }

    Conversation::new_unvalidated(compacted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn_context(text: &str) -> Message {
        Message::user()
            .with_text(text)
            .with_metadata(MessageMetadata::agent_only().with_turn_context())
    }

    fn summary() -> Message {
        Message::user()
            .with_text("summary")
            .with_metadata(MessageMetadata::agent_only().with_compaction_summary())
    }

    fn agent_texts(conversation: &Conversation) -> Vec<String> {
        conversation
            .agent_visible_messages()
            .iter()
            .map(Message::as_concat_text)
            .collect()
    }

    #[test]
    fn preserves_the_prompt_and_carries_only_its_own_turn_context() {
        let messages = [
            Message::user().with_text("earlier request"),
            turn_context("<turn-context>stale</turn-context>"),
            Message::assistant().with_text("earlier response"),
            Message::user().with_text("the real current request"),
            turn_context("<turn-context>frozen</turn-context>"),
        ];
        let compacted = compacted_conversation(
            &messages,
            messages.len(),
            summary(),
            latest_prompt(&messages),
            false,
        );

        assert_eq!(
            agent_texts(&compacted),
            [
                "summary",
                CONVERSATION_CONTINUATION_TEXT,
                "the real current request",
                "<turn-context>frozen</turn-context>",
            ]
        );
        assert!(compacted
            .messages()
            .iter()
            .filter(|message| !message.is_agent_visible())
            .all(|message| message.is_turn_context() || message.is_user_visible()));
    }

    #[test]
    fn an_in_band_reply_is_archived_without_shaping_the_continuation() {
        let mut request = Message::user()
            .with_text("write working notes")
            .with_metadata(MessageMetadata::agent_only());
        request.metadata.hide_response = true;
        let messages = [
            Message::user().with_text("the prompt"),
            turn_context("<turn-context>now</turn-context>"),
            request,
            Message::assistant().with_text("the notes"),
        ];
        let compacted = compacted_conversation(
            &messages,
            2,
            summary(),
            latest_prompt(&messages[..2]),
            false,
        );

        assert_eq!(
            agent_texts(&compacted),
            [
                "summary",
                CONVERSATION_CONTINUATION_TEXT,
                "the prompt",
                "<turn-context>now</turn-context>",
            ]
        );
        assert!(compacted
            .messages()
            .iter()
            .take(messages.len())
            .all(|message| !message.is_agent_visible()));
    }

    #[test]
    fn manual_compaction_does_not_replay_a_prompt() {
        let messages = [
            Message::user().with_text("the prompt"),
            Message::assistant().with_text("done"),
        ];
        let compacted = compacted_conversation(&messages, messages.len(), summary(), None, true);

        assert_eq!(
            agent_texts(&compacted),
            ["summary", MANUAL_COMPACT_CONTINUATION_TEXT]
        );
    }
}
