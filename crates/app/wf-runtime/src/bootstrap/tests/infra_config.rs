use super::*;

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
    std::fs::write(
        project_infra.join("output.toml"),
        "dir = \"./project-out\"\n",
    )
    .unwrap();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let config = runtime
        .block_on(resolve_infra_config(
            RuntimeConfig::default(),
            &InfraSourceConfig {
                project_root: Some(project_dir.path().to_path_buf()),
                settings_dir: Some(settings_dir.path().to_path_buf()),
                ..Default::default()
            },
            None,
        ))
        .unwrap();

    // Project layer wins over the settings layer.
    assert_eq!(
        config.storage.sqlite.as_ref().unwrap().db_path,
        "./project.db"
    );
    assert_eq!(config.output.dir, "./project-out");
    assert_ne!(config.storage, StorageConfig::default());
    assert!(config.metrics.is_some());

    // MCP/skills sources are inherited.
    assert_eq!(
        config.mcp.project_root,
        Some(project_dir.path().to_path_buf())
    );

    // Programmatic values win over the file layer.
    let programmatic = RuntimeConfig {
        metrics: Some(MetricsConfig::default()),
        ..Default::default()
    };
    let config = runtime
        .block_on(resolve_infra_config(
            programmatic,
            &InfraSourceConfig {
                project_root: Some(project_dir.path().to_path_buf()),
                settings_dir: Some(settings_dir.path().to_path_buf()),
                ..Default::default()
            },
            None,
        ))
        .unwrap();
    assert!(config.metrics.is_some());
    // storage was left default -> still filled from the file layer.
    assert_ne!(config.storage, StorageConfig::default());
}

/// The workstation file layer (explicit ambient opt-in: global XDG file
/// then project `.wf/config.toml`, project wins) fills unset fields;
/// explicit programmatic values (CLI parameters) still take priority.
#[test]
fn test_resolve_infra_config_user_file_layer() {
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
    let config = runtime.block_on(resolve_infra_config(
        RuntimeConfig::default(),
        &InfraSourceConfig {
            project_root: Some(project_dir.path().to_path_buf()),
            allow_ambient: true,
            ..Default::default()
        },
        None,
    ));
    let config = config.unwrap();
    assert_eq!(config.storage.storage_type, StorageType::Sqlite);
    assert_eq!(config.log_config.level, "debug");

    // Programmatic values (CLI parameters) win over both file layers.
    let config = runtime.block_on(resolve_infra_config(
        RuntimeConfig {
            log_config: LogConfig::default().with_level("off"),
            ..Default::default()
        },
        &InfraSourceConfig {
            project_root: Some(project_dir.path().to_path_buf()),
            allow_ambient: true,
            ..Default::default()
        },
        None,
    ));
    let config = config.unwrap();
    assert_eq!(config.log_config.level, "off");
    assert_eq!(config.storage.storage_type, StorageType::Sqlite);

    std::env::remove_var("XDG_CONFIG_HOME");
}

/// Services keep ambient disabled by default: the XDG global file is
/// ignored while the project file still applies, and the credential store
/// resolves next to the project instead of the daemon home directory.
#[test]
fn test_resolve_infra_config_ambient_disabled_by_default() {
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
    let config = runtime
        .block_on(resolve_infra_config(
            RuntimeConfig::default(),
            &InfraSourceConfig {
                project_root: Some(project_dir.path().to_path_buf()),
                ..Default::default()
            },
            None,
        ))
        .unwrap();
    assert_eq!(config.log_config.level, "info");
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

/// An explicitly closed file channel delivered through programmatic
/// overrides keeps its value even though it reads identical to the type
/// default, and overrides outrank a non-default programmatic value.
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
    let config = runtime
        .block_on(resolve_infra_config(
            RuntimeConfig::default(),
            &closed_source(),
            None,
        ))
        .unwrap();
    assert!(!config.file_checkpoint.as_ref().unwrap().enabled);

    let programmatic = RuntimeConfig {
        file_checkpoint: Some(FileCheckpointConfig {
            enabled: true,
            workspace_root: Some("/repo".to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let config = runtime
        .block_on(resolve_infra_config(programmatic, &closed_source(), None))
        .unwrap();
    assert!(!config.file_checkpoint.as_ref().unwrap().enabled);
}
