use super::*;

static XDG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn xdg_lock() -> std::sync::MutexGuard<'static, ()> {
    XDG_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// programmatic values are absent, while programmatic values keep priority.
/// Temp fixtures only: tests never depend on repository files.
#[test]
fn test_resolve_infra_config_from_file_layers() {
    let settings_dir = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    let settings_infra = settings_dir.path().join("infrastructure");
    let project_infra = project_dir.path().join(".wf").join("infrastructure");
    std::fs::create_dir_all(&settings_infra).unwrap();
    std::fs::create_dir_all(&project_infra).unwrap();
    std::fs::write(
        settings_infra.join("storage.toml"),
        "type = \"sqlite\"\n[sqlite]\ndb_path = \"./settings.db\"\n",
    )
    .unwrap();
    std::fs::write(
        project_infra.join("storage.toml"),
        "type = \"sqlite\"\n[sqlite]\ndb_path = \"./project.db\"\n",
    )
    .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let infra = InfraSourceConfig {
        project_root: Some(project_dir.path().to_path_buf()),
        settings_dir: Some(settings_dir.path().to_path_buf()),
        ..Default::default()
    };
    let config = resolve_with_file_layer(RuntimeConfig::default(), &infra, &runtime);

    // Project layer wins over the settings layer.
    assert_eq!(
        config.storage.sqlite.as_ref().unwrap().db_path,
        "./project.db"
    );
    assert_ne!(config.storage, StorageConfig::default());
    assert!(config.metrics.is_some());

    // MCP/skills sources are inherited.
    assert_eq!(
        config.mcp.project_root,
        Some(project_dir.path().to_path_buf())
    );

    // Programmatic overrides win over the file layer.
    let overridden = InfraSourceConfig {
        overrides: wf_config::orchestrator::ConfigOverrides {
            metrics: Some(MetricsConfig::default()),
            ..Default::default()
        },
        ..infra.clone()
    };
    let config = resolve_with_file_layer(RuntimeConfig::default(), &overridden, &runtime);
    assert!(config.metrics.is_some());
    // Other domains still come from the file layer.
    assert_ne!(config.storage, StorageConfig::default());
}

/// The workstation file layer (explicit ambient opt-in: global XDG file
/// then project `.wf/config.toml`, project wins) fills unset fields;
/// explicit programmatic values (CLI parameters) still take priority.
#[test]
fn test_resolve_infra_config_user_file_layer() {
    let _xdg_guard = xdg_lock();
    let global_dir = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(global_dir.path().join("wf")).unwrap();
    std::fs::create_dir_all(project_dir.path().join(".wf")).unwrap();
    std::env::set_var("XDG_CONFIG_HOME", global_dir.path());
    std::fs::write(
        global_dir.path().join("wf").join("config.toml"),
        r#"
log_level = "debug"

[storage]
type = "memory"
"#,
    )
    .unwrap();
    std::fs::write(
        project_dir.path().join(".wf").join("config.toml"),
        r#"
[storage]
type = "sqlite"

[storage.sqlite]
db_path = "./layer-test.db"
"#,
    )
    .unwrap();

    // Project layer wins over the global layer for storage; the unset
    // log level falls through to the global layer.
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let infra = InfraSourceConfig {
        project_root: Some(project_dir.path().to_path_buf()),
        allow_ambient: true,
        ..Default::default()
    };
    let config = resolve_with_file_layer(RuntimeConfig::default(), &infra, &runtime);
    assert_eq!(config.storage.storage_type, StorageType::Sqlite);
    assert_eq!(config.log_config.level, Some(LogLevel::Debug));

    // CLI log options win over both file layers; infrastructure files keep
    // their own resolution chain.
    let config = resolve_with_file_layer(
        RuntimeConfig {
            log_config: LogConfig::default().with_level(LogLevel::Off),
            ..Default::default()
        },
        &infra,
        &runtime,
    );
    assert_eq!(config.log_config.level, Some(LogLevel::Off));
    assert_eq!(config.storage.storage_type, StorageType::Sqlite);

    std::env::remove_var("XDG_CONFIG_HOME");
}

/// Services keep ambient disabled by default: the XDG global file is
/// ignored while the project file still applies, and the credential store
/// resolves next to the project instead of the daemon home directory.
#[test]
fn test_resolve_infra_config_ambient_disabled_by_default() {
    let _xdg_guard = xdg_lock();
    let global_dir = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(global_dir.path().join("wf")).unwrap();
    std::fs::create_dir_all(project_dir.path().join(".wf")).unwrap();
    std::env::set_var("XDG_CONFIG_HOME", global_dir.path());
    std::fs::write(
        global_dir.path().join("wf").join("config.toml"),
        "log_level = \"debug\"\n",
    )
    .unwrap();
    std::fs::write(
        project_dir.path().join(".wf").join("config.toml"),
        "log_level = \"info\"\n",
    )
    .unwrap();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let infra = InfraSourceConfig {
        project_root: Some(project_dir.path().to_path_buf()),
        ..Default::default()
    };
    let config = resolve_with_file_layer(RuntimeConfig::default(), &infra, &runtime);
    assert_eq!(config.log_config.level, Some(LogLevel::Info));
    assert_eq!(
        config.llm.auth.store_path,
        Some(
            project_dir
                .path()
                .join(".wf")
                .join("auth.json")
                .to_string_lossy()
                .into_owned()
        )
    );

    std::env::remove_var("XDG_CONFIG_HOME");
}

