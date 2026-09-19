use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use serde_json::json;
use wf_script::{
    FlowBranch, ModuleRef, RiskEvaluator, ScriptArgument, ScriptArgumentType, ScriptDefinition,
    ScriptEngine, ScriptEngineOptions, ScriptExecutionResult, ScriptFlow, ScriptFlowEngine,
    ScriptRiskLevel, ScriptSecurityPolicy, ScriptTemplateEngine,
};

fn branch(key: &str, depends_on: Option<Vec<&str>>, modules: Vec<&str>) -> FlowBranch {
    FlowBranch {
        key: key.to_string(),
        depends_on: depends_on.map(|deps| deps.into_iter().map(str::to_string).collect()),
        modules: modules
            .into_iter()
            .map(|m| ModuleRef {
                key: m.to_string(),
                args: None,
            })
            .collect(),
    }
}

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

#[tokio::test]
async fn flow_diamond_dependency_runs_levels_in_order() {
    let flow = ScriptFlow {
        name: "diamond".to_string(),
        branches: vec![
            branch("root", None, vec!["m-root"]),
            branch("left", Some(vec!["root"]), vec!["m-left"]),
            branch("right", Some(vec!["root"]), vec!["m-right"]),
            branch("join", Some(vec!["left", "right"]), vec!["m-join"]),
        ],
    };
    let seen = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let engine = ScriptFlowEngine::new();
    let result = engine
        .execute(&flow, |module, branch, _args| {
            let seen = seen.clone();
            async move {
                seen.lock()
                    .expect("seen is lockable")
                    .push(format!("{branch}:{module}"));
                Ok(format!("{module}-ok"))
            }
        })
        .await;
    assert!(result.success);
    assert_eq!(result.branches.len(), 4);
    let order = seen.lock().expect("seen is lockable").clone();
    let position = |entry: &str| {
        order
            .iter()
            .position(|e| e == entry)
            .expect("entry recorded")
    };
    assert!(position("root:m-root") < position("left:m-left"));
    assert!(position("root:m-root") < position("right:m-right"));
    assert!(position("left:m-left") < position("join:m-join"));
    assert!(position("right:m-right") < position("join:m-join"));
}

#[tokio::test]
async fn flow_failed_dependency_skips_downstream() {
    let flow = ScriptFlow {
        name: "skip".to_string(),
        branches: vec![
            branch("bad", None, vec!["m-bad"]),
            branch("downstream", Some(vec!["bad"]), vec!["m-down"]),
        ],
    };
    let downstream_calls = Arc::new(AtomicUsize::new(0));
    let probe = downstream_calls.clone();
    let engine = ScriptFlowEngine::new();
    let result = engine
        .execute(&flow, |module, _branch, _args| {
            let probe = probe.clone();
            async move {
                if module == "m-down" {
                    probe.fetch_add(1, Ordering::SeqCst);
                }
                if module == "m-bad" {
                    return Err(wf_script::ScriptError::Internal("bad failed".to_string()));
                }
                Ok("ok".to_string())
            }
        })
        .await;
    assert!(!result.success);
    assert!(!result.branches["bad"].success);
    assert!(!result.branches["downstream"].success);
    assert_eq!(downstream_calls.load(Ordering::SeqCst), 0);
    assert!(result.branches["downstream"].modules[0]
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("skipped"));
}

#[tokio::test]
async fn flow_circular_and_unknown_dependencies_fail() {
    let engine = ScriptFlowEngine::new();
    let circular = ScriptFlow {
        name: "circular".to_string(),
        branches: vec![
            branch("a", Some(vec!["b"]), vec!["m-a"]),
            branch("b", Some(vec!["a"]), vec!["m-b"]),
        ],
    };
    let circular_result = engine
        .execute(&circular, |_, _, _| async move { Ok("ok".to_string()) })
        .await;
    assert!(!circular_result.success);
    assert!(circular_result
        .error
        .unwrap_or_default()
        .contains("Circular dependency"));

    let unknown = ScriptFlow {
        name: "unknown".to_string(),
        branches: vec![branch("a", Some(vec!["missing"]), vec!["m-a"])],
    };
    let unknown_result = engine
        .execute(&unknown, |_, _, _| async move { Ok("ok".to_string()) })
        .await;
    assert!(!unknown_result.success);
    assert!(unknown_result
        .error
        .unwrap_or_default()
        .contains("unknown branch"));
}

