use wf_api::entity::skill;

use crate::args::{Cli, SkillSub};
use crate::cmd::render::render_envelope;
use crate::error::CliResult;
use crate::output::OutputEnvelope;

pub async fn run(cli: &Cli, sub: &SkillSub) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();

    let result = match sub {
        SkillSub::List => {
            let skills = skill::list_skills(ctx)?;
            let data = serde_json::to_value(&skills)?;
            render_envelope(cli.output, OutputEnvelope::success("skill-list", data))
        }
        SkillSub::Query { filter } => {
            let all = skill::list_skills(ctx)?;
            let filtered: Vec<_> = if let Some(q) = filter {
                all.into_iter()
                    .filter(|s| s.name.contains(q.as_str()))
                    .collect()
            } else {
                all
            };
            let data = serde_json::to_value(&filtered)?;
            render_envelope(cli.output, OutputEnvelope::success("skill-query", data))
        }
        SkillSub::Show { name } => {
            let s = skill::get_skill(ctx, name)?;
            let data = serde_json::to_value(&s)?;
            render_envelope(
                cli.output,
                OutputEnvelope::success("skill-show", data).with_entity(name.clone()),
            )
        }
        SkillSub::Enable { name } => {
            skill::enable(ctx, name)?;
            let data = serde_json::json!({"enabled": name});
            render_envelope(
                cli.output,
                OutputEnvelope::success("skill-enable", data).with_entity(name.clone()),
            )
        }
        SkillSub::Disable { name } => {
            skill::disable(ctx, name)?;
            let data = serde_json::json!({"disabled": name});
            render_envelope(
                cli.output,
                OutputEnvelope::success("skill-disable", data).with_entity(name.clone()),
            )
        }
        SkillSub::Scan => {
            let list = skill::scan_skills(ctx, ".")?;
            let data = serde_json::json!({"scanned": list.len()});
            render_envelope(cli.output, OutputEnvelope::success("skill-scan", data))
        }
        SkillSub::Reload => {
            let list = skill::reload(ctx, ".")?;
            let data = serde_json::json!({"reloaded": list.len()});
            render_envelope(cli.output, OutputEnvelope::success("skill-reload", data))
        }
        SkillSub::ClearCache => {
            skill::clear_cache(ctx)?;
            let data = serde_json::json!({"cleared": true});
            render_envelope(
                cli.output,
                OutputEnvelope::success("skill-clear-cache", data),
            )
        }
    };

    adapter.shutdown().await?;
    result
}
