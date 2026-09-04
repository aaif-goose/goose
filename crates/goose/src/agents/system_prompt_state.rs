//! Keeps the request `system` frozen for the life of a conversation so the
//! cached prefix and any thinking bound to it stay valid. Later changes are
//! appended to the conversation as system-update messages instead.

use anyhow::Result;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::conversation::message::{Message, MessageMetadata};
use crate::conversation::Conversation;
use crate::session::extension_data::ExtensionData;
use crate::session::SessionManager;
use rmcp::model::Role;

const STATE_NAME: &str = "system_prompt";
const STATE_VERSION: &str = "v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemPromptSections {
    pub base: String,
    pub extras: IndexMap<String, String>,
}

impl SystemPromptSections {
    pub fn render(&self) -> String {
        if self.extras.is_empty() {
            self.base.clone()
        } else {
            let extras: Vec<&str> = self.extras.values().map(String::as_str).collect();
            format!(
                "{}\n\n# Additional Instructions:\n\n{}",
                self.base,
                extras.join("\n\n")
            )
        }
    }
}

/// `sections` is what `system` renders. An update counts as told to the model
/// only while its message is still visible in the conversation, so history
/// rewrites (retry, cancel) re-emit whatever they removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemPromptSnapshot {
    pub system: String,
    pub sections: SystemPromptSections,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub updates: Vec<DeliveredUpdate>,
}

