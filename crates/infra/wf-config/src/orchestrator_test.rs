use super::*;
use std::io::Write;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn setup_test_project(dir: &Path) {
    let infra = dir.join("configs").join("infrastructure");
    std::fs::create_dir_all(&infra).unwrap();

    let mut f = std::fs::File::create(infra.join("storage.toml")).unwrap();
    writeln!(f, "type = \"sqlite\"\n[sqlite]\ndb_path = \"./test.db\"").unwrap();

    let mut f = std::fs::File::create(infra.join("timeout.toml")).unwrap();
    writeln!(f, "default = 60000").unwrap();

    let mut f = std::fs::File::create(infra.join("metrics.toml")).unwrap();
    writeln!(f, "enabled = true\nreporting_interval = 5000").unwrap();

    let mut f = std::fs::File::create(infra.join("output.toml")).unwrap();
    writeln!(f, "dir = \"./test-outputs\"\nlog_file_pattern = \"test.log\"\nenable_log_terminal = false\nenable_sdk_logs = false\nsdk_log_level = \"info\"").unwrap();
}

/// Project with a preset index + development preset whose `files` mapping
/// points at custom filenames.
fn setup_preset_project(dir: &Path) {
    let infra = dir.join("configs").join("infrastructure");
    std::fs::create_dir_all(&infra).unwrap();

    write_json(
        &infra.join("index.json"),
        r#"{"version": "1.0", "type": "infrastructure_presets", "paths": ["./*.json"]}"#,
    );
    write_json(
        &infra.join("development.json"),
        r#"{"id": "development", "name": "Development", "files": {"storage": "./custom-storage.toml", "timeout": "./custom-timeout.toml", "metrics": "./custom-metrics.toml", "output": "./custom-output.toml", "file_checkpoint": "./custom-checkpoint.toml", "presets": "./custom-presets.toml", "tools": "./custom-tools.toml", "sandbox": "./custom-sandbox.toml"}}"#,
    );

    let mut f = std::fs::File::create(infra.join("custom-storage.toml")).unwrap();
    writeln!(f, "type = \"postgres\"\n[postgres]\nhost = \"localhost\"\nport = 5432\nusername = \"u\"\npassword = \"p\"\ndatabase = \"d\"").unwrap();
    let mut f = std::fs::File::create(infra.join("custom-timeout.toml")).unwrap();
    writeln!(f, "default = 42000").unwrap();
    let mut f = std::fs::File::create(infra.join("custom-metrics.toml")).unwrap();
    writeln!(f, "enabled = false").unwrap();
    let mut f = std::fs::File::create(infra.join("custom-output.toml")).unwrap();
    writeln!(f, "dir = \"./preset-outputs\"\nlog_file_pattern = \"p.log\"\nenable_log_terminal = false\nenable_sdk_logs = false\nsdk_log_level = \"info\"").unwrap();
}

