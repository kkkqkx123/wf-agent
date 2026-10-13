use crate::args::Cli;
use crate::cmd::render::render_envelope;
use crate::connection::ClientHandle;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

/// Filter, pagination, and post-processing options for `wf query`.
#[derive(Debug, Default)]
pub struct QueryOptions<'a> {
    pub status: Option<&'a str>,
    pub workflow_id: Option<&'a str>,
    pub limit: Option<usize>,
    pub sort: Option<&'a str>,
    pub desc: bool,
    pub offset: Option<usize>,
    pub aggregate: Option<&'a str>,
    pub export: Option<&'a str>,
    pub filter: Option<&'a str>,
}

pub async fn run(cli: &Cli, opts: QueryOptions<'_>) -> CliResult<()> {
    let handle = ClientHandle::from_cli(cli).await?;
    let client = handle.client();

    let mut filters = serde_json::json!({});
    if let Some(status) = opts.status {
        filters["status"] = status.into();
    }
    if let Some(workflow_id) = opts.workflow_id {
        filters["workflow_id"] = workflow_id.into();
    }
    let mut expressions = Vec::new();
    if let Some(expr_str) = opts.filter {
        expressions.push(parse_filter_expr(expr_str)?);
    }

    if let Some(agg) = opts.aggregate {
        // Retained passthrough: the snapshot declares `operations` as
        // `Vec<String>` but the server decodes `Vec<AggregationOp>` objects;
        // typing against the snapshot would send the wrong shape.
        let body = serde_json::json!({
            "filters": filters,
            "expressions": expressions,
            "operations": [parse_aggregate(agg)?],
        });
        let data: serde_json::Value = client.post_json("/api/v1/query/aggregate", &body).await?;
        let envelope = OutputEnvelope::success("query-aggregate", data);
        render_envelope(cli.output, envelope)?;
        return Ok(());
    }

    if let Some(fmt) = opts.export {
        // Retained passthrough: `/query/export` answers as a file download
        // (`text/csv`), not the JSON envelope, so it stays outside the
        // generated envelope system.
        let format = match fmt.to_ascii_lowercase().as_str() {
            "json" | "csv" | "xml" => fmt.to_ascii_lowercase(),
            _ => return Err(CliError::Arguments(format!("invalid export format {fmt}"))),
        };
        let body = serde_json::json!({
            "filters": filters,
            "expressions": expressions,
            "format": format,
        });
        let data: serde_json::Value = client.post_json("/api/v1/query/export", &body).await?;
        let output = data.as_str().unwrap_or("").to_string();
        if cli.output == crate::output::OutputFormat::Text {
            println!("{output}");
        } else {
            let data = serde_json::json!({"export": output, "format": fmt});
            render_envelope(cli.output, OutputEnvelope::success("query-export", data))?;
        }
        return Ok(());
    }

    // Named request body; response `data` stays generic JSON passthrough
    // because query records have no fixed schema.
    let body = wf_client::models::QueryBody {
        filters: Some(filters),
        expressions,
        sort_field: opts.sort.map(str::to_string),
        sort_descending: opts.sort.map(|_| opts.desc),
        limit: opts.limit.map(|v| v as u64),
        offset: opts.offset.map(|v| v as u64),
    };
    let envelope = client
        .generated()
        .handle_query(&body)
        .await
        .map_err(wf_client::client::into_typed_client_error)?;
    let data: serde_json::Value =
        wf_client::client::unwrap_generated_envelope(envelope.into_inner())?;
    let envelope = OutputEnvelope::success("query-executions", data);
    render_envelope(cli.output, envelope)?;
    Ok(())
}

fn parse_filter_expr(s: &str) -> CliResult<serde_json::Value> {
    let parts: Vec<&str> = s.splitn(3, ' ').collect();
    if parts.len() != 3 {
        return Err(CliError::Arguments(format!(
            "invalid filter expr '{s}': expected 'field operator value'"
        )));
    }
    match parts[1] {
        "eq" | "neq" | "gt" | "gte" | "lt" | "lte" | "in" | "nin" | "contains" | "regex" => {}
        _ => {
            return Err(CliError::Arguments(format!(
                "unknown operator {}",
                parts[1]
            )))
        }
    }
    let value: serde_json::Value =
        serde_json::from_str(parts[2]).unwrap_or(serde_json::Value::String(parts[2].to_string()));
    Ok(serde_json::json!({
        "field": parts[0],
        "operator": parts[1],
        "value": value,
    }))
}

fn parse_aggregate(s: &str) -> CliResult<serde_json::Value> {
    if s == "count" {
        return Ok(serde_json::json!({"type": "count"}));
    }
    if let Some(field) = s.strip_prefix("sum:") {
        return Ok(serde_json::json!({"type": "sum", "field": field}));
    }
    if let Some(field) = s.strip_prefix("avg:") {
        return Ok(serde_json::json!({"type": "avg", "field": field}));
    }
    if let Some(field) = s.strip_prefix("min:") {
        return Ok(serde_json::json!({"type": "min", "field": field}));
    }
    if let Some(field) = s.strip_prefix("max:") {
        return Ok(serde_json::json!({"type": "max", "field": field}));
    }
    if let Some(field) = s.strip_prefix("group_by:") {
        return Ok(serde_json::json!({"type": "group_by", "group_by": field}));
    }
    Err(CliError::Arguments(format!("invalid aggregate {s}")))
}
