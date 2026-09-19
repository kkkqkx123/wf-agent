use std::collections::HashMap;

use wf_script::{
    cap_stream, truncate_tail, ScriptDefinition, ScriptEngine, ScriptEngineOptions,
    ScriptExecutionOptions, ScriptExecutionResult, MAX_ENV_BYTES, MAX_STDIN_BYTES,
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

fn success_transport(
    name: &str,
) -> impl Fn(String, Option<ScriptExecutionOptions>) -> futures::future::Ready<ScriptExecutionResult>
{
    let owned = name.to_string();
    move |cmd, _| {
        futures::future::ready(ScriptExecutionResult {
            success: true,
            script_name: owned.clone(),
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

fn unique_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "wf-script-payload-{}-{}-{}",
        std::process::id(),
        tag,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("test dir is creatable");
    dir
}

#[test]
fn truncate_tail_short_text_keeps_everything() {
    let (kept, truncated) = truncate_tail("hello", 100);
    assert_eq!(kept, "hello");
    assert!(!truncated);
}

#[test]
fn truncate_tail_boundary_respects_multibyte_chars() {
    let text = format!("{}tail", "界".repeat(50));
    let (kept, truncated) = truncate_tail(&text, 8);
    assert!(truncated);
    assert!(kept.ends_with("tail"));
    assert!(std::str::from_utf8(kept.as_bytes()).is_ok());
}

#[test]
fn cap_stream_matrix() {
    let plain = cap_stream(Some("hello".to_string()), None, None, "s-stdout");
    assert!(!plain.truncated);
    assert_eq!(plain.text.as_deref(), Some("hello"));
    assert_eq!(plain.total_bytes, 5);

    let kept = cap_stream(Some("0123456789".to_string()), Some(4), None, "s-stdout");
    assert!(kept.truncated);
    assert_eq!(kept.text.as_deref(), Some("6789"));
    assert_eq!(kept.total_bytes, 10);
    assert!(kept.spilled_path.is_none());

    let empty = cap_stream(None, Some(4), None, "s-stdout");
    assert!(!empty.truncated);
    assert!(empty.text.is_none());
}

#[test]
fn cap_stream_spills_full_content_and_reports_path() {
    let dir = unique_dir("spill");
    let capped = cap_stream(
        Some("0123456789".to_string()),
        Some(4),
        Some(&dir.to_string_lossy()),
        "probe-stdout",
    );
    assert!(capped.truncated);
    assert_eq!(capped.text.as_deref(), Some("6789"));
    let path = capped.spilled_path.expect("spill path is reported");
    assert_eq!(
        std::fs::read_to_string(path).expect("spill is readable"),
        "0123456789"
    );
}

#[test]
fn cap_stream_spill_failure_keeps_tail_and_reports_error() {
    let probe = unique_dir("spill-blocker");
    let blocker = probe.join("blocker");
    std::fs::write(&blocker, "occupied").expect("blocker is writable");
    let capped = cap_stream(
        Some("0123456789".to_string()),
        Some(4),
        Some(&blocker.to_string_lossy()),
        "probe-stdout",
    );
    assert!(capped.truncated);
    assert_eq!(capped.text.as_deref(), Some("6789"));
    assert!(capped.spilled_path.is_none());
    assert!(capped.spill_error.is_some());
}

#[tokio::test]
async fn stdin_and_stdin_file_conflict_rejected() {
    let script = content_script("conflict", "cat");
    let options = ScriptExecutionOptions {
        stdin: Some("hi".to_string()),
        stdin_file: Some("/tmp/whatever".to_string()),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            success_transport("conflict"),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap_or_default().contains("exactly one"));
}

#[tokio::test]
async fn stdin_file_materializes_into_stdin() {
    let dir = unique_dir("stdin-file");
    let source = dir.join("input.txt");
    std::fs::write(&source, "file-stdin").expect("stdin file is writable");
    let script = content_script("stdin-file", "cat");
    let options = ScriptExecutionOptions {
        stdin_file: Some(source.to_string_lossy().to_string()),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            |_, opts| {
                let opts = opts.expect("options reach transport");
                assert_eq!(opts.stdin.as_deref(), Some("file-stdin"));
                assert!(opts.stdin_file.is_none());
                futures::future::ready(ScriptExecutionResult {
                    success: true,
                    script_name: "stdin-file".to_string(),
                    stdout: None,
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
            },
        )
        .await;
    assert!(result.success, "error: {:?}", result.error);
}

#[tokio::test]
async fn oversized_inline_stdin_rejected() {
    let script = content_script("big-stdin", "cat");
    let options = ScriptExecutionOptions {
        stdin: Some("x".repeat(MAX_STDIN_BYTES + 1)),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            success_transport("big-stdin"),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap_or_default().contains("stdin"));
}

#[tokio::test]
async fn missing_stdin_file_rejected() {
    let script = content_script("missing-stdin", "cat");
    let options = ScriptExecutionOptions {
        stdin_file: Some("/no/such/wf-script-stdin-file".to_string()),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            success_transport("missing-stdin"),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap_or_default().contains("stdin file"));
}

#[tokio::test]
async fn environment_size_boundary() {
    let script = content_script("env", "echo hi");
    let just_over = ScriptExecutionOptions {
        environment: Some(HashMap::from([(
            "BLOB".to_string(),
            "x".repeat(MAX_ENV_BYTES),
        )])),
        ..Default::default()
    };
    let rejected = ScriptEngine
        .execute(
            &script,
            Some(&just_over),
            &ScriptEngineOptions::default(),
            success_transport("env"),
        )
        .await;
    assert!(!rejected.success);
    assert!(rejected.error.unwrap_or_default().contains("environment"));

    let small = ScriptExecutionOptions {
        environment: Some(HashMap::from([("SMALL".to_string(), "ok".to_string())])),
        ..Default::default()
    };
    let accepted = ScriptEngine
        .execute(
            &script,
            Some(&small),
            &ScriptEngineOptions::default(),
            success_transport("env"),
        )
        .await;
    assert!(accepted.success, "error: {:?}", accepted.error);
}

#[tokio::test]
async fn input_files_export_and_escape_rejected() {
    let dir = unique_dir("inputs");
    let inner = dir.join("data.bin");
    std::fs::write(&inner, "payload").expect("input file is writable");
    let root = dir.to_string_lossy().to_string();
    let script = content_script("inputs", "echo hi");
    let options = ScriptExecutionOptions {
        working_directory: Some(root.clone()),
        input_files: Some(HashMap::from([(
            "corpus".to_string(),
            inner.to_string_lossy().to_string(),
        )])),
        ..Default::default()
    };
    let expected = inner.to_string_lossy().to_string();
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            |_, opts| {
                let expected = expected.clone();
                async move {
                    let env = opts
                        .expect("options reach transport")
                        .environment
                        .expect("env set");
                    assert_eq!(env.get("WF_INPUT_CORPUS"), Some(&expected));
                    ScriptExecutionResult {
                        success: true,
                        script_name: "inputs".to_string(),
                        stdout: None,
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
                }
            },
        )
        .await;
    assert!(result.success, "error: {:?}", result.error);

    let escape = ScriptExecutionOptions {
        working_directory: Some(root),
        input_files: Some(HashMap::from([(
            "outside".to_string(),
            "/etc/hostname".to_string(),
        )])),
        ..Default::default()
    };
    let escaped = ScriptEngine
        .execute(
            &script,
            Some(&escape),
            &ScriptEngineOptions::default(),
            success_transport("inputs"),
        )
        .await;
    assert!(!escaped.success);
    assert!(escaped.error.unwrap_or_default().contains("input file"));
}

#[tokio::test]
async fn input_files_sanitization_collision_rejected() {
    let dir = unique_dir("collision");
    let file = dir.join("data.bin");
    std::fs::write(&file, "payload").expect("input file is writable");
    let root = dir.to_string_lossy().to_string();
    let path = file.to_string_lossy().to_string();
    let script = content_script("collision", "echo hi");
    // "a-b" and "a_b" both sanitize to WF_INPUT_A_B.
    let options = ScriptExecutionOptions {
        working_directory: Some(root),
        input_files: Some(HashMap::from([
            ("a-b".to_string(), path.clone()),
            ("a_b".to_string(), path),
        ])),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            success_transport("collision"),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap_or_default().contains("collides"));
}

#[tokio::test]
async fn input_files_overwriting_env_rejected() {
    let dir = unique_dir("overwrite");
    let file = dir.join("data.bin");
    std::fs::write(&file, "payload").expect("input file is writable");
    let root = dir.to_string_lossy().to_string();
    let script = content_script("overwrite", "echo hi");
    let options = ScriptExecutionOptions {
        working_directory: Some(root),
        environment: Some(HashMap::from([(
            "WF_INPUT_CORPUS".to_string(),
            "pre-existing".to_string(),
        )])),
        input_files: Some(HashMap::from([(
            "corpus".to_string(),
            file.to_string_lossy().to_string(),
        )])),
        ..Default::default()
    };
    let result = ScriptEngine
        .execute(
            &script,
            Some(&options),
            &ScriptEngineOptions::default(),
            success_transport("overwrite"),
        )
        .await;
    assert!(!result.success);
    assert!(result
        .error
        .unwrap_or_default()
        .contains("overwrites existing"));
}
