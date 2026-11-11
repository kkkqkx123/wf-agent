use super::*;

#[test]
fn parse_workflow_input_empty_is_none() {
    assert_eq!(parse_workflow_input(None).unwrap(), None);
    assert_eq!(parse_workflow_input(Some("")).unwrap(), None);
    assert_eq!(parse_workflow_input(Some("  ")).unwrap(), None);
}

#[test]
fn parse_workflow_input_valid_json() {
    let v = parse_workflow_input(Some(r#"{"a":1}"#)).unwrap().unwrap();
    assert_eq!(v["a"], 1);
}

#[test]
fn parse_workflow_input_invalid_json_errors() {
    assert!(parse_workflow_input(Some("{bad")).is_err());
}

#[test]
fn parse_workflow_input_rejects_non_objects() {
    assert!(parse_workflow_input(Some(r#"[1,2]"#)).is_err());
    assert!(parse_workflow_input(Some(r#""text""#)).is_err());
    assert!(parse_workflow_input(Some("42")).is_err());
    assert!(parse_workflow_input(Some("true")).is_err());
    assert_eq!(
        parse_workflow_input(Some("{}")).unwrap().unwrap(),
        Value::Object(Default::default())
    );
    assert_eq!(parse_workflow_input(Some("null")).unwrap(), None);
}

#[test]
fn validate_workflow_input_value_gates_shapes() {
    assert!(validate_workflow_input_value(None).is_ok());
    assert!(validate_workflow_input_value(Some(&Value::Null)).is_ok());
    assert!(validate_workflow_input_value(Some(&serde_json::json!({"a": 1}))).is_ok());
    assert!(validate_workflow_input_value(Some(&serde_json::json!([1]))).is_err());
    assert!(validate_workflow_input_value(Some(&serde_json::json!("x"))).is_err());
}

#[test]
fn agent_body_carries_prompt_and_conversation() {
    let params = TurnParams {
        agent: Some("ag".into()),
        model: Some("m".into()),
        approve_prefixes: vec![],
        timeout_ms: Some(5000),
        context: Default::default(),
        conversation: Vec::new(),
        kind: TurnKind::Agent {
            prompt: "hi".to_string(),
            images: Vec::new(),
            attachments: Vec::new(),
        },
    };
    let body = params.agent_request_body();
    assert_eq!(body["agent_id"], "ag");
    assert_eq!(body["model"], "m");
    assert_eq!(body["message"], "hi");
    assert_eq!(body["max_execution_time"], 5000);
    assert!(body.get("max_iterations").is_none());
}

#[test]
fn agent_body_passes_raw_text_for_server_wash() {
    let params = TurnParams {
        agent: None,
        model: None,
        approve_prefixes: vec![],
        timeout_ms: None,
        context: Default::default(),
        conversation: Vec::new(),
        kind: TurnKind::Agent {
            prompt: "\x1b[31mhi\x1b[0m".to_string(),
            images: Vec::new(),
            attachments: Vec::new(),
        },
    };
    let body = params.agent_request_body();
    assert_eq!(body["message"], "\x1b[31mhi\x1b[0m");
    assert_eq!(body["model"], "");
}

#[test]
fn agent_body_carries_context() {
    let params = TurnParams {
        agent: None,
        model: None,
        approve_prefixes: vec![],
        timeout_ms: None,
        context: parse_agent_context(Some(r#"{"city": "paris"}"#), None).expect("context parses"),
        conversation: Vec::new(),
        kind: TurnKind::Agent {
            prompt: "hi".to_string(),
            images: Vec::new(),
            attachments: Vec::new(),
        },
    };
    let body = params.agent_request_body();
    assert_eq!(body["context"]["city"], "paris");
}

#[test]
fn agent_body_carries_images_as_structured_payload() {
    let params = TurnParams {
        agent: None,
        model: None,
        approve_prefixes: vec![],
        timeout_ms: None,
        context: Default::default(),
        conversation: Vec::new(),
        kind: TurnKind::Agent {
            prompt: String::new(),
            images: vec![wf_types::message::UserImageInput::remote(
                "https://example.com/shot.png",
            )],
            attachments: Vec::new(),
        },
    };
    let body = params.agent_request_body();
    let images = body["images"].as_array().expect("images array");
    assert_eq!(images.len(), 1);
    assert_eq!(images[0]["url"], "https://example.com/shot.png");
}

#[test]
fn image_files_load_with_media_detection() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("wf-image-{}.png", std::process::id()));
    std::fs::write(&path, [137u8, 80, 78, 71]).expect("fixture");
    let spec = path.to_string_lossy().to_string();
    let images = load_user_image_specs(std::slice::from_ref(&spec), None).expect("image loads");
    assert_eq!(images.len(), 1);
    assert_eq!(
        images[0].media_type.as_deref(),
        Some("image/png"),
        "media type follows the extension"
    );
    let _ = std::fs::remove_file(&path);
    let missing = dir.join("missing.png").to_string_lossy().to_string();
    assert!(load_user_image_specs(std::slice::from_ref(&missing), None).is_err());
    let text = dir.join("notes.txt").to_string_lossy().to_string();
    assert!(load_user_image_specs(std::slice::from_ref(&text), None).is_err());
}

#[test]
fn image_specs_accept_remote_urls_with_detail() {
    let detail = parse_image_detail_arg(Some("high")).expect("detail parses");
    let images = load_user_image_specs(&["https://example.com/shot.png".to_string()], detail)
        .expect("remote loads");
    assert_eq!(images.len(), 1);
    assert_eq!(
        images[0].url.as_deref(),
        Some("https://example.com/shot.png")
    );
    assert!(parse_image_detail_arg(None).expect("empty").is_none());
    assert!(parse_image_detail_arg(Some("nope")).is_err());
    assert!(!is_remote_image_spec("shot.png"));
    assert!(is_remote_image_spec("https://example.com/a.png"));
    assert!(load_user_image_specs(&["ftp://x/y.png".to_string()], None).is_err());
}

#[test]
fn attachment_files_load_truncate_and_reject_binary() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("wf-attach-{}.txt", std::process::id()));
    std::fs::write(&path, "hello").expect("fixture");
    let loaded = load_user_attachment_specs(std::slice::from_ref(&path)).expect("attach loads");
    assert_eq!(loaded.len(), 1);
    assert_eq!(
        loaded[0].name,
        path.file_name().expect("name").to_string_lossy()
    );
    assert!(!loaded[0].truncated);
    assert_eq!(
        describe_user_attachment(&loaded[0]),
        format!("{} (5 chars)", loaded[0].name)
    );
    let _ = std::fs::remove_file(&path);
    let big_path = dir.join(format!("wf-attach-big-{}.txt", std::process::id()));
    let big = "x".repeat(wf_types::message::MAX_TEXT_ATTACHMENT_CHARS + 10);
    std::fs::write(&big_path, &big).expect("fixture");
    let loaded = load_user_attachment_specs(std::slice::from_ref(&big_path)).expect("truncates");
    assert!(loaded[0].truncated);
    assert_eq!(loaded[0].original_chars, Some(big.chars().count()));
    let _ = std::fs::remove_file(&big_path);
    let bin_path = dir.join(format!("wf-attach-bin-{}.txt", std::process::id()));
    std::fs::write(&bin_path, [b'a', 0, b'b']).expect("fixture");
    assert!(load_user_attachment_specs(std::slice::from_ref(&bin_path)).is_err());
    let _ = std::fs::remove_file(&bin_path);
}

#[test]
fn agent_body_carries_attachments_as_structured_payload() {
    let params = TurnParams {
        agent: None,
        model: None,
        approve_prefixes: vec![],
        timeout_ms: None,
        context: Default::default(),
        conversation: Vec::new(),
        kind: TurnKind::Agent {
            prompt: String::new(),
            images: Vec::new(),
            attachments: vec![wf_types::message::UserTextAttachment {
                name: "a.txt".to_string(),
                content: "hello".to_string(),
                truncated: false,
                original_chars: None,
            }],
        },
    };
    let body = params.agent_request_body();
    let attachments = body["attachments"].as_array().expect("attachments array");
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0]["name"], "a.txt");
}

#[test]
fn stage_budget_line_reports_counts_and_payloads() {
    let line = stage_budget_line(&[], &[]);
    assert!(line.contains("budget: 0/8 images"), "{line}");
    assert!(line.contains("0/8 files"), "{line}");
    let images = vec![wf_types::message::UserImageInput::remote(
        "https://example.com/shot.png",
    )];
    let line = stage_budget_line(&images, &[]);
    assert!(line.contains("budget: 1/8 images"), "{line}");
    assert!(line.contains("1 remote"), "{line}");
}

#[test]
fn workflow_var_args_parse_and_merge_with_priority() {
    let vars = parse_workflow_var_args(&[
        "city=paris".to_string(),
        "count=42".to_string(),
        "raw=hello world".to_string(),
    ])
    .expect("vars parse");
    assert_eq!(vars["city"], serde_json::json!("paris"));
    assert_eq!(vars["count"], serde_json::json!(42));
    let merged = merge_workflow_inputs(
        Some(serde_json::json!({"city": "london", "other": 1})),
        Some(serde_json::json!({"city": "berlin"})),
        &vars,
    )
    .expect("merged");
    assert_eq!(merged["city"], serde_json::json!("paris"));
    assert_eq!(merged["count"], serde_json::json!(42));
    assert_eq!(merged["other"], serde_json::json!(1));
    assert!(parse_workflow_var_arg("novalue").is_err());
    assert!(parse_workflow_var_arg("=x").is_err());
}

#[test]
fn loose_values_keep_json_shapes_with_string_fallback() {
    assert_eq!(parse_loose_value("42"), serde_json::json!(42));
    assert_eq!(parse_loose_value("true"), serde_json::json!(true));
    assert_eq!(parse_loose_value("{\"a\": 1}"), serde_json::json!({"a": 1}));
    assert_eq!(parse_loose_value("paris"), serde_json::json!("paris"));
    assert_eq!(
        parse_loose_value("  hello world  "),
        serde_json::json!("hello world")
    );
}

#[test]
fn context_key_values_check_paths_and_parse_values() {
    let (key, value) = parse_context_key_value("city", "paris").expect("pair parses");
    assert_eq!(key, "city");
    assert_eq!(value, serde_json::json!("paris"));
    let (key, value) = parse_context_key_value("count", "42").expect("pair parses");
    assert_eq!(key, "count");
    assert_eq!(value, serde_json::json!(42));
    let (key, _) = parse_context_key_value("  user.name  ", "x").expect("pair parses");
    assert_eq!(key, "user.name");
    assert!(parse_context_key_value("", "x").is_err());
    assert!(parse_context_key_value("bad key", "x").is_err());
}

#[test]
fn workflow_input_file_and_resolve_merge() {
    let dir = std::env::temp_dir();
    let file = dir.join(format!("wf-input-{}.json", std::process::id()));
    std::fs::write(&file, r#"{"a": 1, "b": 2}"#).expect("fixture");
    let loaded = load_workflow_input_file(Some(&file))
        .expect("file loads")
        .expect("object");
    assert_eq!(loaded["a"], 1);
    let merged = resolve_workflow_input(Some(r#"{"b": 3}"#), Some(&file), &["c=4".to_string()])
        .expect("resolve")
        .expect("merged");
    assert_eq!(merged["a"], 1);
    assert_eq!(merged["b"], 3);
    assert_eq!(merged["c"], 4);
    let _ = std::fs::remove_file(&file);
}

#[test]
fn agent_context_rejects_non_objects_and_merges_file() {
    assert!(parse_agent_context(Some("[1]"), None).is_err());
    assert!(parse_agent_context(Some("42"), None).is_err());
    assert!(parse_agent_context(None, None).expect("empty").is_empty());
    assert!(parse_agent_context(Some("  "), None)
        .expect("blank")
        .is_empty());
    let dir = std::env::temp_dir();
    let file = dir.join(format!("wf-context-{}.json", std::process::id()));
    std::fs::write(&file, r#"{"a": 1, "b": 2}"#).expect("fixture");
    let merged = parse_agent_context(Some(r#"{"b": 3}"#), Some(&file)).expect("file plus inline");
    assert_eq!(merged["a"], 1);
    assert_eq!(merged["b"], 3);
    let _ = std::fs::remove_file(&file);
    assert!(parse_agent_context(None, Some(&file)).is_err());
}
