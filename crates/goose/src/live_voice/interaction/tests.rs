use super::*;
use async_trait::async_trait;
use tokio::sync::oneshot;

struct TestConnection {
    stopped: Option<oneshot::Sender<()>>,
}

struct UndeliveredConnection;

fn test_interaction(
    provider_connection: impl ProviderConnection + 'static,
) -> LiveVoiceInteraction {
    test_interaction_with_publisher(provider_connection, Arc::new(|_| {}))
}

fn test_interaction_with_publisher(
    provider_connection: impl ProviderConnection + 'static,
    transcript_publisher: LiveVoiceTranscriptPublisher,
) -> LiveVoiceInteraction {
    LiveVoiceInteraction::new(
        Box::new(provider_connection),
        Arc::new(SessionManager::new(tempfile::tempdir().unwrap().keep())),
        transcript_publisher,
        LiveMainAgent::new(
            |_, _| Err("unused".into()),
            |_, _| Box::pin(async { Err("unused".into()) }),
        ),
        LiveVoiceInteractionGuard::for_test("test-session"),
    )
}

#[async_trait]
impl ProviderConnection for TestConnection {
    async fn next_event(&mut self) -> ProviderConnectionEvent {
        std::future::pending().await
    }

    async fn send_delegation_update(
        &mut self,
        _update: DelegationUpdate,
    ) -> anyhow::Result<DelegationUpdateDelivery> {
        Ok(DelegationUpdateDelivery::Delivered)
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(stopped) = self.stopped.take() {
            let _ = stopped.send(());
        }
        Ok(())
    }
}

#[async_trait]
impl ProviderConnection for UndeliveredConnection {
    async fn next_event(&mut self) -> ProviderConnectionEvent {
        std::future::pending().await
    }

