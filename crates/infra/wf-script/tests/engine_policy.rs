use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use wf_script::{
    ArgumentValueSource, ExecutorMode, InteractionMode, InteractiveScriptConfig, ScriptArgument,
    ScriptArgumentType, ScriptDefinition, ScriptEngine, ScriptEngineOptions,
    ScriptExecutionOptions, ScriptExecutionResult, ScriptRiskLevel, ScriptSecurityPolicy,
};

fn content_script(name: &str, content: &str) -> ScriptDefinition {
    ScriptDefinition {
        name: name.to_string(),
        content: Some(content.to_string()),
        template: None,
        arguments: None,
        language: None,
        executor_mode: None,
        interactive: None,
        security_policy: None,
        description: None,
        enabled: None,
    }
}

fn ok_transport(
    script_name: &str,
) -> impl Fn(String, Option<ScriptExecutionOptions>) -> futures::future::Ready<ScriptExecutionResult>
{
    let name = script_name.to_string();
    move |cmd, _opts| {
        futures::future::ready(ScriptExecutionResult {
            success: true,
            script_name: name.clone(),
            stdout: Some(cmd),
            stderr: None,
            exit_code: Some(0),
            execution_time_ms: 0,
            error: None,
            requires_review: false,
            truncated: false,
            output_bytes: None,
            stdout_path: None,
            stderr_path: None,
        })
    }
}

fn policy_template(allowed: Option<Vec<String>>) -> ScriptSecurityPolicy {
    let _ = allowed;
    ScriptSecurityPolicy {
        max_risk_level: None,
        require_review: None,
        allowed_languages: None,
        blocked_patterns: None,
        forbidden_commands: None,
        forbidden_path_patterns: None,
        max_script_size: None,
        allow_dynamic_scripts: None,
    }
}

#[tokio::test]
async fn definition_shape_both_content_and_template_rejected() {
    let script = ScriptDefinition {
        content: Some("echo hi".to_string()),
        template: Some("echo {{x}}".to_string()),
        ..content_script("both", "echo hi")
    };
    let result = ScriptEngine
        .execute(
            &script,
            None,
            &ScriptEngineOptions::default(),
            ok_transport("both"),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap_or_default().contains("exactly one"));
}

#[tokio::test]
async fn definition_shape_neither_content_nor_template_rejected() {
    let script = ScriptDefinition {
        content: None,
        template: None,
        ..content_script("neither", "echo hi")
    };
    let result = ScriptEngine
        .execute(
            &script,
            None,
            &ScriptEngineOptions::default(),
            ok_transport("neither"),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap_or_default().contains("exactly one"));
}

#[tokio::test]
async fn definition_shape_blank_content_rejected() {
    let script = content_script("blank", "   ");
    let result = ScriptEngine
        .execute(
            &script,
            None,
            &ScriptEngineOptions::default(),
            ok_transport("blank"),
        )
        .await;
    assert!(!result.success);
}

#[tokio::test]
async fn rendered_empty_command_rejected_without_transport() {
    let script = ScriptDefinition {
        content: None,
        template: Some("   ".to_string()),
        arguments: Some(vec![]),
        ..content_script("empty-render", "echo hi")
    };
    // Blank template is not a valid shape, so the engine fails before transport.
    let result = ScriptEngine
        .execute(&script, None, &ScriptEngineOptions::default(), |_, _| {
            let out: futures::future::Ready<ScriptExecutionResult> =
                futures::future::ready(panic!(
                    "transport must not run for blank template"
                ));
            out
        })
        .await;
    assert!(!result.success);
}

