//! Classifies output-token-limit markers by what the provider call produced
//! before it stopped, so clients can tell a partial answer from a response
//! whose reasoning used the whole budget.

use rmcp::model::Role;
use serde::Serialize;

use crate::conversation::message::{Message, MessageContent, SystemNotificationType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OutputTokenLimit {
    /// Visible output arrived before the limit, so the answer may be incomplete.
    Partial,
    /// The provider stopped before returning anything.
    Empty,
    /// Reasoning used the whole budget before any visible output.
    Reasoning,
}

/// Follows one provider call across streamed messages. Streams split
/// reasoning, answer text and the empty limit marker into separate messages
/// whose ids may differ, so the marker cannot be classified on its own.
/// Callers must also `reset` at provider-call boundaries that never arrive as
/// messages, such as usage reports and history replacement.
#[derive(Debug, Default)]
pub struct OutputTokenLimitTracker {
    saw_thinking: bool,
    saw_visible_output: bool,
    response_complete: bool,
}

impl OutputTokenLimitTracker {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Returns how the response ended when `message` carries the limit marker.
    pub fn observe(&mut self, message: &Message) -> Option<OutputTokenLimit> {
        if message.role != Role::Assistant {
            self.reset();
            return None;
        }
        if self.response_complete {
            self.reset();
        }
        self.saw_thinking |= message.has_thinking_content();
        self.saw_visible_output |= message.has_visible_output();
        let reached_limit = message.metadata.output_token_limit_reached;
        self.response_complete = reached_limit || is_inline_notification(message);
        reached_limit.then(|| self.classify())
    }

    fn classify(&self) -> OutputTokenLimit {
        if self.saw_visible_output {
            OutputTokenLimit::Partial
        } else if self.saw_thinking {
            OutputTokenLimit::Reasoning
        } else {
            OutputTokenLimit::Empty
        }
    }
}

/// Goal and Grind nudges announce themselves inline before prompting the next
/// provider call.
fn is_inline_notification(message: &Message) -> bool {
    message.content.iter().any(|content| {
        matches!(
            content,
            MessageContent::SystemNotification(notification)
                if notification.notification_type == SystemNotificationType::InlineMessage
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::CallToolRequestParams;

    fn marker() -> Message {
        let mut marker = Message::assistant().with_generated_id();
        marker.metadata.output_token_limit_reached = true;
        marker
    }

    fn observe_all(tracker: &mut OutputTokenLimitTracker, messages: &[Message]) {
        for message in messages {
            tracker.observe(message);
        }
    }

    #[test]
    fn single_message_classification() {
        let cases = [
            (
                Message::assistant().with_text("Partial answer"),
                OutputTokenLimit::Partial,
            ),
            (Message::assistant(), OutputTokenLimit::Empty),
            (
                Message::assistant().with_thinking("internal reasoning", ""),
                OutputTokenLimit::Reasoning,
            ),
            (
                Message::assistant().with_thinking("", "encrypted-payload"),
                OutputTokenLimit::Reasoning,
            ),
            (
                Message::assistant().with_thinking("  ", ""),
                OutputTokenLimit::Empty,
            ),
        ];
        for (mut message, expected) in cases {
            message.metadata.output_token_limit_reached = true;
            let mut tracker = OutputTokenLimitTracker::default();
            assert_eq!(tracker.observe(&message), Some(expected), "{message:?}");
        }
    }

    #[test]
    fn messages_without_the_marker_are_not_classified() {
        let mut tracker = OutputTokenLimitTracker::default();
        assert_eq!(
            tracker.observe(&Message::assistant().with_thinking("reasoning", "")),
            None
        );
        assert_eq!(tracker.observe(&Message::user().with_text("hi")), None);
    }

    #[test]
    fn split_reasoning_and_marker_is_reasoning_whatever_the_ids() {
        for (thinking_id, marker_id) in [
            ("chatcmpl-1", "chatcmpl-1"),
            ("chatcmpl-1", "chatcmpl-2"),
            ("msg_a", "msg_b"),
        ] {
            let mut tracker = OutputTokenLimitTracker::default();
            tracker.observe(
                &Message::assistant()
                    .with_id(thinking_id)
                    .with_thinking("I will reason until the budget runs out.", ""),
            );
            let marker = marker().with_id(marker_id);
            assert_eq!(
                tracker.observe(&marker),
                Some(OutputTokenLimit::Reasoning),
                "{thinking_id} -> {marker_id}"
            );
        }
    }

    #[test]
    fn visible_output_anywhere_in_the_response_is_partial() {
        let mut tracker = OutputTokenLimitTracker::default();
        observe_all(
            &mut tracker,
            &[
                Message::assistant().with_thinking("reasoning", ""),
                Message::assistant().with_text("Partial answer"),
            ],
        );
        assert_eq!(tracker.observe(&marker()), Some(OutputTokenLimit::Partial));

        let mut tracker = OutputTokenLimitTracker::default();
        observe_all(
            &mut tracker,
            &[
                Message::assistant().with_text("Hi"),
                Message::assistant().with_thinking("later reasoning", ""),
            ],
        );
        assert_eq!(tracker.observe(&marker()), Some(OutputTokenLimit::Partial));
    }

    #[test]
    fn tool_call_before_its_marker_is_partial() {
        let mut tracker = OutputTokenLimitTracker::default();
        tracker.observe(
            &Message::assistant()
                .with_id("msg_tool")
                .with_tool_request("call-1", Ok(CallToolRequestParams::new("shell"))),
        );
        assert_eq!(
            tracker.observe(&marker().with_id("msg_tool")),
            Some(OutputTokenLimit::Partial)
        );
    }

    #[test]
    fn earlier_calls_do_not_leak_into_the_next_marker() {
        let goal = Message::assistant()
            .with_system_notification(SystemNotificationType::InlineMessage, "Goal: keep going");
        let boundaries = [
            Message::user().with_text("continue"),
            Message::user()
                .with_text("You did some thinking but did not produce an answer.")
                .with_visibility(false, true),
            goal,
            marker(),
        ];
        for boundary in boundaries {
            let mut tracker = OutputTokenLimitTracker::default();
            observe_all(
                &mut tracker,
                &[
                    Message::assistant().with_thinking("previous call reasoning", ""),
                    boundary.clone(),
                ],
            );
            assert_eq!(
                tracker.observe(&marker()),
                Some(OutputTokenLimit::Empty),
                "{boundary:?}"
            );
        }
    }

    #[test]
    fn reset_discards_the_previous_call() {
        let mut tracker = OutputTokenLimitTracker::default();
        tracker.observe(&Message::assistant().with_text("discarded attempt"));
        tracker.reset();
        tracker.observe(&Message::assistant().with_thinking("retry reasoning", ""));
        assert_eq!(
            tracker.observe(&marker()),
            Some(OutputTokenLimit::Reasoning)
        );
    }
}
