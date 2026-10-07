//! Tests for the execution record query API.

use serde_json::Value;

use super::aggregation::{aggregate, AggregationOp, AggregationType};
use super::export::{export_to_csv, export_to_xml};
use super::filter::{
    apply_filter_expressions, get_field_value, sort_records, FilterExpression, FilterOperator,
    SortOptions,
};
use super::group_by_field;
use super::ExecutionRecord;

fn sample_records() -> Vec<ExecutionRecord> {
    vec![
        ExecutionRecord {
            execution_id: "exec-1".into(),
            workflow_id: "wf-a".into(),
            status: "completed".into(),
            input: Some(serde_json::json!({"greeting": "hello", "n": 10})),
            output: Some(serde_json::json!({"total": 3, "label": "alpha"})),
            error: None,
            start_time: 1000,
            end_time: Some(1500),
            duration: Some(500),
        },
        ExecutionRecord {
            execution_id: "exec-2".into(),
            workflow_id: "wf-b".into(),
            status: "failed".into(),
            input: Some(serde_json::json!({"greeting": "world", "n": 20})),
            output: Some(serde_json::json!({"total": 7, "label": "beta"})),
            error: Some("boom".into()),
            start_time: 2000,
            end_time: Some(2400),
            duration: Some(400),
        },
        ExecutionRecord {
            execution_id: "exec-3".into(),
            workflow_id: "wf-a".into(),
            status: "completed".into(),
            input: Some(serde_json::json!({"greeting": "hi", "n": 30})),
            output: Some(serde_json::json!({"total": 11, "label": "gamma"})),
            error: None,
            start_time: 3000,
            end_time: Some(3200),
            duration: Some(200),
        },
    ]
}

#[test]
fn field_path_access_resolves_nested_and_both_naming_forms() {
    let record = &sample_records()[0];
    assert_eq!(
        get_field_value(record, "executionId"),
        Some(Value::from("exec-1"))
    );
    assert_eq!(
        get_field_value(record, "execution_id"),
        Some(Value::from("exec-1"))
    );
    assert_eq!(
        get_field_value(record, "output.total"),
        Some(Value::from(3))
    );
    assert_eq!(
        get_field_value(record, "input.greeting"),
        Some(Value::from("hello"))
    );
    assert_eq!(get_field_value(record, "output.missing"), None);
}

#[test]
fn expressions_filter_by_nested_regex_and_arithmetic() {
    let records = sample_records();
    let expression = FilterExpression {
        field: "output.total".into(),
        operator: FilterOperator::Gt,
        value: Value::from(5),
    };
    let filtered = apply_filter_expressions(&records, std::slice::from_ref(&expression));
    assert_eq!(filtered.len(), 2);
    assert!(filtered
        .iter()
        .all(|r| r.output.as_ref().unwrap()["total"].as_i64().unwrap() > 5));

    let regex = FilterExpression {
        field: "input.greeting".into(),
        operator: FilterOperator::Regex,
        value: Value::from("^w.*d$"),
    };
    let filtered = apply_filter_expressions(&records, std::slice::from_ref(&regex));
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].execution_id, "exec-2");

    let eq = FilterExpression {
        field: "status".into(),
        operator: FilterOperator::Eq,
        value: Value::from("completed"),
    };
    let filtered = apply_filter_expressions(&records, std::slice::from_ref(&eq));
    assert_eq!(filtered.len(), 2);
}

#[test]
fn aggregations_compute_count_sum_avg_min_max() {
    let records = sample_records();
    let operations = vec![
        AggregationOp {
            r#type: AggregationType::Count,
            field: None,
            group_by: None,
            as_: Some("total".into()),
        },
        AggregationOp {
            r#type: AggregationType::Sum,
            field: Some("output.total".into()),
            group_by: None,
            as_: None,
        },
        AggregationOp {
            r#type: AggregationType::Avg,
            field: Some("duration".into()),
            group_by: None,
            as_: None,
        },
        AggregationOp {
            r#type: AggregationType::Min,
            field: Some("duration".into()),
            group_by: None,
            as_: None,
        },
        AggregationOp {
            r#type: AggregationType::Max,
            field: Some("duration".into()),
            group_by: None,
            as_: None,
        },
    ];
    let results = aggregate(&records, &operations);
    assert_eq!(results[0].get("total"), Some(&Value::from(3)));
    assert_eq!(results[1].get("sum"), Some(&Value::from(21)));
    assert_eq!(results[2].get("avg"), Some(&Value::from(1100.0 / 3.0)));
    assert_eq!(results[3].get("min"), Some(&Value::from(200)));
    assert_eq!(results[4].get("max"), Some(&Value::from(500)));
}

#[test]
fn group_by_counts_per_key() {
    let records = sample_records();
    let op = AggregationOp {
        r#type: AggregationType::GroupBy,
        field: None,
        group_by: Some("workflow_id".into()),
        as_: Some("by_workflow".into()),
    };
    let results = aggregate(&records, &[op]);
    let groups = results[0]["by_workflow"].as_object().expect("group object");
    assert_eq!(groups["wf-a"], Value::from(2));
    assert_eq!(groups["wf-b"], Value::from(1));
}

#[test]
fn csv_export_has_header_and_rows() {
    let csv = export_to_csv(&sample_records());
    let lines: Vec<&str> = csv.trim_end().lines().collect();
    assert_eq!(lines.len(), 4);
    assert!(lines[0].starts_with("executionId,workflowId,status"));
    assert!(lines[1].contains("exec-1"));
    assert!(lines[2].contains("wf-b"));
}

#[test]
fn xml_export_escapes_and_wraps_records() {
    let xml = export_to_xml(&sample_records());
    assert!(xml.starts_with("<?xml"));
    assert!(xml.contains("<records>"));
    assert!(xml.contains("<record>"));
    assert!(xml.contains("<executionId>exec-2</executionId>"));
    assert!(xml.ends_with("</records>"));
}

#[test]
fn distinct_and_group_by_field_collect_values() {
    let records = sample_records();
    let distinct = super::get_distinct(&records, "workflow_id");
    assert_eq!(distinct, vec![Value::from("wf-a"), Value::from("wf-b")]);

    let groups = group_by_field(&records, "status");
    assert_eq!(groups["completed"].len(), 2);
    assert_eq!(groups["failed"].len(), 1);
}

#[test]
fn sort_orders_numeric_field_asc_and_desc() {
    let records = sample_records();
    let mut ascending = records.clone();
    sort_records(
        &mut ascending,
        &SortOptions {
            field: "start_time".into(),
            descending: false,
        },
    );
    assert_eq!(ascending[0].execution_id, "exec-1");
    assert_eq!(ascending[2].execution_id, "exec-3");

    let mut descending = records;
    sort_records(
        &mut descending,
        &SortOptions {
            field: "start_time".into(),
            descending: true,
        },
    );
    assert_eq!(descending[0].execution_id, "exec-3");
}

#[test]
fn missing_field_neq_matches_and_contains_operates_on_strings() {
    let records = sample_records();
    let neq = FilterExpression {
        field: "output.missing".into(),
        operator: FilterOperator::Neq,
        value: Value::Null,
    };
    assert_eq!(
        apply_filter_expressions(&records, std::slice::from_ref(&neq)).len(),
        3
    );

    let contains = FilterExpression {
        field: "output.label".into(),
        operator: FilterOperator::Contains,
        value: Value::from("bet"),
    };
    assert_eq!(
        apply_filter_expressions(&records, std::slice::from_ref(&contains)).len(),
        1
    );
}