    async fn send_delegation_update(
        &mut self,
        _update: DelegationUpdate,
    ) -> anyhow::Result<DelegationUpdateDelivery> {
        Ok(DelegationUpdateDelivery::Undelivered)
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn a_live_voice_interaction_owns_and_stops_its_provider_connection() {
    let (stopped, did_stop) = oneshot::channel();
    let mut interaction = test_interaction(TestConnection {
        stopped: Some(stopped),
    });

    interaction.cleanup_provider().await.unwrap();
    did_stop.await.unwrap();
}

#[tokio::test]
async fn an_undelivered_delegation_update_notifies_the_user() {
    let (notice_tx, mut notice_rx) = tokio::sync::mpsc::unbounded_channel();
    let transcript_publisher: LiveVoiceTranscriptPublisher = Arc::new(move |message| {
        notice_tx.send(message).unwrap();
    });
    let mut interaction =
        test_interaction_with_publisher(UndeliveredConnection, transcript_publisher);

    interaction
        .send_delegation_update("delegation-1".into(), "result".into())
        .await
        .unwrap();

    let notice = notice_rx.recv().await.unwrap();
    assert_eq!(notice.role, Role::Assistant);
    assert_eq!(
        notice.as_concat_text(),
        UNDELIVERED_DELEGATION_UPDATE_NOTICE
    );
}

#[tokio::test]
async fn delegation_preparation_respects_offset_and_suppresses_duplicates() {
    let mut interaction = test_interaction(TestConnection { stopped: None });
    interaction.record_transcript("1".into(), Role::Assistant, "ready", 5);
    interaction.record_transcript("2".into(), Role::User, "do ", 10);
    interaction.record_transcript("3".into(), Role::User, "this", 20);
    interaction.record_transcript("4".into(), Role::Assistant, "crossing", 25);
    interaction.record_transcript("5".into(), Role::Assistant, "later", 40);

    let DelegationDecision::Accept(input) =
        interaction.handle_delegation_request("event-1".into(), "delegation-1".into(), 20)
    else {
        panic!("delegation should be accepted");
    };
    assert_eq!(
        input,
        "Live conversation context:\nVoice assistant: ready\nUser: do this"
    );
    interaction
        .transcript
        .mark_context_sent_to_main_agent_through(20);
    assert!(matches!(
        interaction.handle_delegation_request("event-1".into(), "delegation-1".into(), 20),
        DelegationDecision::Ignore
    ));

    interaction
        .record_transcript("6".into(), Role::User, " late detail", 20)
        .unwrap();
    assert_eq!(
        interaction
            .transcript
            .raw_transcript_entries_waiting_to_save()
            .last()
            .unwrap()
            .as_concat_text(),
        "crossinglater"
    );
    let DelegationDecision::Accept(continuation) =
        interaction.handle_delegation_request("event-2".into(), "delegation-2".into(), 20)
    else {
        panic!("continuation should be accepted");
    };
    assert_eq!(
        continuation,
        "Live conversation context:\nUser: late detail"
    );
    interaction
        .transcript
        .mark_context_sent_to_main_agent_through(20);

    assert!(matches!(
        interaction.handle_delegation_request("event-3".into(), "delegation-3".into(), 19),
        DelegationDecision::Reject(_)
    ));

    interaction.record_transcript("7".into(), Role::User, "again", 50);
    let DelegationDecision::Accept(next) =
        interaction.handle_delegation_request("event-4".into(), "delegation-4".into(), 50)
    else {
        panic!("later continuation should be accepted");
    };
    assert_eq!(
        next,
        "Live conversation context:\nVoice assistant: crossinglater\nUser: again"
    );
    interaction
        .transcript
        .mark_context_sent_to_main_agent_through(50);

    let mut missing_user = test_interaction(TestConnection { stopped: None });
    missing_user.record_transcript("1".into(), Role::Assistant, "hello", 10);
    assert!(matches!(
        missing_user.handle_delegation_request("event-1".into(), "delegation-1".into(), 10),
        DelegationDecision::Reject(_)
    ));
}

#[tokio::test]
async fn context_waiting_for_main_agent_excludes_instruction_and_clears_after_handoff() {
    let mut interaction = test_interaction(TestConnection { stopped: None });
    interaction.record_transcript("1".into(), Role::Assistant, "Anything else?", 10);
    interaction.record_transcript("2".into(), Role::User, "No thanks", 20);

    let context = interaction
        .transcript
        .context_waiting_for_main_agent()
        .unwrap();
    assert_eq!(
        context,
        "Live conversation context:\nVoice assistant: Anything else?\nUser: No thanks"
    );
    assert_eq!(
        interaction
            .transcript
            .context_waiting_for_main_agent()
            .unwrap(),
        context
    );

    interaction.record_transcript("3".into(), Role::User, "Already shared", 30);
    interaction
        .transcript
        .mark_context_sent_to_main_agent_through(30);
    assert!(interaction
        .transcript
        .context_waiting_for_main_agent()
        .is_none());
}

#[tokio::test]
async fn transcript_grouping_projects_deltas_and_finalizes_messages() {
    let mut interaction = test_interaction(TestConnection { stopped: None });
    let first = interaction
        .record_transcript("1".into(), Role::User, "hello", 10)
        .unwrap();
    assert!(first.is_user_visible());
    assert!(!first.is_agent_visible());
    let message_id = first.id.clone();
    let second = interaction
        .record_transcript("2".into(), Role::User, " world", 20)
        .unwrap();
    assert_eq!(second.id, message_id);
    assert_eq!(second.as_concat_text(), " world");

    assert!(interaction
        .record_transcript("2".into(), Role::User, " world", 20)
        .is_none());

    let role_change = interaction
        .record_transcript("3".into(), Role::Assistant, "hello", 30)
        .unwrap();
    assert_eq!(
        interaction
            .transcript
            .raw_transcript_entries_waiting_to_save()
            .last()
            .unwrap()
            .as_concat_text(),
        "hello world"
    );
    assert_eq!(role_change.role, Role::Assistant);
    assert!(role_change.is_user_visible());
    assert!(!role_change.is_agent_visible());
}