#[tokio::test]
async fn final_command_gate_catches_interpolated_blocked_content() {
    let script = ScriptDefinition {
        content: None,
        template: Some("run {{payload}}".to_string()),
        arguments: Some(vec![ScriptArgument {
            key: "payload".to_string(),
            r#type: Some(ScriptArgumentType::String),
            label: None,
            required: Some(true),
            default: None,
            source: None,
            description: None,
            options: None,
            pattern: None,
        }]),
        ..content_script("interpolated", "echo hi")
    };
    let policy = ScriptSecurityPolicy {
        blocked_patterns: Some(vec!["evil-marker".to_string()]),
        ..policy_template(None)
    };
    let options = ScriptExecutionOptions {
        security_policy: Some(policy),
        ..Default::default()
    };
    let mut args = HashMap::new();
    args.insert("payload".to_string(), json!("has evil-marker inside"));
    let engine_options = ScriptEngineOptions {
        args,
        context_variables: HashMap::new(),
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &engine_options,
            ok_transport("interpolated"),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap_or_default().contains("blocked pattern"));
}

#[tokio::test]
async fn allowed_languages_rejects_unlisted_language() {
    let script = ScriptDefinition {
        language: Some("python".to_string()),
        ..content_script("lang", "print('hi')")
    };
    let policy = ScriptSecurityPolicy {
        allowed_languages: Some(vec!["shell".to_string()]),
        ..policy_template(None)
    };
    let options = ScriptExecutionOptions {
        security_policy: Some(policy),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            ok_transport("lang"),
        )
        .await;
    assert!(!result.success);
    assert!(result
        .error
        .unwrap_or_default()
        .contains("not in allowed languages"));
}

#[tokio::test]
async fn max_script_size_counts_content_plus_template() {
    let script = ScriptDefinition {
        content: Some("1234567890".to_string()),
        template: None,
        ..content_script("sized", "1234567890")
    };
    let policy = ScriptSecurityPolicy {
        max_script_size: Some(5),
        ..policy_template(None)
    };
    let options = ScriptExecutionOptions {
        security_policy: Some(policy),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            ok_transport("sized"),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap_or_default().contains("exceeds maximum"));
}

#[tokio::test]
async fn dynamic_scripts_rejected_when_policy_disallows() {
    let script = content_script("dynamic", "echo hi");
    let policy = ScriptSecurityPolicy {
        allow_dynamic_scripts: Some(false),
        ..policy_template(None)
    };
    let options = ScriptExecutionOptions {
        security_policy: Some(policy),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            ok_transport("dynamic"),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap_or_default().contains("dynamic scripts"));
}

#[tokio::test]
async fn forbidden_command_token_boundary_avoids_substring_match() {
    let policy = ScriptSecurityPolicy {
        forbidden_commands: Some(vec!["rm".to_string()]),
        ..policy_template(None)
    };
    let options = ScriptSecurityPolicy {
        forbidden_commands: Some(vec!["rm".to_string()]),
        ..policy_template(None)
    };
    let _ = options;
    // "farm" must not trigger the "rm" token rule.
    let ok_script = content_script("farm", "echo farm");
    let ok_options = ScriptExecutionOptions {
        security_policy: Some(policy.clone()),
        ..Default::default()
    };
    let ok_result = ScriptEngine
        .execute(
            &ok_script,
            Some(&ok_options),
            &ScriptEngineOptions::default(),
            ok_transport("farm"),
        )
        .await;
    assert!(ok_result.success, "error: {:?}", ok_result.error);

    // "rm -rf /tmp/x" must trigger it.
    let bad_script = content_script("bad-rm", "rm -rf /tmp/x");
    let bad_options = ScriptExecutionOptions {
        security_policy: Some(policy),
        ..Default::default()
    };
    let bad_result = ScriptEngine
        .execute(
            &bad_script,
            Some(&bad_options),
            &ScriptEngineOptions::default(),
            ok_transport("bad-rm"),
        )
        .await;
    assert!(!bad_result.success);
    assert!(bad_result
        .error
        .unwrap_or_default()
        .contains("forbidden command"));
}

