use super::*;

#[tokio::test]
async fn test_runtime_keyword_memory_bootstrap_shares_singleton() {
    clear_env_vars();

    let runtime = Runtime::bootstrap(default_test_config()).await.unwrap();
    assert!(
        runtime.keyword_index().is_some(),
        "memory bootstrap opens the ephemeral keyword index"
    );
    assert!(
        runtime.api_context().keyword_index().is_some(),
        "api context observes the bootstrap singleton"
    );
    assert!(
        format!("{:?}", runtime.keyword_index().expect("keyword")).contains("sqlite-fts"),
        "singleton must report the sqlite backend"
    );

    runtime.shutdown().await.unwrap();
    clear_env_vars();
}

#[tokio::test]
async fn test_runtime_keyword_disabled_leaves_empty_slot() {
    clear_env_vars();

    let mut config = default_test_config();
    config.keyword.enabled = false;
    let runtime = Runtime::bootstrap(config).await.unwrap();
    assert!(runtime.keyword_index().is_none());
    assert!(runtime.api_context().keyword_index().is_none());

    runtime.shutdown().await.unwrap();
    clear_env_vars();
}

#[tokio::test]
async fn test_runtime_keyword_file_persists_next_to_storage() {
    clear_env_vars();

    let dir = std::env::temp_dir().join(format!("wf-keyword-runtime-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("app.db");
    let storage = StorageConfig {
        storage_type: StorageType::Sqlite,
        sqlite: Some(wf_types::config::storage::SqliteStorageConfig {
            db_path: db_path.to_string_lossy().into_owned(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let expected_keyword = wf_api::rag::keyword_db_path(&storage);
    let mut config = default_test_config();
    config.storage = storage;
    let runtime = Runtime::bootstrap(config).await.unwrap();
    assert!(runtime.keyword_index().is_some());
    assert!(expected_keyword.exists(), "keyword db file must exist");
    runtime.shutdown().await.unwrap();
    assert!(expected_keyword.exists(), "keyword db survives restart");
    let _ = std::fs::remove_dir_all(&dir);
    clear_env_vars();
}

#[tokio::test]
async fn test_runtime_keyword_illegal_path_fails_fast() {
    clear_env_vars();

    let dir = std::env::temp_dir().join(format!("wf-keyword-runtime-fail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = default_test_config();
    config.keyword.db_path = Some(dir.to_string_lossy().into_owned());
    let err = match Runtime::bootstrap(config).await {
        Ok(_) => panic!("keyword open failure must fail bootstrap"),
        Err(e) => e,
    };
    assert!(
        err.to_string().to_lowercase().contains("keyword index"),
        "error must name the keyword index: {err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
    clear_env_vars();
}

#[tokio::test]
async fn test_runtime_keyword_tantivy_without_feature_fails_fast() {
    clear_env_vars();

    let mut config = default_test_config();
    config.keyword.backend = wf_types::config::keyword::KeywordBackend::Tantivy;
    let err = match Runtime::bootstrap(config).await {
        Ok(runtime) => {
            runtime.shutdown().await.unwrap();
            panic!("tantivy without the feature must fail bootstrap");
        }
        Err(e) => e,
    };
    assert!(
        err.to_string().to_lowercase().contains("keyword"),
        "tantivy open failure must name the keyword index: {err}"
    );
    clear_env_vars();
}

#[tokio::test]
async fn test_runtime_keyword_elastic_without_url_fails_fast() {
    clear_env_vars();

    let mut config = default_test_config();
    config.keyword.backend = wf_types::config::keyword::KeywordBackend::Elastic;
    config.keyword.elastic_url = None;
    let err = match Runtime::bootstrap(config).await {
        Ok(_) => panic!("elastic without a URL must fail bootstrap"),
        Err(e) => e,
    };
    assert!(
        err.to_string().to_lowercase().contains("keyword"),
        "elastic open failure must name the keyword index: {err}"
    );
    clear_env_vars();
}

#[tokio::test]
async fn test_runtime_keyword_elastic_unreachable_fails_fast() {
    clear_env_vars();

    let mut config = default_test_config();
    config.keyword.backend = wf_types::config::keyword::KeywordBackend::Elastic;
    config.keyword.elastic_url = Some("http://127.0.0.1:19200".to_string());
    config.keyword.elastic_timeout_secs = 1;
    let err = match Runtime::bootstrap(config).await {
        Ok(_) => panic!("elastic with an unreachable server must fail bootstrap"),
        Err(e) => e,
    };
    assert!(
        err.to_string().to_lowercase().contains("keyword"),
        "elastic probe failure must name the keyword index: {err}"
    );
    clear_env_vars();
}
