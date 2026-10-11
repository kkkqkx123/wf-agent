//! Skill loader tests: scanning, content loading, resources, events and
//! in-memory loader injection.

use super::parse::parse_yaml_frontmatter;
use super::*;

fn make_skill_dir(root: &Path, name: &str) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(dir.join("references")).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(
            dir.join("SKILL.md"),
            format!(
                "---\nname: {}\ndescription: Test skill\ndescription_extended: \"hello\"\nversion: 1.0.0\nallowedTools:\n- read_file\n- write_file\n---\n\n# Body\n\nContent here.",
                name
            ),
        )
        .unwrap();
    std::fs::write(dir.join("references").join("ref1.md"), "# Ref one").unwrap();
    std::fs::write(dir.join("scripts").join("run.py"), "print('hi')").unwrap();
    std::fs::write(dir.join("assets").join("logo.png"), vec![1u8, 2, 3, 4]).unwrap();
    dir
}

#[test]
fn test_scan_and_list_skills() {
    let root = std::env::temp_dir().join(format!("wf-skill-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    make_skill_dir(&root, "my-skill");

    let loader = SkillLoader::new(SkillConfig {
        paths: vec![root.to_string_lossy().to_string()],
        auto_scan: Some(true),
    });

    let skills = loader.list_skills();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "my-skill");
    assert_eq!(skills[0].version.as_deref(), Some("1.0.0"));
    assert_eq!(
        skills[0].allowed_tools.as_deref(),
        Some(vec!["read_file".to_string(), "write_file".to_string()].as_slice())
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_name_mismatch_rejected() {
    let root = std::env::temp_dir().join(format!("wf-skill-mismatch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let dir = root.join("other-name");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: real-name\ndescription: x\n---\n\nBody",
    )
    .unwrap();

    let loader = SkillLoader::new(SkillConfig {
        paths: vec![root.to_string_lossy().to_string()],
        auto_scan: Some(true),
    });

    assert_eq!(loader.list_skills().len(), 0);
    assert!(loader.load_skill_dir(&dir).is_err());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_load_content_strips_frontmatter() {
    let root = std::env::temp_dir().join(format!("wf-skill-content-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    make_skill_dir(&root, "my-skill");

    let loader = SkillLoader::new(SkillConfig {
        paths: vec![root.to_string_lossy().to_string()],
        auto_scan: Some(true),
    });

    let content = loader.load_content("my-skill").unwrap();
    assert!(!content.contains("---"));
    assert!(content.contains("Content here"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_load_skill_resource() {
    let root = std::env::temp_dir().join(format!("wf-skill-res-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    make_skill_dir(&root, "my-skill");

    let loader = SkillLoader::new(SkillConfig {
        paths: vec![root.to_string_lossy().to_string()],
        auto_scan: Some(true),
    });

    let paths = loader.list_skill_resources("my-skill", SkillResourceType::References);
    assert_eq!(paths, vec!["ref1.md".to_string()]);

    let text = loader
        .load_skill_resource("my-skill", SkillResourceType::References, "ref1.md")
        .unwrap();
    match text {
        ResourceContent::Text(t) => assert_eq!(t, "# Ref one"),
        ResourceContent::Binary(_) => panic!("expected text content"),
    }

    let scripts = loader.load_skill_resource("my-skill", SkillResourceType::Scripts, "run.py");
    assert!(scripts.is_ok());

    let binary = loader
        .load_skill_resource("my-skill", SkillResourceType::Assets, "logo.png")
        .unwrap();
    match binary {
        ResourceContent::Binary(b) => assert_eq!(b, vec![1u8, 2, 3, 4]),
        ResourceContent::Text(_) => panic!("expected binary content"),
    }

    assert!(loader
        .load_skill_resource("my-skill", SkillResourceType::References, "missing.md")
        .is_err());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_path_traversal_rejected() {
    let root = std::env::temp_dir().join(format!("wf-skill-trav-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    make_skill_dir(&root, "my-skill");

    let loader = SkillLoader::new(SkillConfig {
        paths: vec![root.to_string_lossy().to_string()],
        auto_scan: Some(true),
    });

    assert!(loader
        .load_skill_resource("my-skill", SkillResourceType::References, "../SKILL.md")
        .is_err());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_load_resources_all() {
    let root = std::env::temp_dir().join(format!("wf-skill-all-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    make_skill_dir(&root, "my-skill");

    let loader = SkillLoader::new(SkillConfig {
        paths: vec![root.to_string_lossy().to_string()],
        auto_scan: Some(true),
    });

    let all = loader
        .load_resources("my-skill", SkillResourceType::Scripts)
        .unwrap();
    assert_eq!(all.len(), 1);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_yaml_parsing() {
    let fields = parse_yaml_frontmatter(
        "name: test\nwhen_to_use: \"quoted\"\nversion: 2\nflag: true\nnothing: null\n",
    );
    assert_eq!(fields.get("name").and_then(|v| v.as_str()), Some("test"));
    assert_eq!(fields.get("version").and_then(|v| v.as_i64()), Some(2));
    assert_eq!(fields.get("flag").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(fields.get("nothing"), Some(&Value::Null));
    assert_eq!(
        fields.get("when_to_use").and_then(|v| v.as_str()),
        Some("quoted")
    );
}

#[test]
fn test_enable_disable_skill() {
    let root = std::env::temp_dir().join(format!("wf-skill-en-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    make_skill_dir(&root, "my-skill");

    let loader = SkillLoader::new(SkillConfig {
        paths: vec![root.to_string_lossy().to_string()],
        auto_scan: Some(true),
    });

    // Enabled by default.
    assert!(loader.is_skill_enabled("my-skill"));
    assert_eq!(loader.get_enabled_skills().len(), 1);
    assert!(loader.get_disabled_skills().is_empty());

    loader.disable_skill("my-skill").unwrap();
    assert!(!loader.is_skill_enabled("my-skill"));
    assert!(loader.load_content("my-skill").is_err());

    loader.enable_skill("my-skill").unwrap();
    assert!(loader.load_content("my-skill").is_ok());

    assert!(loader.enable_skill("missing").is_err());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_event_emission() {
    let root = std::env::temp_dir().join(format!("wf-skill-ev-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    make_skill_dir(&root, "my-skill");

    let loader = SkillLoader::new(SkillConfig {
        paths: vec![root.to_string_lossy().to_string()],
        auto_scan: Some(true),
    });

    let events: Arc<std::sync::Mutex<Vec<SkillEvent>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = events.clone();
    loader.set_event_handler(Arc::new(move |event| {
        captured.lock().unwrap().push(event.clone());
    }));

    // Load completed path emits started + completed.
    loader.load_content("my-skill").unwrap();
    // Load failed path (disabled) emits started + failed.
    loader.disable_skill("my-skill").unwrap();
    let _ = loader.load_content("my-skill");
    loader.enable_skill("my-skill").unwrap();
    loader.clear_cache_for("my-skill");
    loader.clear_cache();

    let got: Vec<SkillEvent> = wf_common::lock::lock_ok(events.lock()).clone();
    assert!(got.iter().any(|e| matches!(
        e,
        SkillEvent::LoadStarted { name } if name == "my-skill"
    )));
    assert!(got.iter().any(|e| matches!(
        e,
        SkillEvent::LoadCompleted { name } if name == "my-skill"
    )));
    assert!(got.iter().any(|e| matches!(
        e,
        SkillEvent::LoadFailed { name, .. } if name == "my-skill"
    )));
    assert!(got.iter().any(|e| matches!(
        e,
        SkillEvent::Enabled { name } if name == "my-skill"
    )));
    assert!(got.iter().any(|e| matches!(
        e,
        SkillEvent::Disabled { name } if name == "my-skill"
    )));
    assert!(got.iter().any(|e| matches!(
        e,
        SkillEvent::CacheCleared { name: Some(n) } if n == "my-skill"
    )));
    assert!(got
        .iter()
        .any(|e| matches!(e, SkillEvent::CacheCleared { name: None })));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_with_in_memory_loader_scan_and_load() {
    use crate::skill_fs::InMemorySkillLoader;

    let mut mem = InMemorySkillLoader::new();
    mem.add_file(
            Path::new("/skills/my-skill/SKILL.md"),
            "---\nname: my-skill\ndescription: Mem\ndescription_extended: \"x\"\nversion: 1.0.0\n---\n\n# Body\n\nHello {{name}}",
        );
    mem.add_file(Path::new("/skills/my-skill/references/ref1.md"), "# Ref");
    mem.add_file(Path::new("/skills/my-skill/scripts/run.py"), "print('hi')");
    mem.add_file(
        Path::new("/skills/other/SKILL.md"),
        "---\nname: other\ndescription: O\n---\n\nOther body",
    );

    let loader = SkillLoader::with_loader(
        SkillConfig {
            paths: vec!["/skills".to_string()],
            auto_scan: Some(true),
        },
        Arc::new(mem),
    );

    assert!(loader.has_skill("my-skill"));
    assert!(loader.has_skill("other"));
    let content = loader
        .load_skill_content(
            "my-skill",
            Some(&SkillLoadContext {
                variables: Some(HashMap::from([(
                    "name".to_string(),
                    Value::String("wf".to_string()),
                )])),
                tools: None,
            }),
        )
        .unwrap();
    assert!(content.contains("Hello wf"));
    assert!(content.contains("# Body"));
    // Resource listing came from the in-memory loader too.
    let resources = loader.list_skill_resources("my-skill", SkillResourceType::References);
    assert_eq!(resources, vec!["ref1.md".to_string()]);
    let scripts = loader.list_skill_resources("my-skill", SkillResourceType::Scripts);
    assert_eq!(scripts, vec!["run.py".to_string()]);
}

#[test]
fn test_in_memory_loader_error_injection() {
    use crate::skill_fs::InMemorySkillLoader;

    let mut mem = InMemorySkillLoader::new();
    mem.add_file(
        Path::new("/skills/fail-skill/SKILL.md"),
        "---\nname: fail-skill\ndescription: F\n---\n\nBody",
    );
    mem.fail_read(Path::new("/skills/fail-skill/SKILL.md"));

    let loader = SkillLoader::with_loader(
        SkillConfig {
            paths: vec!["/skills".to_string()],
            auto_scan: Some(true),
        },
        Arc::new(mem),
    );

    // The injected read failure surfaces as a load error.
    assert!(loader.load_content("fail-skill").is_err());

    // Reading a missing resource fails cleanly (no panic).
    let mut mem2 = InMemorySkillLoader::new();
    mem2.add_file(
        Path::new("/skills/r-skill/SKILL.md"),
        "---\nname: r-skill\ndescription: R\n---\n\nBody",
    );
    let loader2 = SkillLoader::with_loader(
        SkillConfig {
            paths: vec!["/skills".to_string()],
            auto_scan: Some(true),
        },
        Arc::new(mem2),
    );
    assert!(loader2
        .load_skill_resource("r-skill", SkillResourceType::References, "missing.md")
        .is_err());
}

#[test]
fn test_variable_substitution() {
    let root = std::env::temp_dir().join(format!("wf-skill-var-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("var-skill")).unwrap();
    std::fs::write(
            root.join("var-skill/SKILL.md"),
            "---\nname: var-skill\ndescription: Var\n---\n\nHello {{name}}, count={{count}} missing={{nope}}",
        )
        .unwrap();

    let loader = SkillLoader::new(SkillConfig {
        paths: vec![root.to_string_lossy().to_string()],
        auto_scan: Some(false),
    });
    let dir = root.join("var-skill");
    loader.load_skill_dir(&dir).unwrap();

    let mut variables = HashMap::new();
    variables.insert("name".to_string(), Value::String("world".into()));
    variables.insert("count".to_string(), Value::from(3));
    let context = SkillLoadContext {
        variables: Some(variables),
        tools: None,
    };
    let content = loader
        .load_skill_content("var-skill", Some(&context))
        .unwrap();
    assert!(content.contains("Hello world, count=3 missing="));

    // Without variables the placeholders remain.
    let content = loader.load_content("var-skill").unwrap();
    assert!(content.contains("{{name}}"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_allowed_tools_permission_check() {
    let root = std::env::temp_dir().join(format!("wf-skill-perm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("perm-skill")).unwrap();
    std::fs::write(
            root.join("perm-skill/SKILL.md"),
            "---\nname: perm-skill\ndescription: Perm\nallowedTools:\n- read_file\n- write_file\n---\n\nBody",
        )
        .unwrap();

    let loader = SkillLoader::new(SkillConfig {
        paths: vec![root.to_string_lossy().to_string()],
        auto_scan: Some(false),
    });
    let dir = root.join("perm-skill");
    loader.load_skill_dir(&dir).unwrap();

    // Missing tools → rejected.
    let denied = SkillLoadContext {
        variables: None,
        tools: Some(vec!["read_file".into()]),
    };
    assert!(loader
        .load_skill_content("perm-skill", Some(&denied))
        .is_err());

    // All tools present → allowed.
    let allowed = SkillLoadContext {
        variables: None,
        tools: Some(vec!["read_file".into(), "write_file".into()]),
    };
    assert!(loader
        .load_skill_content("perm-skill", Some(&allowed))
        .is_ok());

    // No context → permission skipped.
    assert!(loader.load_skill_content("perm-skill", None).is_ok());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_metadata_prompt_generation() {
    let skills = vec![
        SkillMetadata {
            name: "analyze-data".into(),
            description: "Analyze datasets".into(),
            when_to_use: None,
            version: Some("1.0.0".into()),
            license: None,
            allowed_tools: None,
            metadata: None,
        },
        SkillMetadata {
            name: "review".into(),
            description: "Review code".into(),
            when_to_use: None,
            version: None,
            license: None,
            allowed_tools: None,
            metadata: None,
        },
    ];

    let prompt = generate_skill_metadata_prompt(&skills);
    assert!(prompt.contains("Available skills:"));
    assert!(prompt.contains("analyze-data: Analyze datasets (v1.0.0)"));
    assert!(prompt.contains("review: Review code"));

    // Placeholder replacement.
    let injected = inject_skill_metadata("You are a coder.\n{SKILLS_METADATA}", &skills);
    assert!(!injected.contains("{SKILLS_METADATA}"));
    assert!(injected.contains("Available skills:"));

    // Append when no placeholder.
    let appended = inject_skill_metadata("You are a coder.", &skills);
    assert!(appended.contains("You are a coder."));
    assert!(appended.contains("Available skills:"));

    // No enabled skills → placeholder removed, prompt otherwise unchanged.
    let empty = inject_skill_metadata("Hi {SKILLS_METADATA}", &[]);
    assert_eq!(empty, "Hi ");
    assert!(inject_skill_metadata("Hi", &[]).contains("Hi"));
}