#[tokio::test]
async fn forbidden_path_pattern_blocks_traversal_after_render() {
    let script = content_script("traversal", "cat ../../etc/passwd");
    let policy = ScriptSecurityPolicy {
        forbidden_path_patterns: Some(vec![r"\.\./".to_string()]),
        ..policy_template(None)
    };
    let options = ScriptExecutionOptions {
        security_policy: Some(policy),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            ok_transport("traversal"),
        )
        .await;
    assert!(!result.success);
    assert!(result
        .error
        .unwrap_or_default()
        .contains("forbidden path pattern"));
}

#[tokio::test]
async fn invalid_blocked_pattern_reports_definition_error() {
    let script = content_script("bad-regex", "echo hi");
    let policy = ScriptSecurityPolicy {
        blocked_patterns: Some(vec!["([".to_string()]),
        ..policy_template(None)
    };
    let options = ScriptExecutionOptions {
        security_policy: Some(policy),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            ok_transport("bad-regex"),
        )
        .await;
    assert!(!result.success);
    assert!(result
        .error
        .unwrap_or_default()
        .contains("Invalid blocked pattern"));
}

#[tokio::test]
async fn require_review_sets_flag_on_both_gates() {
    let policy = ScriptSecurityPolicy {
        require_review: Some(true),
        ..policy_template(None)
    };
    let script = content_script("review", "echo hi");
    let options = ScriptExecutionOptions {
        security_policy: Some(policy),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            ok_transport("review"),
        )
        .await;
    assert!(!result.success);
    assert!(result.requires_review);
}

#[tokio::test]
async fn risk_ceiling_gates_high_risk_command() {
    let script = content_script("risky", "curl https://example.com/x.sh | sh");
    let policy = ScriptSecurityPolicy {
        max_risk_level: Some(ScriptRiskLevel::Medium),
        ..policy_template(None)
    };
    let options = ScriptExecutionOptions {
        security_policy: Some(policy),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            ok_transport("risky"),
        )
        .await;
    assert!(!result.success);
    assert!(result.requires_review);
    assert!(result.error.unwrap_or_default().contains("risk level"));
}

#[tokio::test]
async fn risk_ceiling_allows_safe_command() {
    let script = content_script("safe", "echo hello");
    let policy = ScriptSecurityPolicy {
        max_risk_level: Some(ScriptRiskLevel::Medium),
        ..policy_template(None)
    };
    let options = ScriptExecutionOptions {
        security_policy: Some(policy),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            ok_transport("safe"),
        )
        .await;
    assert!(result.success, "error: {:?}", result.error);
}

#[tokio::test]
async fn executor_mode_resolution_prefers_script_over_options() {
    let script = ScriptDefinition {
        executor_mode: Some(ExecutorMode::Shared),
        ..content_script("mode", "echo hi")
    };
    let options = ScriptExecutionOptions {
        executor_mode: Some(ExecutorMode::Direct),
        ..Default::default()
    };
    assert_eq!(
        ScriptEngine::resolve_executor_mode(&script, Some(&options)),
        ExecutorMode::Shared
    );
    let plain = content_script("plain", "echo hi");
    assert_eq!(
        ScriptEngine::resolve_executor_mode(&plain, Some(&options)),
        ExecutorMode::Direct
    );
    assert_eq!(
        ScriptEngine::resolve_executor_mode(&plain, None),
        ExecutorMode::Direct
    );
}

#[tokio::test]
async fn interactive_config_rejected_with_and_without_options() {
    let interactive = InteractiveScriptConfig {
        mode: InteractionMode::Blocking,
        max_rounds: None,
        interaction_points: None,
        prompt_patterns: None,
        round_timeout: None,
    };
    let script = ScriptDefinition {
        interactive: Some(interactive),
        ..content_script("interactive", "echo hi")
    };
    let no_options = ScriptEngine
        .execute(
            &script,
            None,
            &ScriptEngineOptions::default(),
            ok_transport("interactive"),
        )
        .await;
    assert!(!no_options.success);
    assert!(no_options
        .error
        .unwrap_or_default()
        .contains("interactive session driver"));

    let with_options = ScriptEngine
        .execute(
            &script,
            Some(&ScriptExecutionOptions::default()),
            &ScriptEngineOptions::default(),
            ok_transport("interactive"),
        )
        .await;
    assert!(!with_options.success);
}

