use anyhow::Result;
use goose::config::GooseMode;
use goose::conversation::message::{Message, MessageMetadata};
use goose::conversation::Conversation;
use goose::session::{SessionManager, SessionType};
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
