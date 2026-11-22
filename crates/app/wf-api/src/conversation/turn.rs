//! Submitting a turn and driving it to convergence.
//!
//! Submitting is a short write: the user node enters the tree, the reply is
//! `Pending`, the in-flight slot is claimed. Generation then runs detached from
//! any connection, publishing events as it goes; a client that disconnects
//! loses the stream, not the answer.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use wf_execution_shared::conversation_tree::{self, TreeEditor};
use wf_execution_shared::ChatSession;
use wf_types::llm::{MessageStreamEvent, TokenUsageStats};
use wf_types::message::{Message, MessageRole};
use wf_types::{ConversationNode, MessageRef, ParamsFingerprint, Reply, ReplyState};

use super::query::session_profile;
use super::runtime::{self, InflightTurn, SessionRuntime};
use super::{events, store};
use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Parameters of one turn.
#[derive(Debug, Clone, Default)]
pub struct TurnRequest {
    pub session_id: String,
    /// Profile that serves this turn. Absent continues with the tree's profile.
    pub profile_id: Option<String>,
    pub message: String,
    pub images: Vec<wf_types::message::UserImageInput>,
    /// Node the new turn hangs under. Absent appends at the active leaf.
    pub parent: Option<String>,
    /// Optimistic concurrency: the tree version the client last saw.
    pub if_version: Option<u64>,
    pub directory: Option<String>,
}

/// What the caller gets back: the ids to follow the turn with and the version
/// the tree is at after the user node landed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TurnHandle {
    pub turn_id: String,
    pub node_id: String,
    pub parent: String,
    pub version: u64,
}

/// Answer of one generation, before it is attached to the tree.
pub(crate) struct GeneratedAnswer {
    pub message: Option<Message>,
    pub usage: Option<TokenUsageStats>,
    pub state: ReplyState,
    pub error: Option<String>,
}

impl GeneratedAnswer {
    fn failed(error: impl Into<String>) -> Self {
        Self {
            message: None,
            usage: None,
            state: ReplyState::Failed,
            error: Some(error.into()),
        }
    }

    fn interrupted() -> Self {
        Self {
            message: None,
            usage: None,
            state: ReplyState::Interrupted,
            error: None,
        }
    }
}

