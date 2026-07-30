use wf_types::trigger::{TriggerAction, TriggerCondition, TriggerTemplate};
use wf_types::Metadata;

use crate::registrar::{register_item, Options, Registries};
use crate::result::Summary;

pub const CONTEXT_COMPRESSION_TRIGGER_NAME: &str = "context_compression_trigger";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub fn create_context_compression_trigger(triggered_workflow_id: Option<String>) -> TriggerTemplate {
    let t = now_ms();
    TriggerTemplate {
        name: CONTEXT_COMPRESSION_TRIGGER_NAME.into(),
        description: Some(
            "Automatically triggers a context compression sub-workflow when Token usage exceeds the limit".into(),
        ),
        condition: Some(TriggerCondition {
            event_type: "CONTEXT_COMPRESSION_REQUESTED".into(),
            event_name: None,
            condition: None,
            metadata: None,
        }),
        action: Some(TriggerAction::ExecuteTriggeredSubworkflow {
            triggered_workflow_id: triggered_workflow_id.unwrap_or_else(|| "llm_summary_workflow".into()),
            wait_for_completion: Some(true),
            timeout: Some(60000),
            input_mapping: None,
            output_mapping: None,
        }),
        enabled: Some(true),
        max_triggers: Some(0),
        metadata: Some(Metadata::from_iter([
            ("category".to_string(), serde_json::Value::String("system".into())),
            ("tags".to_string(), serde_json::Value::Array(vec![
                serde_json::Value::String("context".into()),
                serde_json::Value::String("compression".into()),
                serde_json::Value::String("token".into()),
                serde_json::Value::String("memory".into()),
            ])),
        ])),
        created_at: t,
        updated_at: t,
        create_checkpoint: None,
        checkpoint_description_template: None,
    }
}

pub fn register(regs: &Registries, opts: &Options) -> Summary {
    let trigger = create_context_compression_trigger(None);
    let key = trigger.name.clone();
    register_item(&regs.trigger_templates, key, trigger, opts.skip_if_exists)
}
