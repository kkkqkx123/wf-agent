use wf_execution_shared::types::state_manager::StateManager;
use wf_execution_shared::ConversationSession;
use wf_types::message::Message;

#[tokio::test]
async fn new_session_is_empty_with_zero_size() {
    let session = ConversationSession::new();
    assert!(session.is_empty());
    assert_eq!(session.size(), 0);
}

#[tokio::test]
async fn added_messages_grow_size() {
    let mut session = ConversationSession::new();
    session.add_message(Message::user_text("hello".to_string()));
    session.add_message(Message::system_text("ready".to_string()));
    assert!(!session.is_empty());
    assert_eq!(session.size(), 2);
}

#[tokio::test]
async fn snapshot_and_restore_roundtrip() {
    let mut session = ConversationSession::new();
    session.add_message(Message::user_text("hello".to_string()));
    let snapshot = session.create_snapshot().await.expect("snapshot succeeds");

    let mut restored = ConversationSession::new();
    restored
        .restore_from_snapshot(snapshot)
        .await
        .expect("restore succeeds");
    assert_eq!(restored.size(), 1);
    assert_eq!(restored.state.messages, session.state.messages);
}

#[tokio::test]
async fn restore_replaces_existing_messages() {
    let mut session = ConversationSession::new();
    session.add_message(Message::user_text("first".to_string()));
    let snapshot = session.create_snapshot().await.expect("snapshot succeeds");

    let mut other = ConversationSession::new();
    other.add_message(Message::user_text("stale-a".to_string()));
    other.add_message(Message::user_text("stale-b".to_string()));
    other
        .restore_from_snapshot(snapshot)
        .await
        .expect("restore succeeds");
    assert_eq!(other.size(), 1);
    assert_eq!(other.state.messages, session.state.messages);
}

#[tokio::test]
async fn cleanup_resets_to_empty() {
    let mut session = ConversationSession::new();
    session.add_message(Message::user_text("hello".to_string()));
    session.cleanup().await.expect("cleanup succeeds");
    assert!(session.is_empty());
    assert_eq!(session.size(), 0);
}