/// Submit one turn: write the user node, claim the slot, start the driver.
pub async fn submit_turn(ctx: Arc<ApiContext>, request: TurnRequest) -> ApiResult<TurnHandle> {
    let text = request.message.trim().to_string();
    if text.is_empty() && request.images.is_empty() {
        return Err(ApiError::Validation("chat turn requires a message".into()));
    }
    // Mentions touch other resources (files, definitions) and must not hold the
    // session lock while they resolve.
    let (mention_messages, mention_images, mention_receipts) =
        crate::agent::composition::resolve_mentions(&ctx, &text).await;
    let mut images = request.images;
    images.extend(mention_images);
    if let Err(reason) = wf_types::message::validate_user_images(&images) {
        return Err(ApiError::Validation(format!(
            "chat turn carries invalid images: {reason}"
        )));
    }

    super::require_chat_session(&ctx, &request.session_id).await?;
    let runtime = runtime::runtime_for(&request.session_id);
    // Refuse before writing anything: a second turn would have to append a
    // second pending answer under a head that is still generating.
    if runtime.inflight().is_some() {
        return Err(ApiError::Conflict(
            "session is already generating a turn".into(),
        ));
    }
    let handle;
    {
        let _guard = runtime.write().await;
        let mut tree = store::load_or_adopt(&ctx, &request.session_id).await?;
        if let Some(expected) = request.if_version {
            if tree.version != expected {
                return Err(ApiError::Conflict(format!(
                    "tree is at version {}, not {expected}",
                    tree.version
                )));
            }
        }
        let parent = request
            .parent
            .clone()
            .or_else(|| tree.active_leaf.clone())
            .unwrap_or_else(|| tree.root.clone());
        if parent != tree.root && tree.node(&parent).is_none() {
            return Err(ApiError::not_found("conversation node", &parent));
        }
        if tree.node(&parent).is_some_and(|node| node.is_deleted()) {
            return Err(ApiError::Validation(
                "cannot append a turn under a deleted node".into(),
            ));
        }
        let profile_id = match request
            .profile_id
            .clone()
            .or_else(|| tree.profile_id.clone())
        {
            Some(profile) if !profile.trim().is_empty() => profile,
            _ => session_profile(&ctx, &tree)
                .await
                .filter(|profile| !profile.trim().is_empty())
                .ok_or_else(|| ApiError::Validation("chat turn requires a profile id".into()))?,
        };
        if tree.profile_id.is_none() {
            tree.profile_id = Some(profile_id.clone());
        }

        let now = wf_common::now();
        let user_message = build_user_message(&text, &images, now)?;
        // The injected context of this turn: mention payloads plus their
        // receipt, stored so the tree can reproduce the exact request later.
        let mut context_messages: Vec<Message> = mention_messages;
        if !mention_receipts.is_empty() {
            let lines: Vec<String> = mention_receipts
                .iter()
                .map(|receipt| format!("@{} → {}", receipt.reference, receipt.outcome))
                .collect();
            let mut receipt =
                crate::dynamic_context_message(format!("mention receipts: {}", lines.join("; ")));
            if let Ok(json) = serde_json::to_string(&mention_receipts) {
                if let Some(metadata) = receipt.metadata.as_mut() {
                    metadata.insert(
                        "mention_receipts".to_string(),
                        serde_json::Value::String(json),
                    );
                }
            }
            context_messages.push(receipt);
        }
        for message in &context_messages {
            persist_message(&ctx, &request.session_id, message.clone()).await?;
        }
        persist_message(&ctx, &request.session_id, user_message.clone()).await?;

        let node_id = wf_common::generate_id();
        let user_ref = MessageRef::new(
            user_message.id.clone(),
            conversation_tree::message_hash(&user_message),
        );
        // `append_turn` allocates the sibling ordinal and overwrites this one.
        let mut node =
            ConversationNode::turn(node_id.clone(), parent.clone(), 0, user_ref.clone(), now);
        node.context = context_messages
            .iter()
            .map(|message| {
                MessageRef::new(message.id.clone(), conversation_tree::message_hash(message))
            })
            .collect();
        node.reply = Some(Reply::pending(
            user_ref,
            Some(ParamsFingerprint {
                profile_id: profile_id.clone(),
                parameters: None,
                tools: Vec::new(),
            }),
            now,
        ));

        let mut digests = store::digests(&tree);
        let version = {
            let mut editor = TreeEditor::new(&mut tree, &mut digests, now);
            editor
                .append_turn(node, &parent)
                .map_err(super::tree_error)?
                .version
        };
        store::save_tree(&ctx, &tree).await?;
        store::record_session(
            &ctx,
            &request.session_id,
            Some(&profile_id),
            request.directory.clone(),
        )
        .await?;

        let turn_id = wf_common::generate_id();
        let cancel = CancellationToken::new();
        runtime.begin_turn(InflightTurn {
            turn_id: turn_id.clone(),
            node_id: node_id.clone(),
            cancel: cancel.clone(),
        })?;
        events::turn_started(&ctx, &request.session_id, &turn_id, &node_id, &parent);
        handle = TurnHandle {
            turn_id: turn_id.clone(),
            node_id: node_id.clone(),
            parent,
            version,
        };

        let driver = Driver {
            ctx: ctx.clone(),
            session_id: request.session_id.clone(),
            turn_id,
            node_id,
            profile_id,
            cancel,
            runtime: runtime.clone(),
        };
        tokio::spawn(async move { driver.run().await });
    }
    Ok(handle)
}

/// Cancel the in-flight turn of a session (or one specific turn).
pub async fn cancel_turn(session_id: &str, turn_id: Option<&str>) -> bool {
    let runtime = runtime::runtime_for(session_id);
    match runtime.cancel_token(turn_id) {
        Some(token) => {
            token.cancel();
            true
        }
        None => false,
    }
}

fn build_user_message(
    text: &str,
    images: &[wf_types::message::UserImageInput],
    now: i64,
) -> ApiResult<Message> {
    let content = wf_types::message::user_content(text, images)
        .ok_or_else(|| ApiError::Validation("chat turn requires a message".into()))?;
    Ok(Message {
        id: wf_common::generate_id(),
        role: MessageRole::User,
        content,
        timestamp: now,
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    })
}

/// Build and persist one text message of the session.
///
/// Sides of a turn are stored messages first and tree references second: the
/// node can only point at an immutable message, so the write happens before the
/// tree changes.
pub(crate) async fn new_text_message(
    ctx: &ApiContext,
    session_id: &str,
    role: MessageRole,
    text: impl Into<String>,
) -> ApiResult<Message> {
    let mut message = Message {
        id: wf_common::generate_id(),
        role,
        content: wf_types::message::MessageContentValue::Text(text.into()),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        thinking_signature: None,
        metadata: None,
    };
    let id = persist_message(ctx, session_id, message.clone()).await?;
    message.id = id;
    Ok(message)
}

