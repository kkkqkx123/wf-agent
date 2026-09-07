//! Mini panels showcase: demonstrates every panel type available in
//! `wf --mini` (command palette, model, skill, queued, workflow, mention)
//! with synthetic data — no runtime, no TTY. Renders each panel to plain
//! text so the full rendering surface is reviewable and diffable.
//!
//! Run with: `cargo run -p wf-cli --example mini_panels`

use wf_api::workflow::summary::WorkflowSummary;
use wf_cli::keymap::KeyAction;
use wf_cli::panels::{
    CommandPalette, MentionPanel, ModelPanel, QueuedPanel, SkillPanel, WorkflowPanel,
};
use wf_cli::queue::QueuedPrompt;
use wf_types::llm::{LlmProfile, LlmProvider};
use wf_types::skill::SkillMetadata;

fn profile(id: &str, model: &str, name: &str) -> LlmProfile {
    LlmProfile {
        id: id.into(),
        name: name.into(),
        provider: LlmProvider::Anthropic,
        model: model.into(),
        api_key: None,
        base_url: None,
        parameters: None,
        generation: None,
        timeout: None,
        max_retries: None,
        retry_delay: None,
        headers: None,
        metadata: None,
        tool_call_format: None,
        auth_type: None,
        custom_headers: None,
        custom_body: None,
        custom_body_enabled: None,
        query_params: None,
        stream_options: None,
        context_window_size: None,
    }
}

fn skill(name: &str, desc: &str) -> SkillMetadata {
    SkillMetadata {
        name: name.into(),
        description: desc.into(),
        when_to_use: None,
        version: None,
        license: None,
        allowed_tools: None,
        metadata: None,
    }
}

fn workflow(id: &str, name: &str, nodes: usize, edges: usize) -> WorkflowSummary {
    WorkflowSummary {
        id: id.into(),
        name: name.into(),
        description: Some(format!("{nodes} nodes, {edges} edges")),
        version: None,
        node_count: nodes,
        edge_count: edges,
        updated_at: 0,
    }
}

fn render_section(
    title: &str,
    width: u16,
    window: u16,
    render: impl FnOnce(u16, u16) -> Vec<ratatui::text::Line<'static>>,
) {
    println!("== {title} ==");
    let lines = render(width, window);
    for line in &lines {
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        if !text.is_empty() {
            println!("  {text}");
        }
    }
    println!();
}

