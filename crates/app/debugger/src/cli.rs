use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "wf-debug", about = "Offline workflow and agent debugger")]
pub struct DebuggerCli {
    #[command(subcommand)]
    pub command: DebuggerCommand,
}

#[derive(Debug, Subcommand)]
pub enum DebuggerCommand {
    Replay(ReplayArgs),
    Check(CheckArgs),
    Branches(BranchesArgs),
    Hooks(HooksArgs),
    Triggers(TriggersArgs),
    Assert(AssertArgs),
    Timeline(TimelineArgs),
    Agents(AgentsArgs),
    Import(ImportArgs),
}

#[derive(Debug, clap::Args)]
pub struct ReplayArgs {
    #[arg(long)]
    pub trace: PathBuf,
    #[arg(long, default_value_t = false)]
    pub json: bool,
    #[arg(long, default_value_t = false)]
    pub no_color: bool,
}

#[derive(Debug, clap::Args)]
pub struct CheckArgs {
    #[arg(long)]
    pub trace: PathBuf,
    #[arg(long)]
    pub agent: Option<String>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
    #[arg(long, default_value_t = false)]
    pub no_color: bool,
}

#[derive(Debug, clap::Args)]
pub struct BranchesArgs {
    #[arg(long)]
    pub decision: PathBuf,
    #[arg(long)]
    pub variables: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct HooksArgs {
    #[arg(long)]
    pub trace: PathBuf,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct TriggersArgs {
    #[arg(long)]
    pub trace: PathBuf,
    #[arg(long)]
    pub event_type: Option<String>,
    #[arg(long)]
    pub event_name: Option<String>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct AssertArgs {
    #[arg(long)]
    pub trace: PathBuf,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct TimelineArgs {
    #[arg(long)]
    pub trace: PathBuf,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct AgentsArgs {
    #[arg(long)]
    pub trace: PathBuf,
    /// Builtin agent template to check against (for example
    /// `@standard/explorer`). Overrides the trace identity when set.
    #[arg(long)]
    pub agent: Option<String>,
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct ImportArgs {
    #[arg(long)]
    pub snapshot: PathBuf,
    /// Write the normalized trace here; prints to stdout when absent.
    #[arg(long)]
    pub out: Option<PathBuf>,
}

pub fn run(cli: DebuggerCli) -> Result<i32> {
    match cli.command {
        DebuggerCommand::Replay(args) => cmd_replay(args),
        DebuggerCommand::Check(args) => cmd_check(args),
        DebuggerCommand::Branches(args) => cmd_branches(args),
        DebuggerCommand::Hooks(args) => cmd_hooks(args),
        DebuggerCommand::Triggers(args) => cmd_triggers(args),
        DebuggerCommand::Assert(args) => cmd_assert(args),
        DebuggerCommand::Timeline(args) => cmd_timeline(args),
        DebuggerCommand::Agents(args) => cmd_agents(args),
        DebuggerCommand::Import(args) => cmd_import(args),
    }
}

fn cmd_replay(args: ReplayArgs) -> Result<i32> {
    let trace = crate::input::load_trace(&args.trace)?;
    let (text, code) = crate::runner::render_replay(&trace, args.json, args.no_color);
    print!("{text}");
    Ok(code)
}

fn cmd_check(args: CheckArgs) -> Result<i32> {
    let trace = crate::input::load_trace(&args.trace)?;
    let (text, code) =
        crate::runner::render_check(&trace, args.agent.as_deref(), args.json, args.no_color);
    print!("{text}");
    Ok(code)
}

fn cmd_branches(args: BranchesArgs) -> Result<i32> {
    let point = crate::input::load_decision(&args.decision)?;
    let variables = crate::input::load_variables(args.variables.as_deref())?;
    let verdict = crate::branches::evaluate_decision(&point, &variables);
    let summary = crate::branches::summarize_verdicts(std::slice::from_ref(&verdict));
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "verdict": verdict,
                "coverage": summary,
            }))
            .unwrap_or_default()
        );
    } else {
        println!("decision {} hits={}", verdict.node_id, verdict.hit_count);
        for branch in &verdict.branches {
            println!(
                "  -> {} hit={} {}",
                branch.target_node_id,
                branch.hit,
                branch.expression.clone().unwrap_or_default()
            );
        }
        if let Some(default) = verdict.default_target.as_deref() {
            println!(
                "  default {default} reachable={}",
                verdict.default_reachable
            );
        }
        println!("coverage {:.2}", summary.coverage_ratio);
    }
    Ok(0)
}

fn cmd_hooks(args: HooksArgs) -> Result<i32> {
    let trace = crate::input::load_trace(&args.trace)?;
    let reports = crate::hook_dbg::collect_hook_points(&trace);
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&reports).unwrap_or_default()
        );
    } else {
        for report in &reports {
            println!(
                "hook {} matched={} skipped={} veto={:?} gate_blocked={}",
                report.hook_type,
                report.matched.len(),
                report.skipped.len(),
                report.veto_reason,
                report.gate_blocked,
            );
        }
    }
    Ok(0)
}

