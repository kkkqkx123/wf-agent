//! Fixture-driven showcase runner for `wf-tools` (feature `harness`).
//!
//! Usage:
//!   cargo run -p wf-tools --features harness --example tool_showcase -- --all
//!   cargo run -p wf-tools --features harness --example tool_showcase -- --tool read_file
//!   cargo run -p wf-tools --features harness --example tool_showcase -- --category filesystem
//!   cargo run -p wf-tools --features harness --example tool_showcase -- --tier process
//!   cargo run -p wf-tools --features harness --example tool_showcase -- --check
//!   cargo run -p wf-tools --features harness --example tool_showcase -- --list
//!
//! Without `--check` the runner writes normalized results to the crate-local
//! `outputs/` directory; `--check` compares the fresh results against the
//! committed outputs and exits non-zero on drift.

use std::path::PathBuf;
use std::process::ExitCode;

use wf_tools::harness::{tool_tier, Case, Runner, Tier};
use wf_tools::predefined;

#[tokio::main]
async fn main() -> ExitCode {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let runner = Runner::new(manifest.join("fixtures"), manifest.join("outputs"));

    let mut tool: Option<String> = None;
    let mut category: Option<String> = None;
    let mut tier: Option<Tier> = None;
    let mut check = false;
    let mut list = false;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--all" | "--update" => {}
            "--check" => check = true,
            "--list" => list = true,
            "--tool" => {
                i += 1;
                tool = args.get(i).cloned();
            }
            "--category" => {
                i += 1;
                category = args.get(i).cloned();
            }
            "--tier" => {
                i += 1;
                tier = args.get(i).and_then(|s| Tier::parse(s));
            }
            other => {
                eprintln!("unknown argument: {other}");
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }

    let cases = match runner.load_cases() {
        Ok(cases) => cases,
        Err(e) => {
            eprintln!("failed to load fixtures: {e}");
            return ExitCode::FAILURE;
        }
    };

    if list {
        for case in &cases {
            println!("{}\t{}", case.fixture.tool, case.name);
        }
        return ExitCode::SUCCESS;
    }

    let explicit = tool.is_some() || category.is_some() || tier.is_some();
    let selected: Vec<&Case> = cases
        .iter()
        .filter(|case| {
            let fixture_tool = case.fixture.tool.as_str();
            if let Some(wanted) = &tool {
                if fixture_tool != wanted {
                    return false;
                }
            }
            if let Some(wanted) = &category {
                let defined = predefined::all_definitions()
                    .into_iter()
                    .find(|d| d.id == fixture_tool)
                    .map(|d| d.category);
                if defined != Some(wanted.as_str()) {
                    return false;
                }
            }
            let case_tier = case
                .fixture
                .tier
                .as_deref()
                .and_then(Tier::parse)
                .unwrap_or_else(|| tool_tier(fixture_tool));
            if let Some(wanted) = tier {
                if case_tier != wanted {
                    return false;
                }
            } else if !explicit && !case_tier.default_enabled() {
                // A default run executes only offline/stub cases.
                return false;
            }
            true
        })
        .collect();

    let mut outcomes = Vec::new();
    for case in &selected {
        let outcome = runner.run_case(case).await;
        if !outcome.passed() {
            eprintln!("FAIL {}::{}", outcome.tool, outcome.name);
            for failure in &outcome.failures {
                eprintln!("  - {failure}");
            }
            if let Some(e) = &outcome.execution_error {
                eprintln!("  - execution error: {e}");
            }
        }
        outcomes.push(outcome);
    }

    if check {
        let drift = runner.check(&outcomes).unwrap_or_else(|e| {
            eprintln!("check failed: {e}");
            Vec::new()
        });
        for line in &drift {
            eprintln!("DRIFT {line}");
        }
        if drift.is_empty() && outcomes.iter().all(|o| o.passed()) {
            println!("check passed: {} case(s)", outcomes.len());
            return ExitCode::SUCCESS;
        }
        return ExitCode::FAILURE;
    }

    let write_all_meta = !explicit;
    if let Err(e) = runner.write_outputs(&outcomes, write_all_meta) {
        eprintln!("failed to write outputs: {e}");
        return ExitCode::FAILURE;
    }

    let passed = outcomes.iter().filter(|o| o.passed()).count();
    println!(
        "wrote {} case(s) to outputs/ ({} passed)",
        outcomes.len(),
        passed
    );
    if passed != outcomes.len() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