fn main() {
    let width = 80u16;
    let window = 10u16;

    println!("=== mini panels showcase ===");
    println!();

    // 1. Command palette: full list + filtered view.
    let palette = CommandPalette::new();
    render_section("command palette (full)", width, window, |w, h| {
        palette.render_lines(w, h)
    });
    println!("  commands: {} total", palette.visible_len());
    println!();

    let mut filtered = CommandPalette::new();
    filtered.filter_push('m');
    filtered.filter_push('o');
    render_section("command palette (filter: 'mo')", width, window, |w, h| {
        filtered.render_lines(w, h)
    });
    println!("  filtered: {} commands", filtered.visible_len());
    println!("  selected: {:?}", filtered.selected_command());
    println!();

    let mut filtered2 = CommandPalette::new();
    filtered2.filter_push('q');
    render_section("command palette (filter: 'q')", width, window, |w, h| {
        filtered2.render_lines(w, h)
    });
    println!("  filtered: {} commands", filtered2.visible_len());
    println!("  selected: {:?}", filtered2.selected_command());
    println!();

    // 2. Model panel: multiple profiles with current marking.
    let profiles = vec![
        profile("default", "claude-3.5-sonnet", "Default"),
        profile("fast", "gpt-4o-mini", "Fast"),
        profile("reasoning", "o1-preview", "Reasoning"),
        profile("local", "llama-3.1-70b", "Local"),
    ];
    let model_panel = ModelPanel::new(&profiles, Some("default"));
    render_section("model panel (current: default)", width, window, |w, h| {
        model_panel.render_lines(w, h)
    });
    println!("  selected: {:?}", model_panel.selected_model());
    println!("  on_current: {}", model_panel.on_current());
    println!();

    let mut model_nav = model_panel.clone();
    model_nav.handle(KeyAction::MoveNext);
    model_nav.handle(KeyAction::MoveNext);
    render_section("model panel (moved to index 2)", width, window, |w, h| {
        model_nav.render_lines(w, h)
    });
    println!("  selected: {:?}", model_nav.selected_model());
    println!("  on_current: {}", model_nav.on_current());
    println!("  position: {}", model_nav.position_string());
    println!();

    // 3. Skill panel: various skill types.
    let skills = vec![
        skill("pdf", "Parse and extract text from PDF files"),
        skill("xlsx", "Read and write Excel spreadsheets"),
        skill("web-search", "Search the web for information"),
        skill("code-review", "Review code for quality and issues"),
        skill("data-analysis", "Analyze datasets and produce insights"),
    ];
    let skill_panel = SkillPanel::new(&skills);
    render_section("skill panel (5 skills)", width, window, |w, h| {
        skill_panel.render_lines(w, h)
    });
    println!("  selected: {:?}", skill_panel.selected_skill());
    println!("  position: {}", skill_panel.position_string());
    println!();

    let mut skill_nav = skill_panel.clone();
    skill_nav.handle(KeyAction::MoveNext);
    skill_nav.handle(KeyAction::MoveNext);
    render_section("skill panel (moved to index 2)", width, window, |w, h| {
        skill_nav.render_lines(w, h)
    });
    println!("  selected: {:?}", skill_nav.selected_skill());
    println!();

    // 4. Workflow panel: various workflows.
    let workflows = vec![
        workflow("wf-code-review", "Code Review Pipeline", 5, 4),
        workflow("wf-doc-gen", "Documentation Generator", 3, 2),
        workflow("wf-test-suite", "Test Suite Runner", 8, 7),
        workflow("wf-deploy", "Deployment Pipeline", 6, 5),
    ];
    let wf_panel = WorkflowPanel::new(&workflows);
    render_section("workflow panel (4 workflows)", width, window, |w, h| {
        wf_panel.render_lines(w, h)
    });
    println!("  selected: {:?}", wf_panel.selected_workflow());
    println!("  position: {}", wf_panel.position_string());
    println!();

    // 5. Queued panel: queued prompts.
    let queued = vec![
        QueuedPrompt {
            id: 1,
            text: "Summarize the project README".into(),
        },
        QueuedPrompt {
            id: 2,
            text: "Run the test suite and report failures".into(),
        },
        QueuedPrompt {
            id: 3,
            text: "Generate a changelog from recent commits".into(),
        },
    ];
    let queued_panel = QueuedPanel::new(&queued);
    render_section("queued panel (3 prompts)", width, window, |w, h| {
        queued_panel.render_lines(w, h)
    });
    println!("  selected id: {:?}", queued_panel.selected_id());
    println!("  position: {}", queued_panel.position_string());
    println!();

    // 6. Mention panel: combined files + skills + workflows.
    let files = vec![
        "src/main.rs".into(),
        "src/lib.rs".into(),
        "Cargo.toml".into(),
        "README.md".into(),
        "docs/architecture.md".into(),
    ];
    let mention_panel = MentionPanel::new(&files, &skills, &workflows, None);
    render_section("mention panel (no filter)", width, window, |w, h| {
        mention_panel.render_lines(w, h)
    });
    println!("  candidates: {}", mention_panel.len());
    println!("  selected: {:?}", mention_panel.selected_candidate());
    println!();

    let mention_filtered = MentionPanel::new(&files, &skills, &workflows, Some("pdf"));
    render_section("mention panel (filter: 'pdf')", width, window, |w, h| {
        mention_filtered.render_lines(w, h)
    });
    println!("  candidates: {}", mention_filtered.len());
    println!("  selected: {:?}", mention_filtered.selected_candidate());
    println!();

    let mention_skill = MentionPanel::new(&files, &skills, &workflows, Some("skill:"));
    render_section("mention panel (filter: 'skill:')", width, window, |w, h| {
        mention_skill.render_lines(w, h)
    });
    println!("  candidates: {}", mention_skill.len());
    println!();

    // 7. Direct command lookup.
    println!("== direct command lookup ==");
    let palette = CommandPalette::new();
    for input in &["/model", "/skills", "/quit", "/help", "/new", "/unknown"] {
        match palette.find(input) {
            Some(cmd) => println!("  '{input}' -> {:?}", cmd),
            None => println!("  '{input}' -> None"),
        }
    }
    println!();

    // 8. Summary.
    println!("=== summary ===");
    println!("panels demonstrated:");
    println!(
        "  - command palette: {} built-in commands, filtering, navigation",
        palette.visible_len()
    );
    println!(
        "  - model panel: {} profiles, current marking, navigation",
        profiles.len()
    );
    println!("  - skill panel: {} skills, navigation", skills.len());
    println!(
        "  - workflow panel: {} workflows, navigation",
        workflows.len()
    );
    println!("  - queued panel: {} prompts, id tracking", queued.len());
    println!(
        "  - mention panel: {} files + {} skills + {} workflows, filter",
        files.len(),
        skills.len(),
        workflows.len()
    );
}