fn write_json(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

fn clear_wf_env_vars() {
    for var in [
        "WF_STORAGE_TYPE",
        "WF_STORAGE_SQLITE_DB_PATH",
        "WF_TIMEOUT_DEFAULT",
        "WF_METRICS_ENABLED",
        "WF_OUTPUT_DIR",
        "WF_AGENT_MAX_ITERATIONS_CAP",
        "WF_AGENT_DEFAULT_MAX_ITERATIONS",
        "WF_AGENT_MAX_CONCURRENT",
        "WF_AGENT_MAX_SUB_AGENT_DEPTH",
        "WF_AGENT_MAX_PAUSE_DURATION_MS",
        "WF_WORKFLOW_LOOP_MAX_ITERATIONS_CAP",
        "WF_WORKFLOW_LOOP_DEFAULT_MAX_ITERATIONS",
        "WF_WORKFLOW_MAX_NAVIGATION_MULTIPLIER",
        "WF_EXEC_NODE_TIMEOUT_MS",
        "WF_EXEC_MAX_EXECUTION_TIME_MS",
    ] {
        std::env::remove_var(var);
    }
}

#[test]
fn test_assemble_from_project_dir() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_wf_env_vars();

    let dir = std::env::temp_dir().join(format!("wf-orch-{}-assemble", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    setup_test_project(&dir);

    let config = ConfigOrchestrator::assemble(&dir, None).unwrap();

    assert_eq!(config.storage.storage_type, StorageType::Sqlite);
    let sqlite = config.storage.sqlite.as_ref().unwrap();
    assert_eq!(sqlite.db_path, "./test.db");
    assert_eq!(config.timeout.default, Some(60000));
    assert_eq!(config.metrics.reporting_interval, Some(5000));
    assert_eq!(config.output.dir, "./test-outputs");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn repo_metrics_toml_matches_schema() {
    // The checked-in `configs/infrastructure/metrics.toml`
    // parses against the current `MetricsConfig` schema (no removed
    // `template_metrics` section, new fields present). Skipped when the
    // workspace `configs/` dir is absent (vendored/standalone builds).
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    if !repo_root.join("configs").join("infrastructure").exists() {
        return;
    }
    let _lock = ENV_LOCK.lock().unwrap();
    clear_wf_env_vars();
    let config = ConfigOrchestrator::assemble(&repo_root, None).unwrap();
    let metrics = config.metrics;
    assert_eq!(metrics.enabled, Some(true));
    assert_eq!(metrics.retention_ms, Some(3_600_000));
    let thresholds = metrics.anomaly_thresholds.unwrap();
    assert_eq!(thresholds.max_error_count, Some(100));
    assert!((thresholds.min_success_rate.unwrap() - 0.8).abs() < 1e-9);
    assert!(metrics.subgraph_metrics.is_some());
}

#[test]
fn test_assemble_defaults_when_no_files() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_wf_env_vars();

    let dir = std::env::temp_dir().join(format!("wf-orch-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let config = ConfigOrchestrator::assemble(&dir, None).unwrap();

    assert_eq!(config.storage.storage_type, StorageType::Memory);
    assert_eq!(config.timeout.default, Some(30000));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_assemble_with_overrides() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_wf_env_vars();

    let dir = std::env::temp_dir().join(format!("wf-orch-override-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    setup_test_project(&dir);

    let overrides = ConfigOverrides {
        timeout: Some(TimeoutConfig {
            default: Some(99999),
            ..Default::default()
        }),
        ..Default::default()
    };

    let config = ConfigOrchestrator::assemble(&dir, Some(overrides)).unwrap();
    assert_eq!(config.timeout.default, Some(99999));
    assert_eq!(config.storage.storage_type, StorageType::Sqlite);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_assemble_env_override() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_wf_env_vars();

    let dir = std::env::temp_dir().join(format!("wf-orch-env-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    setup_test_project(&dir);

    std::env::set_var("WF_STORAGE_TYPE", "memory");
    let config = ConfigOrchestrator::assemble(&dir, None).unwrap();
    assert_eq!(config.storage.storage_type, StorageType::Memory);
    clear_wf_env_vars();

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_assemble_limits_env_override() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_wf_env_vars();

    let dir = std::env::temp_dir().join(format!("wf-orch-limits-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    setup_test_project(&dir);

    std::env::set_var("WF_AGENT_MAX_ITERATIONS_CAP", "500");
    std::env::set_var("WF_WORKFLOW_LOOP_MAX_ITERATIONS_CAP", "3000");
    std::env::set_var("WF_EXEC_NODE_TIMEOUT_MS", "15000");
    std::env::set_var("WF_EXEC_MAX_EXECUTION_TIME_MS", "120000");
    let config = ConfigOrchestrator::assemble(&dir, None).unwrap();
    let agent = config.limits.agent.as_ref().unwrap();
    let workflow = config.limits.workflow.as_ref().unwrap();
    let defaults = config.limits.execution_defaults.as_ref().unwrap();
    assert_eq!(agent.max_iterations_cap, Some(500));
    assert_eq!(workflow.loop_max_iterations_cap, Some(3000));
    assert_eq!(defaults.node_timeout_ms, Some(15000));
    assert_eq!(defaults.max_execution_time_ms, Some(120000));
    clear_wf_env_vars();

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_builder_custom_infra_dir() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_wf_env_vars();

    let dir = std::env::temp_dir().join(format!("wf-orch-builder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let custom_infra = dir.join("custom").join("infra");
    std::fs::create_dir_all(&custom_infra).unwrap();

    let mut f = std::fs::File::create(custom_infra.join("storage.toml")).unwrap();
    writeln!(f, "type = \"postgres\"\n[postgres]\nhost = \"localhost\"\nport = 5432\nusername = \"user\"\npassword = \"pass\"\ndatabase = \"test\"").unwrap();

    let config = ConfigOrchestratorBuilder::new(&dir)
        .infra_dir(&custom_infra)
        .build()
        .assemble(None)
        .unwrap();

    assert_eq!(config.storage.storage_type, StorageType::Postgres);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_assemble_with_preset_hits_preset() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_wf_env_vars();

    let dir = std::env::temp_dir().join(format!("wf-orch-preset-hit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    setup_preset_project(&dir);

    let config = ConfigOrchestrator::assemble_with_preset(
        &dir,
        Some(DEFAULT_INFRA_PRESET),
        Some(default_infra_file_mapping()),
        None,
    )
    .unwrap();

    // The development preset maps storage to custom-storage.toml.
    assert_eq!(config.storage.storage_type, StorageType::Postgres);
    assert_eq!(config.timeout.default, Some(42000));
    assert_eq!(config.metrics.enabled, Some(false));
    assert_eq!(config.output.dir, "./preset-outputs");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_assemble_with_preset_falls_back_on_missing_preset() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_wf_env_vars();

    let dir = std::env::temp_dir().join(format!("wf-orch-preset-miss-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    setup_preset_project(&dir);

    // Preset name does not exist in the index: fall back to default paths.
    let config = ConfigOrchestrator::assemble_with_preset(
        &dir,
        Some("nonexistent"),
        Some(default_infra_file_mapping()),
        None,
    )
    .unwrap();
    assert_eq!(config.storage.storage_type, StorageType::Memory);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_load_default_infrastructure_configs_resolves_default_preset() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_wf_env_vars();

    let dir = std::env::temp_dir().join(format!("wf-orch-default-preset-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    setup_preset_project(&dir);

    let config = load_default_infrastructure_configs(&dir).unwrap();
    assert_eq!(config.storage.storage_type, StorageType::Postgres);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_assemble_new_domain_overrides() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_wf_env_vars();

    let dir = std::env::temp_dir().join(format!("wf-orch-new-domain-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    setup_test_project(&dir);

    let overrides = ConfigOverrides {
        presets: Some(PresetsConfig {
            context_compression: Some(wf_types::config::presets::ContextCompressionPresetConfig {
                enabled: Some(true),
                threshold: Some(0.9),
                max_tokens: Some(2048),
                strategy: Some("sliding_window".to_string()),
            }),
            predefined_tools: None,
            predefined_prompts: None,
        }),
        tools: Some(ToolConfigs {
            read_file: Some(ReadFileConfig {
                workspace_dir: None,
                max_file_size: 1000,
                max_chars: 2000,
                max_lines: 50,
                enable_ignore: true,
                enable_protect: true,
                model_id: None,
            }),
            ..Default::default()
        }),
        file_checkpoint: Some(FileCheckpointConfig {
            enabled: true,
            workspace_root: Some("/data".to_string()),
            max_delta_chain_length: 40,
            custom_ignore_patterns: None,
            storage: None,
            failure_behavior: wf_types::config::file_checkpoint::FailureBehavior::Error,
            ..Default::default()
        }),
        ..Default::default()
    };

    let config = ConfigOrchestrator::assemble(&dir, Some(overrides)).unwrap();
    let presets = config.presets.context_compression.unwrap();
    assert_eq!(presets.enabled, Some(true));
    assert_eq!(presets.max_tokens, Some(2048));
    assert!(config.tools.read_file.is_some());
    assert_eq!(config.tools.read_file.as_ref().unwrap().max_file_size, 1000);
    assert!(config.file_checkpoint.enabled);
    assert_eq!(config.file_checkpoint.max_delta_chain_length, 40);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_full_bundle_assembly_from_repo_configs() {
    // The checked-in `configs/infrastructure/` preset
    // bundle (development.json + all domain files) assembles fully.
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    if !repo_root.join("configs").join("infrastructure").exists() {
        return;
    }
    let _lock = ENV_LOCK.lock().unwrap();
    clear_wf_env_vars();

    let config = load_default_infrastructure_configs(&repo_root).unwrap();

    // storage/metrics come from the preset mapping (same files).
    assert_eq!(config.metrics.enabled, Some(true));
    assert_eq!(config.timeout.default, Some(30000));
    assert!(config.sandbox.is_some(), "repo sandbox.toml must load");
    assert_eq!(config.output.dir, "./outputs");
    assert!(
        config.presets.context_compression.is_some(),
        "repo presets.toml must load"
    );
    assert!(
        config.tools.read_file.is_some(),
        "repo tools.toml [read_file] must load"
    );
    assert_eq!(
        config.tools.read_file.as_ref().unwrap().max_file_size,
        500_000,
        "read_file defaults applied"
    );
    assert_eq!(
        config.file_checkpoint.max_delta_chain_length, 20,
        "repo file-checkpoint.toml must load"
    );
}