impl SystemPromptSnapshot {
    fn told(&self) -> SystemPromptSections {
        let mut sections = self.sections.clone();
        for update in &self.updates {
            update.delta.apply(&mut sections);
        }
        sections
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveredUpdate {
    pub message_id: String,
    pub delta: SectionsDelta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionsDelta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub extras: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
}

impl SectionsDelta {
    fn between(previous: &SystemPromptSections, live: &SystemPromptSections) -> Option<Self> {
        let delta = Self {
            base: (previous.base != live.base).then(|| live.base.clone()),
            extras: live
                .extras
                .iter()
                .filter(|(key, text)| previous.extras.get(*key) != Some(text))
                .map(|(key, text)| (key.clone(), text.clone()))
                .collect(),
            removed: previous
                .extras
                .keys()
                .filter(|key| !live.extras.contains_key(*key))
                .cloned()
                .collect(),
        };
        let empty = delta.base.is_none() && delta.extras.is_empty() && delta.removed.is_empty();
        (!empty).then_some(delta)
    }

    fn apply(&self, sections: &mut SystemPromptSections) {
        if let Some(base) = &self.base {
            sections.base = base.clone();
        }
        for (key, text) in &self.extras {
            sections.extras.insert(key.clone(), text.clone());
        }
        for key in &self.removed {
            sections.extras.shift_remove(key);
        }
    }

    fn describe(&self, previous: &SystemPromptSections) -> String {
        let mut parts = vec![
            "System prompt update. The following applies from this point on, in addition to your \
             original instructions."
                .to_string(),
        ];
        if let Some(base) = &self.base {
            parts.push(format!(
                "The main system instructions have been replaced with the following:\n\n{base}"
            ));
        }
        for (key, text) in &self.extras {
            if previous.extras.contains_key(key) {
                parts.push(format!(
                    "The additional instructions \"{key}\" have changed to:\n\n{text}"
                ));
            } else {
                parts.push(format!("New additional instructions \"{key}\":\n\n{text}"));
            }
        }
        for key in &self.removed {
            parts.push(format!(
                "The additional instructions \"{key}\" no longer apply."
            ));
        }
        parts.join("\n\n")
    }
}

#[derive(Debug, Clone)]
pub struct PendingSystemUpdate {
    pub message: Message,
    snapshot: SystemPromptSnapshot,
}

impl PendingSystemUpdate {
    pub async fn commit(self, session_manager: &SessionManager, session_id: &str) -> Result<()> {
        store_snapshot(session_manager, session_id, &self.snapshot).await
    }
}

pub struct FrozenSystemPrompt {
    pub system: String,
    pub update: Option<PendingSystemUpdate>,
}

fn system_update_message(text: String) -> Message {
    Message::user()
        .with_text(text)
        .with_metadata(MessageMetadata::agent_only().with_system_update())
        .with_generated_id()
}

fn load_snapshot(data: &ExtensionData) -> Option<SystemPromptSnapshot> {
    data.get_extension_state(STATE_NAME, STATE_VERSION)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
}

async fn store_snapshot(
    session_manager: &SessionManager,
    session_id: &str,
    snapshot: &SystemPromptSnapshot,
) -> Result<()> {
    let mut data = session_manager
        .get_session(session_id, false)
        .await?
        .extension_data;
    data.set_extension_state(STATE_NAME, STATE_VERSION, serde_json::to_value(snapshot)?);
    session_manager
        .update(session_id)
        .extension_data(data)
        .apply()
        .await
}

pub async fn freeze(
    session_manager: &SessionManager,
    session_id: &str,
    live: SystemPromptSections,
    conversation: &Conversation,
) -> Result<FrozenSystemPrompt> {
    let session = session_manager.get_session(session_id, false).await?;
    let stored = load_snapshot(&session.extension_data);
    let (snapshot, frozen) = resolve(stored.clone(), live, conversation);
    if stored.as_ref() != Some(&snapshot) {
        store_snapshot(session_manager, session_id, &snapshot).await?;
    }
    Ok(frozen)
}

fn resolve(
    stored: Option<SystemPromptSnapshot>,
    live: SystemPromptSections,
    conversation: &Conversation,
) -> (SystemPromptSnapshot, FrozenSystemPrompt) {
    let previous = stored
        .filter(|_| has_assistant_turn(conversation))
        .map(|snapshot| settle(snapshot, conversation));
    let Some(previous) = previous else {
        let snapshot = SystemPromptSnapshot {
            system: live.render(),
            sections: live,
            updates: Vec::new(),
        };
        let frozen = FrozenSystemPrompt {
            system: snapshot.system.clone(),
            update: None,
        };
        return (snapshot, frozen);
    };
    let told = previous.told();
    let update = SectionsDelta::between(&told, &live).map(|delta| {
        let message = system_update_message(delta.describe(&told));
        let mut snapshot = previous.clone();
        snapshot.updates.push(DeliveredUpdate {
            message_id: message.id.clone().expect("generated id"),
            delta,
        });
        PendingSystemUpdate { message, snapshot }
    });
    let frozen = FrozenSystemPrompt {
        system: previous.system.clone(),
        update,
    };
    (previous, frozen)
}

fn settle(mut snapshot: SystemPromptSnapshot, conversation: &Conversation) -> SystemPromptSnapshot {
    snapshot.updates.retain(|update| {
        conversation.iter().any(|message| {
            message.is_agent_visible() && message.id.as_deref() == Some(update.message_id.as_str())
        })
    });
    snapshot
}

fn has_assistant_turn(conversation: &Conversation) -> bool {
    conversation
        .iter()
        .any(|message| message.is_agent_visible() && message.role == Role::Assistant)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sections(base: &str, extras: &[(&str, &str)]) -> SystemPromptSections {
        SystemPromptSections {
            base: base.to_string(),
            extras: extras
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    fn answered() -> Conversation {
        Conversation::new_unvalidated(vec![
            Message::user().with_text("hi"),
            Message::assistant().with_text("hello"),
        ])
    }

    fn frozen(base: &str) -> SystemPromptSnapshot {
        SystemPromptSnapshot {
            system: base.to_string(),
            sections: sections(base, &[]),
            updates: Vec::new(),
        }
    }

    #[test]
    fn delta_describes_and_applies_added_changed_and_removed_extras() {
        let previous = sections("base", &[("hints", "h"), ("gone", "g"), ("same", "s")]);
        let live = sections("base", &[("hints", "h2"), ("same", "s"), ("new", "n")]);
        let delta = SectionsDelta::between(&previous, &live).unwrap();

        let text = delta.describe(&previous);
        assert!(text.contains("\"hints\" have changed to:\n\nh2"));
        assert!(text.contains("New additional instructions \"new\":\n\nn"));
        assert!(text.contains("\"gone\" no longer apply."));
        assert!(!text.contains("\"same\""));

        let mut rebuilt = previous.clone();
        delta.apply(&mut rebuilt);
        assert_eq!(rebuilt.render(), live.render());
    }

    #[test]
    fn prompt_is_live_until_the_first_assistant_turn() {
        let unanswered = Conversation::new_unvalidated(vec![Message::user().with_text("hi")]);
        let (snapshot, result) = resolve(Some(frozen("old")), sections("new", &[]), &unanswered);
        assert_eq!(result.system, "new");
        assert!(result.update.is_none());
        assert_eq!(snapshot.sections, sections("new", &[]));

        let (_, result) = resolve(Some(frozen("old")), sections("new", &[]), &answered());
        assert_eq!(result.system, "old");
        assert!(result.update.is_some());
    }

    #[test]
    fn an_update_counts_only_while_its_message_is_in_the_conversation() {
        let live = sections("base", &[("hints", "h")]);
        let (_, result) = resolve(Some(frozen("base")), live.clone(), &answered());
        let update = result.update.unwrap();
        let committed = update.snapshot;

        let (_, lost) = resolve(Some(committed.clone()), live.clone(), &answered());
        assert!(
            lost.update.is_some(),
            "a message that never landed is re-sent"
        );

        let mut delivered = answered();
        delivered.push(update.message);
        delivered.push(Message::assistant().with_text("ok"));
        let (settled, told) = resolve(Some(committed), live.clone(), &delivered);
        assert!(told.update.is_none());
        assert_eq!(settled.told(), live);

        let (_, rolled_back) = resolve(Some(settled), live, &answered());
        assert!(
            rolled_back.update.is_some(),
            "a retry that removed the message re-sends the update"
        );
    }
}