/// Persist one message under the session anchor.
pub(crate) async fn persist_message(
    ctx: &ApiContext,
    session_id: &str,
    message: Message,
) -> ApiResult<String> {
    use wf_storage::adapter::base::BaseStorageAdapter;
    let record = crate::entity::message::record(session_id, Some(session_id), message);
    let id = record.id.clone();
    ctx.storage
        .message
        .save(&record)
        .await
        .map_err(ApiError::from)?;
    Ok(id)
}

/// One detached generation: prepare, stream, converge.
struct Driver {
    ctx: Arc<ApiContext>,
    session_id: String,
    turn_id: String,
    node_id: String,
    profile_id: String,
    cancel: CancellationToken,
    runtime: Arc<SessionRuntime>,
}

impl Driver {
    async fn run(self) {
        let answer = generate_answer(
            &self.ctx,
            &self.session_id,
            &self.turn_id,
            &self.node_id,
            &self.profile_id,
            &self.cancel,
            &self.runtime,
        )
        .await;
        converge_answer(
            &self.ctx,
            &self.session_id,
            &self.turn_id,
            &self.node_id,
            answer,
            &self.runtime,
        )
        .await;
        self.runtime.finish_turn(&self.turn_id);
    }
}

/// Assemble the request for a node and stream the answer.
///
/// Shared by the turn driver and the stepper: both regenerate one node from its
/// path, so both must build the request the same way.
pub(crate) async fn generate_answer(
    ctx: &ApiContext,
    session_id: &str,
    turn_id: &str,
    node_id: &str,
    profile_id: &str,
    cancel: &CancellationToken,
    runtime: &Arc<SessionRuntime>,
) -> GeneratedAnswer {
    let request = {
        let runtime = runtime.clone();
        let _guard = runtime.write().await;
        match prepare_request(ctx, session_id, node_id, profile_id).await {
            Ok(request) => request,
            Err(error) => return GeneratedAnswer::failed(error.to_string()),
        }
    };
    let mut stream = match ctx
        .llm_gateway
        .generate_stream(&request, Some(cancel.clone()))
        .await
    {
        Ok(stream) => stream,
        Err(error) => return GeneratedAnswer::failed(error.to_string()),
    };

    let mut answer = String::new();
    let mut usage: Option<TokenUsageStats> = None;
    let mut assistant: Option<Message> = None;
    let mut refusal: Option<String> = None;
    while let Some(event) = stream.next().await {
        match event {
            Ok(MessageStreamEvent::Text(text)) => {
                answer.push_str(&text.text);
                runtime.push_text(&turn_id, &text.text);
                events::turn_delta(&ctx, &session_id, &turn_id, &node_id, &text.text);
            }
            Ok(MessageStreamEvent::Stream(chunk)) => {
                answer.push_str(&chunk.content);
                runtime.push_text(&turn_id, &chunk.content);
                events::turn_delta(&ctx, &session_id, &turn_id, &node_id, &chunk.content);
            }
            Ok(MessageStreamEvent::ReasoningText(reasoning)) => {
                runtime.push_reasoning(&turn_id, &reasoning.reasoning);
                events::turn_reasoning(&ctx, &session_id, &turn_id, &node_id, &reasoning.reasoning);
            }
            Ok(MessageStreamEvent::Message(message)) => assistant = Some(message.message),
            Ok(MessageStreamEvent::FinalMessage(final_message)) => {
                if let Some(reported) = final_message.usage {
                    merge_usage(&mut usage, &reported);
                }
                assistant = Some(final_message.message);
            }
            Ok(MessageStreamEvent::Usage(reported)) => {
                merge_usage(&mut usage, &reported.usage);
                events::turn_usage(&ctx, &session_id, &turn_id, &node_id, &reported.usage);
            }
            Ok(MessageStreamEvent::Error(error)) => {
                return GeneratedAnswer::failed(error.error);
            }
            Ok(MessageStreamEvent::Refusal(refused)) => refusal = Some(refused.snapshot.clone()),
            Ok(MessageStreamEvent::Abort(abort)) => {
                let mut interrupted = GeneratedAnswer::interrupted();
                interrupted.error = Some(abort.reason);
                return interrupted;
            }
            Ok(MessageStreamEvent::RedactedThinking(_))
            | Ok(MessageStreamEvent::Annotation(_))
            | Ok(MessageStreamEvent::NativeBlock(_))
            | Ok(MessageStreamEvent::End(_))
            | Ok(MessageStreamEvent::Connect(_))
            | Ok(MessageStreamEvent::Start(_))
            | Ok(MessageStreamEvent::InputJson(_))
            | Ok(MessageStreamEvent::ToolCallDelta(_)) => {}
            Err(error) => return GeneratedAnswer::failed(error.to_string()),
        }
        if cancel.is_cancelled() {
            let mut interrupted = GeneratedAnswer::interrupted();
            interrupted.error = Some("cancelled".to_string());
            return interrupted;
        }
    }

    match assistant {
        Some(message) => GeneratedAnswer {
            message: Some(message),
            usage,
            state: ReplyState::Kept,
            error: None,
        },
        None => {
            // A stream that ends without a message still produced text in
            // most providers; keeping it as an interrupted answer is more
            // honest than dropping what the user already saw.
            let reason = refusal
                .filter(|refusal| !refusal.is_empty())
                .unwrap_or_else(|| "chat stream ended without a final message".to_string());
            if answer.trim().is_empty() {
                GeneratedAnswer::failed(reason)
            } else {
                GeneratedAnswer {
                    message: None,
                    usage,
                    state: ReplyState::Interrupted,
                    error: Some(reason),
                }
            }
        }
    }
}

