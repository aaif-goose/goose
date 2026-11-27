use anyhow::Result;
use goose::config::GooseMode;
use goose::conversation::message::{Message, MessageMetadata};
use goose::conversation::Conversation;
use goose::session::{SessionManager, SessionType};
use oi_kit_providers::conversation::token_usage::Usage;
use tempfile::TempDir;

fn message_at(mut message: Message, created: i64) -> Message {
    message.created = created;
    message
}

fn superseded(mut message: Message) -> Message {
    message.metadata.agent_visible = false;
    message
}

fn compaction_summary(text: &str, created: i64) -> Message {
    message_at(
        Message::user()
            .with_text(text)
            .with_metadata(MessageMetadata::agent_only().with_compaction_summary()),
        created,
    )
}

fn continuation(text: &str, created: i64) -> Message {
    message_at(
        Message::assistant()
            .with_text(text)
            .with_metadata(MessageMetadata::agent_only()),
        created,
    )
}

async fn new_session(sm: &SessionManager, dir: &TempDir, name: &str) -> Result<String> {
    let session = sm
        .create_session(
            dir.path().to_path_buf(),
            name.to_string(),
            SessionType::Hidden,
            GooseMode::default(),
        )
        .await?;
    Ok(session.id)
}

fn agent_texts(session: &goose::session::Session) -> Vec<String> {
    session
        .conversation
        .as_ref()
        .expect("session has a conversation")
        .agent_visible_messages()
        .iter()
        .map(|message| message.as_concat_text())
        .collect()
}

/// A conversation as compaction persists it, with the turn that compaction
/// opened still at the end: compacted history, the summary that replaced it,
/// the reissued kickoff and the turn's reply.
fn compacted_conversation() -> Vec<Message> {
    vec![
        superseded(message_at(Message::user().with_text("first prompt"), 1_000)),
        superseded(message_at(
            Message::assistant().with_text("first reply"),
            1_100,
        )),
        superseded(message_at(
            Message::user()
                .with_text("second prompt")
                .with_id("kickoff-2"),
            2_000,
        )),
        superseded(message_at(
            Message::assistant().with_text("second reply"),
            2_100,
        )),
        compaction_summary("summary of the conversation", 3_000),
        continuation("Your context was compacted.", 3_000),
        message_at(
            Message::user()
                .with_text("second prompt")
                .with_metadata(MessageMetadata::agent_only()),
            3_000,
        ),
        message_at(
            Message::assistant().with_text("second reply, continued"),
            3_100,
        ),
    ]
}

#[tokio::test]
async fn truncating_a_compaction_summary_restores_the_messages_it_hid() -> Result<()> {
    let dir = TempDir::new()?;
    let sm = SessionManager::new(dir.path().to_path_buf());
    let id = new_session(&sm, &dir, "truncate-loses-summary").await?;

    sm.replace_conversation(
        &id,
        &Conversation::new_unvalidated(compacted_conversation()),
    )
    .await?;

    // Editing the reissued kickoff in place truncates from the turn, taking
    // the summary with it.
    sm.truncate_conversation(&id, 3_000).await?;

    let session = sm.get_session(&id, true).await?;
    assert_eq!(
        agent_texts(&session),
        vec![
            "first prompt",
            "first reply",
            "second prompt",
            "second reply"
        ]
    );
    Ok(())
}

#[tokio::test]
async fn truncating_from_a_message_across_a_compaction_summary_restores_visibility() -> Result<()> {
    let dir = TempDir::new()?;
    let sm = SessionManager::new(dir.path().to_path_buf());
    let id = new_session(&sm, &dir, "truncate-from-message-loses-summary").await?;

    sm.replace_conversation(
        &id,
        &Conversation::new_unvalidated(compacted_conversation()),
    )
    .await?;

    sm.truncate_conversation_from_message(&id, "kickoff-2")
        .await?;

    // The kickoff and everything after it is removed; the turn that follows
    // will re-send an edited copy of it.
    let session = sm.get_session(&id, true).await?;
    assert_eq!(agent_texts(&session), vec!["first prompt", "first reply"]);
    Ok(())
}

#[tokio::test]
async fn truncating_after_a_summary_keeps_the_compacted_state() -> Result<()> {
    let dir = TempDir::new()?;
    let sm = SessionManager::new(dir.path().to_path_buf());
    let id = new_session(&sm, &dir, "truncate-keeps-summary").await?;

    let mut conversation = vec![
        superseded(message_at(Message::user().with_text("first prompt"), 1_000)),
        superseded(message_at(
            Message::assistant().with_text("first reply"),
            1_100,
        )),
        compaction_summary("summary of the conversation", 3_000),
        continuation("Your context was compacted.", 3_000),
    ];
    conversation.push(message_at(Message::user().with_text("later prompt"), 4_000));
    conversation.push(message_at(
        Message::assistant().with_text("later reply"),
        4_100,
    ));
    sm.replace_conversation(&id, &Conversation::new_unvalidated(conversation))
        .await?;

    sm.truncate_conversation(&id, 4_000).await?;

    let session = sm.get_session(&id, true).await?;
    assert_eq!(
        agent_texts(&session),
        vec!["summary of the conversation", "Your context was compacted."]
    );
    Ok(())
}