fn cmd_triggers(args: TriggersArgs) -> Result<i32> {
    let trace = crate::input::load_trace(&args.trace)?;
    if let Some(event_type) = args.event_type.as_deref() {
        let run = crate::trigger_dbg::dry_run(
            &trace.trigger_templates,
            event_type,
            args.event_name.as_deref(),
            &trace.initial_variables,
        );
        if args.json {
            println!("{}", serde_json::to_string_pretty(&run).unwrap_or_default());
        } else {
            println!("candidates: {:?}", run.candidates);
            for drop in &run.dropped {
                println!("  drop {}: {}", drop.template_name, drop.reason);
            }
        }
        return Ok(0);
    }
    let mut matched = 0;
    let mut total = 0;
    for visit in crate::traverse::walk(&trace) {
        let (step_matched, _) = crate::trigger_dbg::summarize_seen(&visit.step.triggers_seen);
        matched += step_matched;
        total += visit.step.triggers_seen.len();
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "matched": matched,
                "total": total,
            }))
            .unwrap_or_default()
        );
    } else {
        println!("triggers matched={matched}/{total}");
    }
    Ok(0)
}

fn cmd_assert(args: AssertArgs) -> Result<i32> {
    let trace = crate::input::load_trace(&args.trace)?;
    let outcome = crate::assert::run_assertions(&trace);
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&outcome).unwrap_or_default()
        );
    } else {
        for result in &outcome.results {
            let status = if result.pass { "PASS" } else { "FAIL" };
            println!("{status} {}", result.name);
            if !result.pass {
                println!("  expected: {:?}", result.expected);
                println!("  actual: {:?}", result.actual);
                if !result.message.is_empty() {
                    println!("  {}", result.message);
                }
            }
        }
        println!("passed={} failed={}", outcome.passed, outcome.failed);
    }
    Ok(if outcome.failed() { 1 } else { 0 })
}

fn cmd_timeline(args: TimelineArgs) -> Result<i32> {
    let trace = crate::input::load_trace(&args.trace)?;
    let entries = crate::timeline::build_timeline(&trace);
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&entries).unwrap_or_default()
        );
    } else {
        for entry in &entries {
            println!("{} [{}] {}", entry.at, entry.kind, entry.label);
        }
    }
    Ok(0)
}

fn cmd_agents(args: AgentsArgs) -> Result<i32> {
    let trace = crate::input::load_trace(&args.trace)?;
    let analysis = crate::agent_dbg::analyze_agent_trace(&trace, args.agent.as_deref());
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&analysis).unwrap_or_default()
        );
    } else if !analysis.known_template {
        println!("unknown agent template '{}'", analysis.template_id);
    } else {
        println!(
            "agent {} calls={} expected_denials={} violations={}",
            analysis.template_id,
            analysis.tool_calls,
            analysis.expected_denials,
            analysis.violations.len()
        );
        for violation in &analysis.violations {
            println!(
                "  [step-{}] {} {}: {}",
                violation.path, violation.tool, violation.kind, violation.detail
            );
        }
    }
    Ok(if analysis.violations.is_empty() { 0 } else { 1 })
}

fn cmd_import(args: ImportArgs) -> Result<i32> {
    let text = std::fs::read_to_string(&args.snapshot)
        .with_context(|| format!("read snapshot {}", args.snapshot.display()))?;
    let trace = crate::import::import_snapshot_text(&text)?;
    let rendered = serde_json::to_string_pretty(&trace).unwrap_or_default();
    if let Some(out) = args.out.as_deref() {
        std::fs::write(out, format!("{rendered}\n"))
            .with_context(|| format!("write trace {}", out.display()))?;
    } else {
        println!("{rendered}");
    }
    Ok(0)
}