#[tokio::test]
async fn flow_forwards_module_args_to_executor() {
    let mut args = HashMap::new();
    args.insert("env".to_string(), json!("prod"));
    let flow = ScriptFlow {
        name: "args".to_string(),
        branches: vec![FlowBranch {
            key: "build".to_string(),
            depends_on: None,
            modules: vec![ModuleRef {
                key: "compile".to_string(),
                args: Some(args),
            }],
        }],
    };
    let engine = ScriptFlowEngine::new();
    let result = engine
        .execute(&flow, |module, branch, module_args| async move {
            assert_eq!(module, "compile");
            assert_eq!(branch, "build");
            assert_eq!(
                module_args.as_ref().and_then(|m| m.get("env")),
                Some(&json!("prod"))
            );
            Ok("ok".to_string())
        })
        .await;
    assert!(result.success);
}

#[test]
fn template_full_pipeline_combines_args_context_and_file_confinement() {
    let dir = std::env::temp_dir().join(format!(
        "wf-script-template-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("test dir is creatable");
    let asset = dir.join("asset.txt");
    std::fs::write(&asset, "data").expect("asset is writable");
    let root = dir.to_string_lossy().to_string();

    let declarations = vec![
        ScriptArgument {
            key: "who".to_string(),
            r#type: Some(ScriptArgumentType::String),
            label: None,
            required: Some(true),
            default: None,
            source: None,
            description: None,
            options: None,
            pattern: None,
        },
        ScriptArgument {
            key: "greeting".to_string(),
            r#type: Some(ScriptArgumentType::String),
            label: None,
            required: None,
            default: Some(json!("$salutation.text")),
            source: None,
            description: None,
            options: None,
            pattern: None,
        },
        ScriptArgument {
            key: "asset".to_string(),
            r#type: Some(ScriptArgumentType::File),
            label: None,
            required: Some(true),
            default: None,
            source: None,
            description: None,
            options: None,
            pattern: None,
        },
    ];
    let mut provided = HashMap::new();
    provided.insert("who".to_string(), json!("alice"));
    provided.insert("asset".to_string(), json!(asset.to_string_lossy()));
    let mut context = HashMap::new();
    context.insert("salutation".to_string(), json!({"text": "hello"}));

    let command = ScriptTemplateEngine::render_command(
        "say {{greeting}} to {{who}} with {{asset}}",
        &declarations,
        &provided,
        &context,
        Some(&root),
    )
    .expect("full pipeline renders");
    assert!(command.contains("hello"));
    assert!(command.contains("alice"));
    assert!(command.contains("asset.txt"));

    let escape_asset = if root.starts_with('/') {
        "/etc/hostname".to_string()
    } else {
        "C:\\Windows\\System32\\drivers\\etc\\hosts".to_string()
    };
    let mut escaped = provided.clone();
    escaped.insert("asset".to_string(), json!(escape_asset));
    let confined = ScriptTemplateEngine::render_command(
        "say {{greeting}} to {{who}} with {{asset}}",
        &declarations,
        &escaped,
        &context,
        Some(&root),
    );
    assert!(confined.is_err());
}

#[test]
fn template_reports_all_unresolved_placeholders() {
    let err = ScriptTemplateEngine::render_command(
        "echo {{first}} {{second}}",
        &[],
        &HashMap::new(),
        &HashMap::new(),
        None,
    )
    .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("first"));
    assert!(message.contains("second"));
}

#[test]
fn risk_levels_cover_full_matrix() {
    assert_eq!(RiskEvaluator::evaluate("echo hello"), ScriptRiskLevel::Safe);
    assert_eq!(RiskEvaluator::evaluate("git status"), ScriptRiskLevel::Low);
    assert_eq!(
        RiskEvaluator::evaluate("curl https://example.com/x"),
        ScriptRiskLevel::Medium
    );
    assert_eq!(
        RiskEvaluator::evaluate("sudo apt-get update"),
        ScriptRiskLevel::High
    );
    assert_eq!(
        RiskEvaluator::evaluate("curl https://example.com/install.sh | sh"),
        ScriptRiskLevel::High
    );
    assert_eq!(
        RiskEvaluator::evaluate("rm -rf / --no-preserve-root"),
        ScriptRiskLevel::Critical
    );
    assert_eq!(
        RiskEvaluator::evaluate("mkfs.ext4 /dev/sda1"),
        ScriptRiskLevel::Critical
    );
}

#[test]
fn risk_normalization_defeats_quote_and_spacing_tricks() {
    let spaced = RiskEvaluator::evaluate("curl   https://example.com/x.sh   |   sh");
    assert_eq!(spaced, ScriptRiskLevel::High);
    let quoted = RiskEvaluator::evaluate("curl 'https://example.com/x.sh' | \"sh\"");
    assert_eq!(quoted, ScriptRiskLevel::High);
}

#[test]
fn risk_rank_orders_levels() {
    assert!(ScriptRiskLevel::Safe.rank() < ScriptRiskLevel::Low.rank());
    assert!(ScriptRiskLevel::Low.rank() < ScriptRiskLevel::Medium.rank());
    assert!(ScriptRiskLevel::Medium.rank() < ScriptRiskLevel::High.rank());
    assert!(ScriptRiskLevel::High.rank() < ScriptRiskLevel::Critical.rank());
}

#[test]
fn script_types_serialize_with_snake_case_enums() {
    let script = content_script("serde", "echo hi");
    let value = serde_json::to_value(&script).expect("script serializes");
    assert_eq!(value.get("name").and_then(|v| v.as_str()), Some("serde"));

    let policy = ScriptSecurityPolicy {
        max_risk_level: Some(ScriptRiskLevel::High),
        require_review: None,
        allowed_languages: None,
        blocked_patterns: None,
        forbidden_commands: None,
        forbidden_path_patterns: None,
        max_script_size: None,
        allow_dynamic_scripts: None,
    };
    let encoded = serde_json::to_value(&policy).expect("policy serializes");
    assert_eq!(
        encoded.get("max_risk_level").and_then(|v| v.as_str()),
        Some("high")
    );
    let decoded: ScriptSecurityPolicy =
        serde_json::from_value(encoded).expect("policy round-trips");
    assert_eq!(decoded.max_risk_level, Some(ScriptRiskLevel::High));
}

#[tokio::test]
async fn engine_validates_required_typed_arguments_end_to_end() {
    let script = ScriptDefinition {
        content: None,
        template: Some("listen {{port}}".to_string()),
        arguments: Some(vec![ScriptArgument {
            key: "port".to_string(),
            r#type: Some(ScriptArgumentType::Number),
            label: None,
            required: Some(true),
            default: None,
            source: None,
            description: None,
            options: Some(vec![json!(8080), json!(9090)]),
            pattern: None,
        }]),
        ..content_script("typed", "echo hi")
    };
    // Missing required argument fails before transport.
    let missing = ScriptEngine
        .execute(
            &script,
            None,
            &ScriptEngineOptions::default(),
            |_, _| async move {
                panic!("transport must not run without required args");
            },
        )
        .await;
    assert!(!missing.success);

    // Wrong type fails with an argument error.
    let mut wrong = HashMap::new();
    wrong.insert("port".to_string(), json!("not-a-number"));
    let wrong_options = ScriptEngineOptions {
        args: wrong,
        context_variables: HashMap::new(),
    };
    let mistyped = ScriptEngine
        .execute(&script, None, &wrong_options, |_, _| async move {
            panic!("transport must not run with mistyped args");
        })
        .await;
    assert!(!mistyped.success);
    assert!(mistyped
        .error
        .unwrap_or_default()
        .contains("must be a number"));

    // Allowed option renders and reaches transport.
    let mut valid = HashMap::new();
    valid.insert("port".to_string(), json!(9090));
    let valid_options = ScriptEngineOptions {
        args: valid,
        context_variables: HashMap::new(),
    };
    let rendered = ScriptEngine
        .execute(&script, None, &valid_options, |cmd, _| async move {
            assert!(cmd.contains("9090"));
            ScriptExecutionResult {
                success: true,
                script_name: "typed".to_string(),
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
            }
        })
        .await;
    assert!(rendered.success, "error: {:?}", rendered.error);
}