#[tokio::test]
async fn truncating_the_newest_of_two_compactions_reactivates_the_older_summary() -> Result<()> {
    let dir = TempDir::new()?;
    let sm = SessionManager::new(dir.path().to_path_buf());
    let id = new_session(&sm, &dir, "truncate-two-compactions").await?;

    // The first compaction's summary was itself superseded by the second.
    let conversation = vec![
        superseded(message_at(Message::user().with_text("first prompt"), 1_000)),
        superseded(message_at(
            Message::assistant().with_text("first reply"),
            1_100,
        )),
        superseded(compaction_summary(
            "summary of the early conversation",
            3_000,
        )),
        superseded(continuation("Your context was compacted.", 3_000)),
        superseded(message_at(
            Message::user().with_text("second prompt"),
            4_000,
        )),
        superseded(message_at(
            Message::assistant().with_text("second reply"),
            4_100,
        )),
        compaction_summary("summary of the later conversation", 5_000),
        continuation("Your context was compacted again.", 5_000),
        message_at(
            Message::assistant().with_text("reply after the second compaction"),
            5_100,
        ),
    ];
    sm.replace_conversation(&id, &Conversation::new_unvalidated(conversation))
        .await?;

    sm.truncate_conversation(&id, 5_000).await?;

    let session = sm.get_session(&id, true).await?;
    assert_eq!(
        agent_texts(&session),
        vec![
            "summary of the early conversation",
            "Your context was compacted.",
            "second prompt",
            "second reply",
        ]
    );
    Ok(())
}

#[tokio::test]
async fn restoring_returns_agent_only_rows_and_keeps_approval_receipts_hidden() -> Result<()> {
    let dir = TempDir::new()?;
    let sm = SessionManager::new(dir.path().to_path_buf());
    let id = new_session(&sm, &dir, "truncate-message-shapes").await?;

    // The agent-only shapes that compaction superseded and the restore must
    // bring back: the slash-command kickoff an operation hid, its expanded
    // instructions, a delivered subagent result, and a delivered steer. The
    // tool-approval receipt was never part of the agent's context.
    let mut approval = message_at(Message::user().with_text("approval receipt"), 2_050);
    approval.metadata.user_visible = false;
    approval.metadata.agent_visible = false;
    approval
        .metadata
        .set_operation_note("tool_approval", "seen", serde_json::Value::from(true));

    let mut conversation = vec![
        superseded(message_at(Message::user().with_text("first prompt"), 1_000)),
        superseded(message_at(
            Message::user().with_text("/skill command"),
            2_000,
        )),
        superseded(message_at(
            Message::user().with_text("Full skill instructions"),
            2_000,
        )),
        approval,
        superseded(message_at(
            Message::user().with_steer().with_text("steer instruction"),
            2_050,
        )),
        superseded(message_at(
            Message::user()
                .with_text("subagent result")
                .with_visibility(false, true),
            2_050,
        )),
        superseded(message_at(
            Message::assistant().with_text("skill work reply"),
            2_100,
        )),
        compaction_summary("summary of the conversation", 3_000),
        message_at(
            Message::assistant().with_text("reply after compaction"),
            3_100,
        ),
    ];
    // Mark the subagent result as delivered, as the subagent operation does.
    conversation[5].metadata.set_operation_note(
        "foreground_subagent",
        "delivered",
        serde_json::Value::from(true),
    );

    sm.replace_conversation(&id, &Conversation::new_unvalidated(conversation))
        .await?;

    sm.truncate_conversation(&id, 3_000).await?;

    let session = sm.get_session(&id, true).await?;
    assert_eq!(
        agent_texts(&session),
        vec![
            "first prompt",
            "/skill command",
            "Full skill instructions",
            "steer instruction",
            "subagent result",
            "skill work reply",
        ]
    );
    Ok(())
}

#[tokio::test]
async fn restoring_truncate_refreshes_the_cached_context_usage() -> Result<()> {
    let dir = TempDir::new()?;
    let sm = SessionManager::new(dir.path().to_path_buf());
    let id = new_session(&sm, &dir, "truncate-usage-refresh").await?;

    let mut conversation = compacted_conversation();
    conversation.pop();
    sm.replace_conversation(&id, &Conversation::new_unvalidated(conversation))
        .await?;

    // The stale post-compaction usage of the lost turn.
    sm.update(&id)
        .usage(Usage::new(Some(1), None, Some(1)))
        .apply()
        .await?;

    sm.truncate_conversation(&id, 3_000).await?;

    let session = sm.get_session(&id, true).await?;
    let total = session.usage.total_tokens.unwrap_or(0);
    assert!(
        total > 1,
        "expected the restored conversation to be counted, got {total}"
    );
    Ok(())
}
