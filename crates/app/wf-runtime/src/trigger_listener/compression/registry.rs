use std::sync::Arc;

use wf_types::trigger::{TriggerAction, TriggerCondition, TriggerTemplate};
use wf_workflow::trigger::TriggerTemplateRegistry;

/// Name of the builtin compression template: the only template whose action
/// is the reserved `ExecuteContextCompression`. User configuration can never
/// carry that action (rejected at load time) or target the signal (rejected
/// at load time and skipped by the matcher), so this name never competes.
pub const BUILTIN_COMPRESSION_TEMPLATE_NAME: &str = "builtin-context-compression";

/// The builtin compression template: matches only the adapter-published
/// routed copy (`CONTEXT_COMPRESSION_REQUESTED` with the routed marker) and
/// runs it through the reserved compression action.
///
/// Unlimited budget by construction: per-version idempotency is owned by the
/// pipeline claim (one (target, version) runs once), and the listener keys
/// the builtin action below the template so concurrent versions of one
/// execution do not share an in-flight slot.
pub(crate) fn builtin_compression_template() -> TriggerTemplate {
    let mut metadata = wf_types::Metadata::new();
    metadata.insert(
        wf_execution_shared::KEY_COMPRESSION_ROUTED.to_string(),
        serde_json::json!(true),
    );
    TriggerTemplate {
        name: BUILTIN_COMPRESSION_TEMPLATE_NAME.to_string(),
        description: Some(
            "Builtin context-compression route: adapter-published handoffs only.".to_string(),
        ),
        condition: Some(TriggerCondition {
            event_type: wf_types::hook::CONTEXT_COMPRESSION_SIGNAL.to_string(),
            event_name: None,
            condition: None,
            metadata: Some(metadata),
            metadata_exists: None,
            execution_prefix: None,
        }),
        action: Some(TriggerAction::ExecuteContextCompression {}),
        enabled: None,
        max_triggers: None,
        priority: None,
        dispatch_mode: None,
        allow_multi_effect: None,
        effect_order: None,
        metadata: None,
        created_at: wf_common::now(),
        updated_at: wf_common::now(),
        create_checkpoint: None,
        checkpoint_description_template: None,
    }
}

/// Listener registry chaining the user templates with the builtin
/// compression template. Read-time chaining (no registry mutation): the
/// builtin route is armed exactly when the listener is built with it, and
/// user registries never contain a compression template.
pub(crate) struct CompressionRoutedRegistry {
    inner: Arc<dyn TriggerTemplateRegistry>,
}

impl CompressionRoutedRegistry {
    pub(crate) fn new(inner: Arc<dyn TriggerTemplateRegistry>) -> Self {
        Self { inner }
    }
}

impl TriggerTemplateRegistry for CompressionRoutedRegistry {
    fn templates(&self) -> Vec<TriggerTemplate> {
        let mut templates = self.inner.templates();
        templates.push(builtin_compression_template());
        templates
    }
}
