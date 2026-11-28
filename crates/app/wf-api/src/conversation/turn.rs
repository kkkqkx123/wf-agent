//! Submitting a turn and driving it to convergence.
//!
//! Submitting is a short write: the user node enters the tree, the reply is
//! `Pending`, the in-flight slot is claimed. Generation then runs detached from
//! any connection, publishing events as it goes; a client that disconnects
//! loses the stream, not the answer.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use wf_execution_shared::conversation_tree::{self, TreeEditor};
use wf_execution_shared::turn_accumulator::{AccumAction, TurnAccumulator};
use wf_execution_shared::ChatSession;
use wf_types::llm::TokenUsageStats;
use wf_types::message::{Message, MessageRole};
use wf_types::{ConversationNode, MessageRef, Reply, ReplyState};

use super::query::{current_fingerprint, session_allowed_tools, session_profile};
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
    /// Per-turn override of the session tool allow-list. Empty falls back to
    /// the session-level list; both empty keeps the plain streaming path.
    pub allowed_tools: Vec<String>,
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
    /// Prefix digest captured before generation. Convergence must not replace
    /// it with a later base, or a stale answer would look fresh.
    pub base: Option<String>,
    /// Ordered tool round-trip messages from a read-only tool round, archived
    /// as `reply.trace`. Empty for the plain streaming path.
    pub trace: Vec<Message>,
}

impl GeneratedAnswer {
    fn failed(error: impl Into<String>) -> Self {
        Self {
            message: None,
            usage: None,
            state: ReplyState::Failed,
            error: Some(error.into()),
            base: None,
            trace: Vec::new(),
        }
    }

    fn interrupted() -> Self {
        Self {
            message: None,
            usage: None,
            state: ReplyState::Interrupted,
            error: None,
            base: None,
            trace: Vec::new(),
        }
    }
}

