use crate::errors::WfError;

pub fn patch_tool_error(
    message: impl Into<String>,
    code: String,
    file_path: Option<String>,
    line_number: Option<u32>,
) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    ctx.insert(
        "tool_name".into(),
        serde_json::Value::String("apply_patch".into()),
    );
    ctx.insert("code".into(), serde_json::Value::String(code));
    if let Some(v) = file_path {
        ctx.insert("file_path".into(), serde_json::Value::String(v));
    }
    if let Some(v) = line_number {
        ctx.insert("line_number".into(), serde_json::Value::Number(v.into()));
    }
    WfError::new(message).with_context(ctx)
}

pub fn patch_parse_error(
    message: impl Into<String>,
    code: Option<String>,
    line_number: Option<u32>,
) -> WfError {
    patch_tool_error(
        message,
        code.unwrap_or_else(|| "PATCH_INVALID_FORMAT".into()),
        None,
        line_number,
    )
}

pub fn patch_apply_error(
    message: impl Into<String>,
    code: String,
    file_path: Option<String>,
) -> WfError {
    patch_tool_error(message, code, file_path, None)
}

pub fn patch_validation_error(
    message: impl Into<String>,
    code: String,
    file_path: Option<String>,
    line_number: Option<u32>,
) -> WfError {
    patch_tool_error(message, code, file_path, line_number)
}
