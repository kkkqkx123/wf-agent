//! Golden-output and coverage guards for the fixture harness (feature `harness`).
//!
//! Run `cargo run -p wf-tools --features harness --example tool_showcase -- --all`
//! to (re)generate `outputs/` before relying on the drift check.

#![cfg(feature = "harness")]

use std::collections::BTreeSet;
use std::path::PathBuf;

use wf_tools::harness::{tool_tier, Case, Runner, Tier};
use wf_tools::predefined;

fn runner() -> Runner {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    Runner::new(manifest.join("fixtures"), manifest.join("outputs"))
}

fn case_tier(case: &Case) -> Tier {
    case.fixture
        .tier
        .as_deref()
        .and_then(Tier::parse)
        .unwrap_or_else(|| tool_tier(&case.fixture.tool))
}

/// Tools the agent loop intercepts instead of dispatching through the
/// registry. They have no handler, so they are represented by a schema
/// snapshot only and never need a runnable fixture.
const SCHEMA_ONLY: &[&str] = &["ask_followup_question", "attempt_completion"];

/// Every predefined tool must have a schema snapshot, so every tool is
/// represented in the reviewable outputs even before it has a runnable fixture.
#[test]
fn every_tool_has_a_schema_snapshot() {
    let outputs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("outputs");
    for def in predefined::all_definitions() {
        let path = outputs.join(def.id).join("schema.json");
        assert!(
            path.exists(),
            "missing schema snapshot for '{}' at {} (run tool_showcase --all)",
            def.id,
            path.display()
        );
    }
}

/// Every runnable tool (offline or stub tier) must ship at least one
/// fixture, except the schema-only interaction tools with no handler.
#[test]
fn every_runnable_tool_has_a_fixture() {
    let runner = runner();
    let cases = runner.load_cases().expect("load fixtures");
    let covered: BTreeSet<&str> = cases
        .iter()
        .map(|case| case.fixture.tool.as_str())
        .collect();
    for def in predefined::all_definitions() {
        if !matches!(tool_tier(def.id), Tier::Offline | Tier::Stub) {
            continue;
        }
        if SCHEMA_ONLY.contains(&def.id) {
            continue;
        }
        assert!(
            covered.contains(def.id),
            "runnable tool '{}' has no fixture under fixtures/",
            def.id
        );
    }
}

/// Offline and stub fixtures must pass their assertions and match the
/// committed outputs exactly (after normalization).
#[tokio::test]
async fn runnable_cases_match_golden_outputs() {
    let runner = runner();
    let cases = runner.load_cases().expect("load fixtures");
    let runnable: Vec<Case> = cases
        .into_iter()
        .filter(|case| {
            matches!(case_tier(case), Tier::Offline | Tier::Stub)
                && !SCHEMA_ONLY.contains(&case.fixture.tool.as_str())
        })
        .collect();
    assert!(!runnable.is_empty(), "no offline/stub fixtures were discovered");

    let mut outcomes = Vec::new();
    for case in &runnable {
        outcomes.push(runner.run_case(case).await);
    }

    let failed: Vec<String> = outcomes
        .iter()
        .filter(|o| !o.passed())
        .map(|o| {
            format!(
                "{}::{} ({})",
                o.tool,
                o.name,
                o.failures
                    .iter()
                    .cloned()
                    .chain(o.execution_error.clone())
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        })
        .collect();
    assert!(failed.is_empty(), "offline/stub cases failed: {failed:#?}");

    let drift = runner.check(&outcomes).expect("check outputs");
    assert!(
        drift.is_empty(),
        "golden output drift (run tool_showcase --all --update):\n{}",
        drift.join("\n")
    );
}