/// Build the tool-free request for this node from the tree path.
///
/// The path is projected from the parent: the node's own user message is
/// restored explicitly, because it was already written when the turn was
/// submitted. Only `Kept` answers on the path enter, so a discarded or stale
/// branch never leaks into the request.
async fn prepare_request(
    ctx: &ApiContext,
    session_id: &str,
    node_id: &str,
    profile_id: &str,
) -> ApiResult<wf_types::llm::LlmRequest> {
    use wf_storage::adapter::base::BaseStorageAdapter;
    let tree = store::load_existing(ctx, session_id)
        .await?
        .ok_or_else(|| ApiError::not_found("conversation tree", &session_id))?;
    let node = tree
        .node(&node_id)
        .ok_or_else(|| ApiError::not_found("conversation node", &node_id))?;
    let mut session = ChatSession::new(profile_id.to_string());
    for projected in conversation_tree::project(&tree, &node.parent) {
        if let Some(record) = ctx
            .storage
            .message
            .load(&projected.message.id)
            .await
            .map_err(ApiError::from)?
        {
            session.restore(vec![record.message]);
        }
    }
    for context in &node.context {
        if let Some(record) = ctx
            .storage
            .message
            .load(&context.id)
            .await
            .map_err(ApiError::from)?
        {
            session.restore(vec![record.message]);
        }
    }
    if let Some(record) = ctx
        .storage
        .message
        .load(&node.user.id)
        .await
        .map_err(ApiError::from)?
    {
        session.restore(vec![record.message]);
    }
    session
        .continue_turn(Some(session_id))
        .map_err(|error| ApiError::Validation(error.to_string()))
}

