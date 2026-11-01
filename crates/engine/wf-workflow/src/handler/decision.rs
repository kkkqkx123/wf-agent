use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_llm::DecisionProvider;
use wf_types::events::{BaseEvent, EventType};
use wf_types::node::configs::{DecisionImageAsset, DecisionNodeConfig};
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;
use crate::variable::VariableResolver;

pub struct DecisionHandler {
    decision_registry: Option<wf_llm::SharedDecisionProfileRegistry>,
}

impl DecisionHandler {
    pub fn new() -> Self {
        Self {
            decision_registry: None,
        }
    }

    pub fn with_registry(registry: wf_llm::SharedDecisionProfileRegistry) -> Self {
        Self {
            decision_registry: Some(registry),
        }
    }

    pub fn with_registry_opt(
        mut self,
        registry: Option<wf_llm::SharedDecisionProfileRegistry>,
    ) -> Self {
        self.decision_registry = registry;
        self
    }
}

impl Default for DecisionHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NodeHandler for DecisionHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::Decision
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

struct ResolvedConnection {
    protocol: wf_llm::DecisionProtocol,
    base_url: String,
    model: String,
    api_key: Option<String>,
    timeout_secs: u64,
}

impl DecisionHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let raw = ctx.node_config.as_ref().unwrap_or(&Value::Null).clone();
        let config: DecisionNodeConfig = serde_json::from_value(raw).map_err(|e| {
            WorkflowError::OperationError(format!(
                "DECISION node '{}' has an invalid config: {e}",
                ctx.node_id
            ))
        })?;
        if config.questions.is_empty() || config.questions.len() > 400 {
            return Err(WorkflowError::OperationError(format!(
                "DECISION node '{}' must define 1 to 400 questions",
                ctx.node_id
            )));
        }

        let connection = self.resolve_connection(ctx, &config)?;
        let (state, assets) = resolve_state(ctx, &config)?;
        let questions = build_questions(ctx, &config)?;

        let provider = wf_llm::AnyDecisionProvider::from_config(wf_llm::DecisionProviderConfig {
            protocol: connection.protocol,
            base_url: connection.base_url,
            model: connection.model.clone(),
            api_key: connection.api_key,
            timeout_secs: connection.timeout_secs,
            headers: HashMap::new(),
            query_params: HashMap::new(),
        })
        .map_err(|e| WorkflowError::NodeFailure {
            node_id: ctx.node_id.clone(),
            category: wf_types::workflow::error_branch::NodeErrorCategory::BusinessFailure,
            detail: format!("decision provider setup failed: {e}"),
            failure_source: wf_types::workflow::error_branch::NodeFailureSource::Handler,
        })?;

        let request = wf_llm::DecisionRequest {
            model: Some(connection.model.clone()),
            state,
            questions,
            safety_identifier: resolve_field(
                &config.safety_identifier,
                &ctx.variables,
                Some(&ctx.system_context()),
            ),
            assets,
        };
        let response = provider
            .create(&request)
            .await
            .map_err(|e| WorkflowError::NodeFailure {
                node_id: ctx.node_id.clone(),
                category: wf_types::workflow::error_branch::NodeErrorCategory::BusinessFailure,
                detail: format!("decision request failed: {e}"),
                failure_source: wf_types::workflow::error_branch::NodeFailureSource::Handler,
            })?;

        let output = serde_json::json!({
            "model": response.model,
            "answers": response.answers,
        });
        if let Some(variable) = config.output_variable.as_deref() {
            if !variable.trim().is_empty() {
                ctx.set_variable(variable.to_string(), output.clone())?;
            }
        }

        let mut metadata = HashMap::new();
        {
            use wf_execution_shared::context::node_metadata as keys;
            metadata.insert(
                keys::MODEL.to_string(),
                Value::String(response.model.clone()),
            );
            metadata.insert(
                keys::EVENT.to_string(),
                Value::String("decision_node_completed".to_string()),
            );
            metadata.insert(
                "answer_count".to_string(),
                Value::Number((response.answers.len() as u64).into()),
            );
            if let Some(input_tokens) = response.usage.input_tokens {
                metadata.insert(
                    keys::PROMPT_TOKENS.to_string(),
                    Value::Number(input_tokens.into()),
                );
            }
            if let Some(output_tokens) = response.usage.output_tokens {
                metadata.insert(
                    keys::COMPLETION_TOKENS.to_string(),
                    Value::Number(output_tokens.into()),
                );
            }
        }
        emit_decision_event(ctx, metadata.clone());

        let total_tokens = response.usage.total_tokens.unwrap_or_else(|| {
            response.usage.input_tokens.unwrap_or(0) + response.usage.output_tokens.unwrap_or(0)
        });
        if total_tokens > 0 {
            if let Some(metrics) = ctx.metrics.as_ref() {
                metrics.node().record_token_usage(
                    &ctx.node_id,
                    ctx.node_type.canonical_name(),
                    total_tokens,
                );
            }
        }

        Ok(NodeExecutionResult {
            output,
            next_node_ids: Vec::new(),
            metadata,
        })
    }

    fn resolve_connection(
        &self,
        ctx: &NodeExecutionContext,
        config: &DecisionNodeConfig,
    ) -> WorkflowResult<ResolvedConnection> {
        let system = ctx.system_context();
        if let Some(profile_id) = config
            .decision_profile_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let registry = self.decision_registry.as_ref().ok_or_else(|| {
                WorkflowError::OperationError(format!(
                    "DECISION node '{}' references decision profile '{profile_id}' but no decision registry is wired",
                    ctx.node_id
                ))
            })?;
            let api_key = resolve_field(&config.api_key, &ctx.variables, Some(&system))
                .filter(|s| !s.is_empty());
            let provider_config =
                registry
                    .provider_config_for(profile_id, api_key)
                    .map_err(|e| {
                        WorkflowError::OperationError(format!(
                            "DECISION node '{}': {e}",
                            ctx.node_id
                        ))
                    })?;
            return Ok(ResolvedConnection {
                protocol: provider_config.protocol,
                base_url: provider_config.base_url,
                model: provider_config.model,
                api_key: provider_config.api_key,
                timeout_secs: provider_config.timeout_secs,
            });
        }

        let protocol_text = config
            .protocol
            .as_deref()
            .unwrap_or("openai")
            .trim()
            .to_string();
        let Some(protocol) = wf_llm::DecisionProtocol::parse(&protocol_text) else {
            return Err(WorkflowError::OperationError(format!(
                "DECISION node '{}' has unknown protocol '{protocol_text}'; expected openai or jev",
                ctx.node_id
            )));
        };
        let base_url = resolve_field(&config.base_url, &ctx.variables, Some(&system))
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                WorkflowError::OperationError(format!(
                    "DECISION node '{}' requires decision_profile_id or inline base_url and model",
                    ctx.node_id
                ))
            })?;
        let model = resolve_field(&config.model, &ctx.variables, Some(&system))
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                WorkflowError::OperationError(format!(
                    "DECISION node '{}' requires decision_profile_id or inline base_url and model",
                    ctx.node_id
                ))
            })?;
        let api_key =
            resolve_field(&config.api_key, &ctx.variables, Some(&system)).filter(|s| !s.is_empty());
        let timeout_secs = config.timeout_secs.unwrap_or(30);
        if timeout_secs == 0 {
            return Err(WorkflowError::OperationError(format!(
                "DECISION node '{}' has timeout_secs 0; it must be >= 1",
                ctx.node_id
            )));
        }
        Ok(ResolvedConnection {
            protocol,
            base_url,
            model,
            api_key,
            timeout_secs,
        })
    }
}