/// File profiles and explicit host profiles share one id space; explicit
/// values win while file-only records remain available.
#[test]
fn test_llm_profiles_merge_with_programmatic_values_by_id() {
    let settings_dir = tempfile::tempdir().unwrap();
    let profiles = settings_dir.path().join("llm-profiles");
    std::fs::create_dir_all(&profiles).unwrap();
    std::fs::write(
        profiles.join("a.toml"),
        "id = \"a\"\nname = \"a\"\nformat = \"openai_chat\"\nmodel = \"file-a\"\n",
    )
    .unwrap();
    std::fs::write(
        profiles.join("b.toml"),
        "id = \"b\"\nname = \"b\"\nformat = \"openai_chat\"\nmodel = \"file-b\"\n",
    )
    .unwrap();

    let explicit: wf_types::llm::LlmProfile = wf_config::parser::parse_toml(
        "id = \"a\"\nname = \"a\"\nformat = \"openai_chat\"\nmodel = \"explicit-a\"\n",
    )
    .unwrap();
    let base = RuntimeConfig {
        llm: LlmConfig {
            profiles: vec![explicit],
            ..Default::default()
        },
        ..Default::default()
    };
    let infra = InfraSourceConfig {
        settings_dir: Some(settings_dir.path().to_path_buf()),
        ..Default::default()
    };
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let config = resolve_with_file_layer(base, &infra, &runtime);

    assert_eq!(config.llm.profiles.len(), 2);
    let models: std::collections::BTreeMap<_, _> = config
        .llm
        .profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile.model.as_str()))
        .collect();
    assert_eq!(models["a"], "explicit-a");
    assert_eq!(models["b"], "file-b");
}

/// Invalid LLM declarations fail bootstrap instead of leaving a missing
/// reference for a later tool call to discover.
#[test]
fn test_resolve_infra_config_rejects_invalid_llm_profile() {
    let project_dir = tempfile::tempdir().unwrap();
    let profiles = project_dir.path().join(".wf").join("llm-profiles");
    std::fs::create_dir_all(&profiles).unwrap();
    std::fs::write(
        profiles.join("bad.profile.toml"),
        "id = \"bad\"\nname = \"Bad\"\nformat = \"OPENAI_CHAT\"\nmodel = \"m\"\napi_key = \"!cmd\"\n",
    )
    .unwrap();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let infra = InfraSourceConfig {
        project_root: Some(project_dir.path().to_path_buf()),
        ..Default::default()
    };
    let layer = load_infra_file_layer(&infra);
    let result = runtime.block_on(resolve_infra_config(
        RuntimeConfig::default(),
        &infra,
        layer.config,
        None,
    ));
    assert!(result.is_err());
}

/// An explicitly closed file channel delivered through programmatic
/// overrides keeps its value even though it reads identical to the type
/// default.
#[test]
fn test_resolve_infra_config_explicit_close_survives() {
    use wf_types::config::file_checkpoint::FileCheckpointConfig;
    std::env::remove_var("WF_FILE_CHECKPOINT_ENABLED");
    std::env::remove_var("WF_FILE_CHECKPOINT_MANUAL_POLL_MS");

    let project_dir = tempfile::tempdir().unwrap();
    let infra_dir = project_dir.path().join(".wf").join("infrastructure");
    std::fs::create_dir_all(&infra_dir).unwrap();
    std::fs::write(
        infra_dir.join("file-checkpoint.toml"),
        "enabled = true\nworkspace_root = \"/repo\"\n",
    )
    .unwrap();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let closed_source = || InfraSourceConfig {
        project_root: Some(project_dir.path().to_path_buf()),
        overrides: wf_config::orchestrator::ConfigOverrides {
            file_checkpoint: Some(FileCheckpointConfig::default()),
            ..Default::default()
        },
        ..Default::default()
    };
    let infra = closed_source();
    let config = resolve_with_file_layer(RuntimeConfig::default(), &infra, &runtime);
    assert!(!config.file_checkpoint.as_ref().unwrap().enabled);

    let enabled_source = InfraSourceConfig {
        overrides: wf_config::orchestrator::ConfigOverrides {
            file_checkpoint: Some(FileCheckpointConfig {
                enabled: true,
                workspace_root: Some("/repo".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..infra.clone()
    };
    let config = resolve_with_file_layer(RuntimeConfig::default(), &enabled_source, &runtime);
    assert!(config.file_checkpoint.as_ref().unwrap().enabled);
}