/// Write the answer into the tree and tell subscribers how it ended.
pub(crate) async fn converge_answer(
    ctx: &ApiContext,
    session_id: &str,
    turn_id: &str,
    node_id: &str,
    mut answer: GeneratedAnswer,
    runtime: &Arc<SessionRuntime>,
) {
    let runtime = runtime.clone();
    let _guard = runtime.write().await;
    let Ok(Some(mut tree)) = store::load_existing(ctx, session_id).await else {
        events::turn_failed(
            &ctx,
            &session_id,
            &turn_id,
            &node_id,
            "session tree disappeared before the answer converged",
            "failed",
        );
        return;
    };

    // An interrupted or failed turn that still produced text keeps that
    // text as its body: the user saw it, so the tree must be able to
    // reproduce it.
    if answer.message.is_none() {
        let partial = runtime
            .inflight()
            .filter(|snapshot| snapshot.turn_id == turn_id)
            .map(|snapshot| snapshot.text)
            .unwrap_or_default();
        if !partial.trim().is_empty() {
            answer.message = Some(Message {
                id: wf_common::generate_id(),
                role: MessageRole::Assistant,
                content: wf_types::message::MessageContentValue::Text(partial),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                thinking_signature: None,
                metadata: None,
            });
        }
    }

    // The tree may only point at a stored message, so the body is written
    // before the reply is repointed — and a provider reply that arrived without
    // an id gets the one the store assigned.
    if let Some(message) = answer.message.clone() {
        match persist_message(ctx, session_id, message.clone()).await {
            Ok(id) => answer.message = Some(Message { id, ..message }),
            Err(error) => {
                tracing::warn!(
                    target: "wf_api::conversation",
                    session_id,
                    error = %error,
                    "failed to persist the answer"
                );
                answer.message = None;
                answer.state = ReplyState::Failed;
                answer.error = Some(error.to_string());
            }
        }
    }

    let now = wf_common::now();
    let mut digests = store::digests(&tree);
    let outcome = {
        let mut editor = TreeEditor::new(&mut tree, &mut digests, now);
        match answer.message.clone() {
            Some(message) => {
                let reference = MessageRef::new(
                    message.id.clone(),
                    conversation_tree::message_hash(&message),
                );
                editor
                    .finish_reply(&node_id, reference, Vec::new(), answer.state, None)
                    .map_err(super::tree_error)
            }
            None => editor
                .set_reply_state(&node_id, answer.state)
                .map_err(super::tree_error),
        }
    };

    match outcome {
        Ok(outcome) => {
            if let Err(error) = store::save_tree(&ctx, &tree).await {
                tracing::warn!(
                    target: "wf_api::conversation",
                    session_id = %session_id,
                    error = %error,
                    "failed to persist the converged tree"
                );
            }
            let dirty = conversation_tree::evaluate_dirty(&tree, &digests, None);
            events::dirty_changed(&ctx, &session_id, outcome.version, &dirty);
            events::tree_changed(
                &ctx,
                &session_id,
                outcome.version,
                "reply_settled",
                Some(&node_id),
            );
            match answer.state {
                ReplyState::Kept => {
                    events::turn_completed(
                        &ctx,
                        &session_id,
                        &turn_id,
                        &node_id,
                        answer
                            .message
                            .as_ref()
                            .map(|message| message.id.as_str())
                            .unwrap_or_default(),
                        answer.usage.as_ref(),
                    );
                    store::touch_session_status(
                        &ctx,
                        &session_id,
                        wf_types::ExecutionStatus::Completed,
                        None,
                    )
                    .await;
                }
                ReplyState::Interrupted => {
                    events::turn_failed(
                        &ctx,
                        &session_id,
                        &turn_id,
                        &node_id,
                        answer.error.as_deref().unwrap_or("cancelled"),
                        "cancelled",
                    );
                    store::touch_session_status(
                        &ctx,
                        &session_id,
                        wf_types::ExecutionStatus::Cancelled,
                        answer.error.clone(),
                    )
                    .await;
                }
                _ => {
                    events::turn_failed(
                        &ctx,
                        &session_id,
                        &turn_id,
                        &node_id,
                        answer.error.as_deref().unwrap_or("generation failed"),
                        "failed",
                    );
                    store::touch_session_status(
                        &ctx,
                        &session_id,
                        wf_types::ExecutionStatus::Failed,
                        answer.error.clone(),
                    )
                    .await;
                }
            }
        }
        Err(error) => {
            events::turn_failed(
                &ctx,
                &session_id,
                &turn_id,
                &node_id,
                &error.to_string(),
                "failed",
            );
        }
    }
}

/// Fold a reported usage block into the accumulated one, taking only the
/// non-zero fields of the later block (providers report either the whole
/// request or a delta of it).
pub(crate) fn merge_usage(total: &mut Option<TokenUsageStats>, reported: &TokenUsageStats) {
    match total {
        None => *total = Some(reported.clone()),
        Some(current) => {
            if reported.prompt_tokens > 0 {
                current.prompt_tokens = reported.prompt_tokens;
            }
            if reported.completion_tokens > 0 {
                current.completion_tokens = reported.completion_tokens;
            }
            if reported.total_tokens > 0 {
                current.total_tokens = reported.total_tokens;
            }
            current.reasoning_tokens = reported.reasoning_tokens.or(current.reasoning_tokens);
            current.cache_read_tokens = reported.cache_read_tokens.or(current.cache_read_tokens);
            current.cache_write_tokens = reported.cache_write_tokens.or(current.cache_write_tokens);
            current.prompt_tokens_cost = reported.prompt_tokens_cost.or(current.prompt_tokens_cost);
            current.completion_tokens_cost = reported
                .completion_tokens_cost
                .or(current.completion_tokens_cost);
            current.total_cost = reported.total_cost.or(current.total_cost);
        }
    }
}