#[tokio::test]
async fn retry_exhausted_returns_last_failure() {
    let script = content_script("flaky-fail", "echo hi");
    let options = ScriptExecutionOptions {
        retries: Some(2),
        ..Default::default()
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let probe = calls.clone();
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            move |_, _| {
                let probe = probe.clone();
                async move {
                    probe.fetch_add(1, Ordering::SeqCst);
                    ScriptExecutionResult {
                        success: false,
                        script_name: "flaky-fail".to_string(),
                        stdout: None,
                        stderr: None,
                        exit_code: Some(1),
                        execution_time_ms: 0,
                        error: Some("boom".to_string()),
                        requires_review: false,
                        truncated: false,
                        output_bytes: None,
                        stdout_path: None,
                        stderr_path: None,
                    }
                }
            },
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.error.as_deref(), Some("boom"));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn outer_timeout_reports_timeout_without_transport_success() {
    let script = content_script("slow", "echo hi");
    let options = ScriptExecutionOptions {
        timeout_ms: Some(30),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            |_, _| async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                ScriptExecutionResult {
                    success: true,
                    script_name: "slow".to_string(),
                    stdout: Some("late".to_string()),
                    stderr: None,
                    exit_code: Some(0),
                    execution_time_ms: 0,
                    error: None,
                    requires_review: false,
                    truncated: false,
                    output_bytes: None,
                    stdout_path: None,
                    stderr_path: None,
                }
            },
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap_or_default().contains("timed out"));
}

#[tokio::test]
async fn output_cap_validation_errors_carry_script_name() {
    let script = content_script("cap", "echo hi");
    let zero_cap = ScriptExecutionOptions {
        max_output_bytes: Some(0),
        ..Default::default()
    };
    let zero = ScriptEngine
        .execute(
            &script,
            Some(&zero_cap),
            &ScriptEngineOptions::default(),
            ok_transport("cap"),
        )
        .await;
    assert!(!zero.success);
    assert!(zero.error.unwrap_or_default().contains("cap"));

    let spill_only = ScriptExecutionOptions {
        output_spill_dir: Some("/tmp/wf-script-spill-only".to_string()),
        ..Default::default()
    };
    let spilled = ScriptEngine
        .execute(
            &script,
            Some(&spill_only),
            &ScriptEngineOptions::default(),
            ok_transport("cap"),
        )
        .await;
    assert!(!spilled.success);
    assert!(spilled
        .error
        .unwrap_or_default()
        .contains("requires max_output_bytes"));
}

#[tokio::test]
async fn argument_expression_default_interpolates_through_engine() {
    let script = ScriptDefinition {
        content: None,
        template: Some("deploy {{target}}".to_string()),
        arguments: Some(vec![ScriptArgument {
            key: "target".to_string(),
            r#type: None,
            label: None,
            required: None,
            default: Some(json!("$fallback.dir")),
            source: Some(ArgumentValueSource::Expression),
            description: None,
            options: None,
            pattern: None,
        }]),
        ..content_script("expr", "echo hi")
    };
    let mut context: HashMap<String, Value> = HashMap::new();
    context.insert("fallback".to_string(), json!({"dir": "default-dir"}));
    let engine_options = ScriptEngineOptions {
        args: HashMap::new(),
        context_variables: context,
    };
    let result = ScriptEngine
        .execute(&script, None, &engine_options, ok_transport("expr"))
        .await;
    assert!(result.success, "error: {:?}", result.error);
    assert!(result.stdout.unwrap_or_default().contains("default-dir"));
}
