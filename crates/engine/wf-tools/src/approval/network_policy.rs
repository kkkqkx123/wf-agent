//! Network approval policy: host normalization and domain matching.

use wf_types::interaction::tool_approval::ToolApprovalRequestData;

pub(super) fn match_uri_pattern(uri: &str, pattern: &str) -> bool {
    let escaped = regex::escape(pattern);
    let regex_str = format!("^{}$", escaped.replace(r"\*", ".*").replace(r"\?", "."));
    regex::Regex::new(&regex_str)
        .map(|re| re.is_match(uri))
        .unwrap_or(false)
}

pub(super) fn normalize_host(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    let value = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
        .unwrap_or(&value);
    let host = value.split('/').next().unwrap_or(value);
    let host = host.split('@').next_back().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    let host = host.trim().trim_matches('.');
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

pub(super) fn domain_matches(host: &str, rule: &str) -> bool {
    let Some(host) = normalize_host(host) else {
        return false;
    };
    let Some(rule) = normalize_host(rule) else {
        return false;
    };
    host == rule || host.ends_with(&format!(".{rule}"))
}

pub(super) fn extract_domain(tool: &ToolApprovalRequestData) -> Option<String> {
    for key in ["domain", "url", "uri", "host", "endpoint", "link"] {
        if let Some(domain) = tool
            .parameters
            .get(key)
            .and_then(|v| v.as_str())
            .and_then(normalize_host)
        {
            return Some(domain);
        }
    }
    None
}
