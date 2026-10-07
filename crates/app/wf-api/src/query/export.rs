//! CSV / XML / JSON export of execution record sets.

use serde_json::Value;

use crate::query::filter::{stringify, to_camel_case, to_snake_case};
use crate::query::ExecutionRecord;

/// Export format of a record set (`parquet` maps to JSON).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Json,
    Csv,
    Xml,
}

/// Serialize a record set in the requested format.
pub fn export_to_format(records: &[ExecutionRecord], format: ExportFormat) -> String {
    match format {
        ExportFormat::Json => serde_json::to_string_pretty(records).unwrap_or_default(),
        ExportFormat::Csv => export_to_csv(records),
        ExportFormat::Xml => export_to_xml(records),
    }
}

/// Serialize a record set as CSV (headers from the first record's keys).
pub fn export_to_csv(records: &[ExecutionRecord]) -> String {
    let Some(first) = records.first() else {
        return String::new();
    };
    let values: Vec<serde_json::Map<String, Value>> = records
        .iter()
        .filter_map(|record| serde_json::to_value(record).ok()?.as_object().cloned())
        .collect();
    let headers: Vec<String> = first_csv_fields(first)
        .into_iter()
        .map(camel_case)
        .collect();
    let mut out = String::new();
    out.push_str(&headers.join(","));
    out.push('\n');
    for map in values {
        let cells: Vec<String> = headers
            .iter()
            .map(|header| {
                map.get(&to_snake_case(header))
                    .cloned()
                    .unwrap_or(Value::Null)
            })
            .map(csv_cell)
            .collect();
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

/// Serialize a record set as XML (`<records><record>…</record></records>`).
pub fn export_to_xml(records: &[ExecutionRecord]) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<records>\n");
    for record in records {
        xml.push_str("  <record>\n");
        let Ok(value) = serde_json::to_value(record) else {
            continue;
        };
        if let Some(map) = value.as_object() {
            for (key, value) in map {
                if value.is_null() {
                    continue;
                }
                let tag = to_camel_case(key);
                xml.push_str(&format!(
                    "    <{}>{}</{}>\n",
                    tag,
                    escape_xml(&stringify(value)),
                    tag
                ));
            }
        }
        xml.push_str("  </record>\n");
    }
    xml.push_str("</records>");
    xml
}

/// First-level CSV headers of a record, in field order.
fn first_csv_fields(_record: &ExecutionRecord) -> Vec<&'static str> {
    vec![
        "execution_id",
        "workflow_id",
        "status",
        "input",
        "output",
        "error",
        "start_time",
        "end_time",
        "duration",
    ]
}

/// Render one CSV cell; strings containing separators are quoted.
fn csv_cell(value: Value) -> String {
    match value {
        Value::String(text) if text.contains(',') || text.contains('"') || text.contains('\n') => {
            format!("\"{}\"", text.replace('"', "\"\""))
        }
        Value::String(text) => text,
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Escape XML special characters.
fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Convenience alias for the camel-case CSV header projection.
fn camel_case(key: &str) -> String {
    to_camel_case(key)
}
