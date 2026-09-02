use wf_api::query::{
    self, AggregationOp, AggregationType, ExportFormat, FilterCriteria, FilterExpression,
    FilterOperator, PaginationOptions, SortOptions,
};

use crate::args::Cli;
use crate::cmd::render::render_envelope;
use crate::error::{CliError, CliResult};
use crate::output::OutputEnvelope;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    cli: &Cli,
    status: Option<&str>,
    workflow_id: Option<&str>,
    limit: Option<usize>,
    sort: Option<&str>,
    desc: bool,
    offset: Option<usize>,
    aggregate: Option<&str>,
    export: Option<&str>,
    filter: Option<&str>,
) -> CliResult<()> {
    let adapter =
        crate::domain::DomainAdapter::bootstrap_for_cli(cli, crate::mode::CliMode::Run).await?;
    let ctx = adapter.api_context();

    let filters = FilterCriteria {
        workflow_id: workflow_id.map(String::from),
        status: status.map(String::from),
        start_time_from: None,
        start_time_to: None,
        tags: None,
        custom: None,
    };

    let pagination = PaginationOptions {
        limit: limit.unwrap_or(query::DEFAULT_QUERY_LIMIT),
        offset: offset.unwrap_or(0),
    };

    let sort_opts = sort.map(|field| SortOptions {
        field: field.to_string(),
        descending: desc,
    });

    let mut records =
        query::query(ctx, Some(&filters), sort_opts.as_ref(), Some(&pagination)).await?;

    if let Some(expr_str) = filter {
        if let Ok(expr) = parse_filter_expr(expr_str) {
            records = query::apply_filter_expressions(&records, &[expr]);
        }
    }

    if let Some(agg) = aggregate {
        let op = parse_aggregate(agg)?;
        let result = query::aggregate(&records, &[op]);
        let data = serde_json::to_value(&result)?;
        let envelope = OutputEnvelope::success("query-aggregate", data);
        render_envelope(cli.output, envelope)?;
        adapter.shutdown().await?;
        return Ok(());
    }

    if let Some(fmt) = export {
        let format = match fmt.to_ascii_lowercase().as_str() {
            "json" => ExportFormat::Json,
            "csv" => ExportFormat::Csv,
            "xml" => ExportFormat::Xml,
            _ => return Err(CliError::Arguments(format!("invalid export format {fmt}"))),
        };
        let output = query::export_to_format(&records, format);
        if cli.output == crate::output::OutputFormat::Text {
            println!("{output}");
        } else {
            let data = serde_json::json!({"export": output, "format": fmt});
            render_envelope(cli.output, OutputEnvelope::success("query-export", data))?;
        }
        adapter.shutdown().await?;
        return Ok(());
    }

    let data = serde_json::to_value(&records)?;
    let envelope = OutputEnvelope::success("query-executions", data);

    render_envelope(cli.output, envelope)?;
    adapter.shutdown().await?;
    Ok(())
}

fn parse_filter_expr(s: &str) -> CliResult<FilterExpression> {
    let parts: Vec<&str> = s.splitn(3, ' ').collect();
    if parts.len() != 3 {
        return Err(CliError::Arguments(format!(
            "invalid filter expr '{s}': expected 'field operator value'"
        )));
    }
    let operator = match parts[1] {
        "eq" => FilterOperator::Eq,
        "neq" => FilterOperator::Neq,
        "gt" => FilterOperator::Gt,
        "gte" => FilterOperator::Gte,
        "lt" => FilterOperator::Lt,
        "lte" => FilterOperator::Lte,
        "in" => FilterOperator::In,
        "nin" => FilterOperator::Nin,
        "contains" => FilterOperator::Contains,
        "regex" => FilterOperator::Regex,
        _ => {
            return Err(CliError::Arguments(format!(
                "unknown operator {}",
                parts[1]
            )))
        }
    };
    let value: serde_json::Value =
        serde_json::from_str(parts[2]).unwrap_or(serde_json::Value::String(parts[2].to_string()));
    Ok(FilterExpression {
        field: parts[0].to_string(),
        operator,
        value,
    })
}

fn parse_aggregate(s: &str) -> CliResult<AggregationOp> {
    if s == "count" {
        return Ok(AggregationOp {
            r#type: AggregationType::Count,
            field: None,
            group_by: None,
            as_: None,
        });
    }
    if let Some(field) = s.strip_prefix("sum:") {
        return Ok(AggregationOp {
            r#type: AggregationType::Sum,
            field: Some(field.to_string()),
            group_by: None,
            as_: None,
        });
    }
    if let Some(field) = s.strip_prefix("avg:") {
        return Ok(AggregationOp {
            r#type: AggregationType::Avg,
            field: Some(field.to_string()),
            group_by: None,
            as_: None,
        });
    }
    if let Some(field) = s.strip_prefix("min:") {
        return Ok(AggregationOp {
            r#type: AggregationType::Min,
            field: Some(field.to_string()),
            group_by: None,
            as_: None,
        });
    }
    if let Some(field) = s.strip_prefix("max:") {
        return Ok(AggregationOp {
            r#type: AggregationType::Max,
            field: Some(field.to_string()),
            group_by: None,
            as_: None,
        });
    }
    if let Some(field) = s.strip_prefix("group_by:") {
        return Ok(AggregationOp {
            r#type: AggregationType::GroupBy,
            field: None,
            group_by: Some(field.to_string()),
            as_: None,
        });
    }
    Err(CliError::Arguments(format!("invalid aggregate {s}")))
}