/// Submit one turn: write the user node, claim the slot, start the driver.
pub async fn submit_turn(ctx: Arc<ApiContext>, request: TurnRequest) -> ApiResult<TurnHandle> {
    let text = request.message.trim().to_string();
    // Mentions touch other resources (files, definitions) and must not hold the
    // session lock while they resolve.
    let (mention_messages, mention_images, mention_receipts) =
        crate::agent::composition::resolve_mentions(&ctx, &text).await;
    let mut images = request.images;
    images.extend(mention_images);
    crate::infra::validation::validate_chat_input(&text, &images)?;

    super::require_chat_session(&ctx, &request.session_id).await?;
    let runtime = runtime::runtime_for(&request.session_id);
    let handle;
    {
        let _guard = runtime.write().await;
        // The check and the slot claim share the write lock with the tree
        // write, so a concurrent submit cannot leave an orphan pending node.
        if runtime.inflight().is_some() {
            return Err(ApiError::Conflict(
                "session is already generating a turn".into(),
            ));
        }
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

        // Effective allow-list: a per-turn override wins, else the session
        // config. Empty means the narrow driver stays off for this turn.
        let allowed_tools = if request.allowed_tools.is_empty() {
            session_allowed_tools(&ctx, &request.session_id).await
        } else {
            let mut tools = request.allowed_tools.clone();
            tools.sort();
            tools.dedup();
            tools
        };

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
        // Record the model-side parameters actually in effect so `model_drift`
        // compares like with like at snapshot time.
        let params = current_fingerprint(&ctx, &profile_id, &allowed_tools);
        node.reply = Some(Reply::pending(user_ref, params, now));

        let mut digests = store::digests(&tree);
        let version = {
            let mut editor = TreeEditor::new(&mut tree, &mut digests, now);
            editor
                .append_turn(node, &parent)
                .map_err(super::tree_error)?
                .version
        };

        // Claim the in-flight slot before the Pending node becomes durable.
        // `recover_pending` runs on reads without the write lock; if it saw a
        // persisted Pending node before the slot existed it would mark this
        // just-starting turn as Interrupted. Claiming first closes that window.
        let turn_id = wf_common::generate_id();
        let cancel = CancellationToken::new();
        runtime.begin_turn(InflightTurn {
            turn_id: turn_id.clone(),
            node_id: node_id.clone(),
            cancel: cancel.clone(),
        })?;
        if let Err(error) = store::save_tree(&ctx, &tree).await {
            runtime.finish_turn(&turn_id);
            return Err(error);
        }
        if let Err(error) = store::record_session(
            &ctx,
            &request.session_id,
            Some(&profile_id),
            request.directory.clone(),
            None,
        )
        .await
        {
            runtime.finish_turn(&turn_id);
            return Err(error);
        }
        events::turn_started(&ctx, &request.session_id, &turn_id, &node_id, &parent);
        handle = TurnHandle {
            turn_id: turn_id.clone(),
            node_id: node_id.clone(),
            parent,
            version,
        };

        let turn_id_for_task = turn_id.clone();
        let driver = Driver {
            ctx: ctx.clone(),
            session_id: request.session_id.clone(),
            turn_id,
            node_id,
            profile_id,
            allowed_tools,
            cancel,
            runtime: runtime.clone(),
        };
        let task = tokio::spawn(async move { driver.run().await });
        ctx.execution_tasks
            .register(turn_id_for_task.clone(), task.abort_handle());
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
    allowed_tools: Vec<String>,
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
            &self.allowed_tools,
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
        self.ctx.execution_tasks.unregister(&self.turn_id);
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
    allowed_tools: &[String],
    cancel: &CancellationToken,
    runtime: &Arc<SessionRuntime>,
) -> GeneratedAnswer {
    let (request, base) = {
        let runtime = runtime.clone();
        let _guard = runtime.write().await;
        match prepare_request(ctx, session_id, node_id, profile_id, allowed_tools).await {
            Ok(prepared) => prepared,
            Err(error) => return GeneratedAnswer::failed(error.to_string()),
        }
    };

    // A configured tool allow-list opts this turn into the narrow, non-streaming
    // read-only round. Without one the path is byte-for-byte the streaming path
    // below, so existing sessions are unaffected.
    if !allowed_tools.is_empty() {
        return generate_readonly_answer(
            ctx, session_id, turn_id, node_id, &request, &base, allowed_tools, cancel,
        )
        .await;
    }

    let mut stream = match ctx
        .llm_gateway
        .generate_stream(&request, Some(cancel.clone()))
        .await
    {
        Ok(stream) => stream,
        Err(error) => return GeneratedAnswer::failed(error.to_string()),
    };

    // Event classification and text/usage folding are shared with the chat
    // execution chain. This sink writes partial text into the runtime slot and
    // publishes turn events, and keeps the per-event cancellation check that
    // the detached driver needs.
    let mut acc = TurnAccumulator::new();
    while let Some(event) = stream.next().await {
        let action = match event {
            Ok(event) => acc.consume(event),
            Err(error) => return GeneratedAnswer::failed(error.to_string()),
        };
        match action {
            AccumAction::Text(delta) => {
                runtime.push_text(turn_id, &delta);
                events::turn_delta(ctx, session_id, turn_id, node_id, &delta);
            }
            AccumAction::Reasoning(delta) => {
                runtime.push_reasoning(turn_id, &delta);
                events::turn_reasoning(ctx, session_id, turn_id, node_id, &delta);
            }
            AccumAction::Usage(reported) => {
                events::turn_usage(ctx, session_id, turn_id, node_id, &reported);
            }
            AccumAction::Failed(error) => return GeneratedAnswer::failed(error),
            AccumAction::Interrupted(reason) => {
                let mut interrupted = GeneratedAnswer::interrupted();
                interrupted.error = Some(reason);
                return interrupted;
            }
            AccumAction::None => {}
        }
        if cancel.is_cancelled() {
            let mut interrupted = GeneratedAnswer::interrupted();
            interrupted.error = Some("cancelled".to_string());
            return interrupted;
        }
    }

    match acc.assistant {
        Some(message) => GeneratedAnswer {
            message: Some(message),
            usage: acc.usage,
            state: ReplyState::Kept,
            error: None,
            base: Some(base),
            trace: Vec::new(),
        },
        None => {
            // A stream that ends without a message still produced text in
            // most providers; keeping it as an interrupted answer is more
            // honest than dropping what the user already saw.
            let reason = acc.missing_message_reason("chat stream ended without a final message");
            if acc.answer.trim().is_empty() {
                GeneratedAnswer::failed(reason)
            } else {
                GeneratedAnswer {
                    message: None,
                    usage: acc.usage,
                    state: ReplyState::Interrupted,
                    error: Some(reason),
                    base: Some(base),
                    trace: Vec::new(),
                }
            }
        }
    }
}

/// Drive one non-streaming read-only tool round.
///
/// The narrow driver may answer in plain text or execute a bounded set of
/// allow-listed tools. Since it is not streaming, the answer text is published
/// as a single `turn_delta` and each executed tool as a `turn_tool` frame; the
/// round-trip messages are carried back for `reply.trace` archival.
async fn generate_readonly_answer(
    ctx: &ApiContext,
    session_id: &str,
    turn_id: &str,
    node_id: &str,
    request: &wf_types::llm::LlmRequest,
    base: &str,
    allowed_tools: &[String],
    cancel: &CancellationToken,
) -> GeneratedAnswer {
    if cancel.is_cancelled() {
        let mut interrupted = GeneratedAnswer::interrupted();
        interrupted.error = Some("cancelled".to_string());
        return interrupted;
    }

    let outcome = match wf_execution_shared::generate_readonly_turn(
        &ctx.llm_gateway,
        &ctx.tool_registry,
        request,
        allowed_tools,
        None,
        turn_id,
        Some(cancel.clone()),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => return GeneratedAnswer::failed(error.to_string()),
    };

    if cancel.is_cancelled() {
        let mut interrupted = GeneratedAnswer::interrupted();
        interrupted.error = Some("cancelled".to_string());
        return interrupted;
    }

    // Publish one tool frame per execution so subscribers see what ran.
    for execution in &outcome.executions {
        events::turn_tool(
            ctx,
            session_id,
            turn_id,
            node_id,
            &execution.tool_name,
            &execution.tool_call_id,
            execution.result.success,
        );
    }

    let usage = outcome.generation.usage.clone();
    if let Some(usage) = &usage {
        events::turn_usage(ctx, session_id, turn_id, node_id, usage);
    }

    // A tool round returns the assistant tool_calls message as `generation`,
    // and the full round-trip as `trace` (whose first entry is that same
    // assistant message). Keeping both as reply body + trace would inject the
    // tool_calls message twice into projection. Instead: with no tools the
    // model text is the answer; with tools the answer body is the tool results
    // folded into one assistant message, while `trace` keeps the ordered
    // round-trip for digest folding and context replay.
    let message = if outcome.trace.is_empty() {
        outcome.generation.message
    } else {
        let text = outcome
            .trace
            .iter()
            .filter(|m| m.role == MessageRole::Tool)
            .map(|m| m.text_content())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        Message::text(MessageRole::Assistant, text)
    };
    // Non-streaming: deliver the whole answer in one delta so SSE clients that
    // only render `turn.delta` still show it.
    if let wf_types::message::MessageContentValue::Text(text) = &message.content {
        if !text.is_empty() {
            events::turn_delta(ctx, session_id, turn_id, node_id, text);
        }
    }

    GeneratedAnswer {
        message: Some(message),
        usage,
        state: ReplyState::Kept,
        error: None,
        base: Some(base.to_string()),
        trace: outcome.trace,
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
    allowed_tools: &[String],
) -> ApiResult<(wf_types::llm::LlmRequest, String)> {
    use wf_storage::adapter::base::BaseStorageAdapter;
    let tree = store::load_existing(ctx, session_id)
        .await?
        .ok_or_else(|| ApiError::not_found("conversation tree", session_id))?;
    let node = tree
        .node(node_id)
        .ok_or_else(|| ApiError::not_found("conversation node", node_id))?;
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
    let base = store::digests(&tree)
        .base(node_id)
        .map(str::to_string)
        .ok_or_else(|| ApiError::not_found("conversation node", node_id))?;
    let mut request = session
        .continue_turn(Some(session_id))
        .map_err(|error| ApiError::Validation(error.to_string()))?;

    // Declare exactly the allow-listed tools to the provider. `ChatSession`
    // stays tool-free; declarations are attached to this one request only.
    // The narrow driver re-validates every emitted call against the same list.
    if !allowed_tools.is_empty() {
        let mut declarations = Vec::with_capacity(allowed_tools.len());
        for name in allowed_tools {
            if let Some(tool) = ctx.tool_registry.get_tool(name) {
                declarations.push(tool.wire_declaration());
            }
        }
        if !declarations.is_empty() {
            request.tools = Some(declarations);
        }
    }
    Ok((request, base))
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
            ctx,
            session_id,
            turn_id,
            node_id,
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

    // Tool round-trip messages are immutable and content-addressed, so each is
    // stored first and then referenced by the reply trace. A store failure for
    // one trace message drops that message rather than failing the whole turn:
    // trace is advisory context, not the answer body.
    let mut trace_refs: Vec<MessageRef> = Vec::with_capacity(answer.trace.len());
    for message in std::mem::take(&mut answer.trace) {
        match persist_message(ctx, session_id, message.clone()).await {
            Ok(id) => trace_refs.push(MessageRef::new(
                id,
                conversation_tree::message_hash(&message),
            )),
            Err(error) => tracing::warn!(
                target: "wf_api::conversation",
                session_id,
                error = %error,
                "failed to persist a reply trace message"
            ),
        }
    }

    let now = wf_common::now();
    let mut digests = store::digests(&tree);
    let current_base = digests
        .base(node_id)
        .map(str::to_string)
        .unwrap_or_default();
    let based_on = answer.base.clone().unwrap_or_else(|| current_base.clone());
    if based_on != current_base {
        tracing::warn!(
            target: "wf_api::conversation",
            session_id = %session_id,
            node_id = %node_id,
            "tree changed while the answer was generating; keeping it stale"
        );
    }
    let outcome = {
        let mut editor = TreeEditor::new(&mut tree, &mut digests, now);
        match answer.message.clone() {
            Some(message) => {
                let reference = MessageRef::new(
                    message.id.clone(),
                    conversation_tree::message_hash(&message),
                );
                // `None` keeps the params captured at submit time (profile +
                // effective tool allow-list), so `model_drift` compares like
                // with like even if config changed mid-generation.
                editor
                    .finish_reply(
                        node_id,
                        reference,
                        trace_refs,
                        answer.state,
                        None,
                        &based_on,
                    )
                    .map_err(super::tree_error)
            }
            None => editor
                .set_reply_state(node_id, answer.state)
                .map_err(super::tree_error),
        }
    };

    match outcome {
        Ok(outcome) => {
            if let Err(error) = store::save_tree(ctx, &tree).await {
                tracing::warn!(
                    target: "wf_api::conversation",
                    session_id = %session_id,
                    error = %error,
                    "failed to persist the converged tree"
                );
            }
            let dirty = conversation_tree::evaluate_dirty(&tree, &digests, None);
            events::dirty_changed(ctx, session_id, outcome.version, &dirty);
            events::tree_changed(
                ctx,
                session_id,
                outcome.version,
                "reply_settled",
                Some(node_id),
            );
            match answer.state {
                ReplyState::Kept => {
                    events::turn_completed(
                        ctx,
                        session_id,
                        turn_id,
                        node_id,
                        answer
                            .message
                            .as_ref()
                            .map(|message| message.id.as_str())
                            .unwrap_or_default(),
                        answer.usage.as_ref(),
                    );
                    store::touch_session_status(
                        ctx,
                        session_id,
                        wf_types::ExecutionStatus::Completed,
                        None,
                    )
                    .await;
                }
                ReplyState::Interrupted => {
                    events::turn_failed(
                        ctx,
                        session_id,
                        turn_id,
                        node_id,
                        answer.error.as_deref().unwrap_or("cancelled"),
                        "cancelled",
                    );
                    store::touch_session_status(
                        ctx,
                        session_id,
                        wf_types::ExecutionStatus::Cancelled,
                        answer.error.clone(),
                    )
                    .await;
                }
                _ => {
                    events::turn_failed(
                        ctx,
                        session_id,
                        turn_id,
                        node_id,
                        answer.error.as_deref().unwrap_or("generation failed"),
                        "failed",
                    );
                    store::touch_session_status(
                        ctx,
                        session_id,
                        wf_types::ExecutionStatus::Failed,
                        answer.error.clone(),
                    )
                    .await;
                }
            }
        }
        Err(error) => {
            events::turn_failed(
                ctx,
                session_id,
                turn_id,
                node_id,
                &error.to_string(),
                "failed",
            );
        }
    }
}