fn resolve_field(
    value: &Option<String>,
    variables: &crate::variable::VariableStore,
    system: Option<&wf_execution_shared::context::SystemContext>,
) -> Option<String> {
    let raw = value.as_ref()?;
    match VariableResolver::resolve_str(raw, variables, system) {
        Value::String(s) => Some(s),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn resolve_state(
    ctx: &NodeExecutionContext,
    config: &DecisionNodeConfig,
) -> WorkflowResult<(
    wf_llm::DecisionState,
    Option<BTreeMap<String, wf_llm::ImageAsset>>,
)> {
    let assets = resolve_assets(ctx, &config.image_assets)?;
    if let Some(state_json) = config.state_json.as_ref() {
        if !state_json.is_null() {
            return Ok((wf_llm::DecisionState::Json(state_json.clone()), assets));
        }
    }
    if let Some(messages) = config.state_messages.as_ref() {
        if !messages.is_null() {
            let parsed: Vec<wf_llm::DecisionMessage> = serde_json::from_value(messages.clone())
                .map_err(|e| {
                    WorkflowError::OperationError(format!(
                        "DECISION node '{}' has invalid state_messages: {e}",
                        ctx.node_id
                    ))
                })?;
            return Ok((wf_llm::DecisionState::Messages(parsed), assets));
        }
    }
    if let Some(parts) = config.state_parts.as_ref() {
        if !parts.is_null() {
            let parsed: Vec<wf_llm::ContentPart> =
                serde_json::from_value(parts.clone()).map_err(|e| {
                    WorkflowError::OperationError(format!(
                        "DECISION node '{}' has invalid state_parts: {e}",
                        ctx.node_id
                    ))
                })?;
            return Ok((wf_llm::DecisionState::Parts(parsed), assets));
        }
    }
    if let Some(name) = config.state_variable.as_deref() {
        if !name.trim().is_empty() {
            match ctx.get_variable(name) {
                Some(Value::String(s)) => {
                    return Ok((wf_llm::DecisionState::Text(s), assets));
                }
                Some(Value::Null) => {
                    return Ok((wf_llm::DecisionState::Text(String::new()), assets));
                }
                Some(other) => {
                    return Ok((wf_llm::DecisionState::Text(other.to_string()), assets));
                }
                None => {
                    return Err(WorkflowError::OperationError(format!(
                        "DECISION node '{}' reads unknown variable '{name}'",
                        ctx.node_id
                    )));
                }
            }
        }
    }
    if let Some(expression) = config.state_expression.as_deref() {
        if !expression.trim().is_empty() {
            let system = ctx.system_context();
            match VariableResolver::resolve_str(expression, &ctx.variables, Some(&system)) {
                Value::String(s) => return Ok((wf_llm::DecisionState::Text(s), assets)),
                Value::Null => return Ok((wf_llm::DecisionState::Text(String::new()), assets)),
                other => return Ok((wf_llm::DecisionState::Text(other.to_string()), assets)),
            }
        }
    }
    match &ctx.input {
        Value::String(s) => Ok((wf_llm::DecisionState::Text(s.clone()), assets)),
        Value::Null => Ok((wf_llm::DecisionState::Text(String::new()), assets)),
        other => Ok((wf_llm::DecisionState::Text(other.to_string()), assets)),
    }
}

fn resolve_assets(
    ctx: &NodeExecutionContext,
    assets: &[DecisionImageAsset],
) -> WorkflowResult<Option<BTreeMap<String, wf_llm::ImageAsset>>> {
    if assets.is_empty() {
        return Ok(None);
    }
    let mut output = BTreeMap::new();
    for asset in assets {
        let alias = asset.alias.trim();
        if alias.is_empty() {
            return Err(WorkflowError::OperationError(format!(
                "DECISION node '{}' has an image asset with an empty alias",
                ctx.node_id
            )));
        }
        let bytes = match (&asset.base64, &asset.variable) {
            (Some(inline), None) if !inline.trim().is_empty() => {
                let cleaned: String = inline.split_whitespace().collect();
                use base64::Engine as _;
                base64::engine::general_purpose::STANDARD
                    .decode(&cleaned)
                    .map_err(|e| {
                        WorkflowError::OperationError(format!(
                            "DECISION node '{}' image asset '{alias}' has invalid base64: {e}",
                            ctx.node_id
                        ))
                    })?
            }
            (None, Some(name)) if !name.trim().is_empty() => match ctx.get_variable(name) {
                Some(Value::String(text)) => {
                    let cleaned: String = text.split_whitespace().collect();
                    use base64::Engine as _;
                    base64::engine::general_purpose::STANDARD
                        .decode(&cleaned)
                        .map_err(|e| {
                            WorkflowError::OperationError(format!(
                                "DECISION node '{}' image asset '{alias}' has invalid base64: {e}",
                                ctx.node_id
                            ))
                        })?
                }
                Some(Value::Array(items)) => {
                    let mut bytes = Vec::with_capacity(items.len());
                    for item in items {
                        match item.as_u64() {
                            Some(byte) if byte <= 255 => bytes.push(byte as u8),
                            _ => {
                                return Err(WorkflowError::OperationError(format!(
                                    "DECISION node '{}' image asset '{alias}' variable must hold base64 text or byte values",
                                    ctx.node_id
                                )));
                            }
                        }
                    }
                    bytes
                }
                Some(_) => {
                    return Err(WorkflowError::OperationError(format!(
                        "DECISION node '{}' image asset '{alias}' variable must hold base64 text or byte values",
                        ctx.node_id
                    )));
                }
                None => {
                    return Err(WorkflowError::OperationError(format!(
                        "DECISION node '{}' image asset '{alias}' reads unknown variable '{name}'",
                        ctx.node_id
                    )));
                }
            },
            _ => {
                return Err(WorkflowError::OperationError(format!(
                    "DECISION node '{}' image asset '{alias}' must define exactly one of base64 or variable",
                    ctx.node_id
                )));
            }
        };
        if bytes.is_empty() {
            return Err(WorkflowError::OperationError(format!(
                "DECISION node '{}' image asset '{alias}' is empty",
                ctx.node_id
            )));
        }
        output.insert(alias.to_string(), wf_llm::ImageAsset::from_bytes(bytes));
    }
    Ok(Some(output))
}

fn build_questions(
    ctx: &NodeExecutionContext,
    config: &DecisionNodeConfig,
) -> WorkflowResult<Vec<wf_llm::NamedQuestion>> {
    let mut output = Vec::with_capacity(config.questions.len());
    for question in &config.questions {
        let name = question.name.clone();
        let node_question = match question.question_type {
            wf_types::node::configs::DecisionQuestionKind::Predicate => {
                wf_llm::DecisionQuestion::Predicate {
                    name: name.clone(),
                    instructions: question.instructions.clone(),
                    true_description: question.true_description.clone(),
                    false_description: question.false_description.clone(),
                }
            }
            wf_types::node::configs::DecisionQuestionKind::Choice => {
                let choices = question.choices.as_ref().cloned().unwrap_or_default();
                if choices.len() < 2 || choices.len() > 255 {
                    return Err(WorkflowError::OperationError(format!(
                        "DECISION node '{}' choice question requires 2 to 255 options",
                        ctx.node_id
                    )));
                }
                let mut mapped = Vec::with_capacity(choices.len());
                for choice in choices {
                    let value = match choice.value {
                        Value::String(s) => wf_llm::ChoiceValue::Text(s),
                        Value::Bool(b) => wf_llm::ChoiceValue::Bool(b),
                        Value::Number(ref n) => wf_llm::ChoiceValue::Text(n.to_string()),
                        _ => {
                            return Err(WorkflowError::OperationError(format!(
                                "DECISION node '{}' choice values must be strings, booleans or numbers",
                                ctx.node_id
                            )));
                        }
                    };
                    mapped.push(wf_llm::llm_decision::types::ChoiceOption {
                        value,
                        description: choice.description,
                    });
                }
                wf_llm::DecisionQuestion::Choice {
                    name: name.clone(),
                    instructions: question.instructions.clone(),
                    choices: mapped,
                }
            }
            wf_types::node::configs::DecisionQuestionKind::Score => {
                let levels = question.levels.as_ref().cloned().unwrap_or_default();
                if levels.is_empty() {
                    return Err(WorkflowError::OperationError(format!(
                        "DECISION node '{}' score question requires at least one level",
                        ctx.node_id
                    )));
                }
                wf_llm::DecisionQuestion::Score {
                    name: name.clone(),
                    instructions: question.instructions.clone(),
                    levels: levels
                        .into_iter()
                        .map(|level| wf_llm::ScoreLevel {
                            label: level.label,
                            description: level.description,
                        })
                        .collect(),
                }
            }
            wf_types::node::configs::DecisionQuestionKind::Box => wf_llm::DecisionQuestion::Box {
                name: name.clone(),
                instructions: question.instructions.clone(),
            },
        };
        output.push(wf_llm::NamedQuestion {
            name,
            question: node_question,
        });
    }
    Ok(output)
}

fn emit_decision_event(ctx: &NodeExecutionContext, metadata: HashMap<String, Value>) {
    let Some(bus) = ctx.event_bus.as_deref() else {
        return;
    };
    bus.publish_logged(
        BaseEvent {
            id: wf_types::Id::new(),
            r#type: EventType::NodeCustomEvent,
            timestamp: wf_common::now(),
            workflow_id: Some(ctx.execution_id.clone()),
            execution_id: Some(ctx.execution_id.clone()),
            agent_loop_id: None,
            event_name: None,
            metadata: Some(metadata),
        },
        &format!("workflow={} decision={}", ctx.execution_id, ctx.node_id),
    )
    .ok();
}
