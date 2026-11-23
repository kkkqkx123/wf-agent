//! Network approval policy: host normalization and parameter extraction.
//!
//! The shared host matching rules live in [`crate::network`] so approval and
//! the concrete transports evaluate the same domain names.

use wf_types::interaction::tool_approval::ToolApprovalRequestData;

pub(super) use crate::network::{domain_matches, normalize_host};

pub(super) fn match_uri_pattern(uri: &str, pattern: &str) -> bool {
    let escaped = regex::escape(pattern);
    let regex_str = format!("^{}$", escaped.replace(r"\*", ".*").replace(r"\?", "."));
    regex::Regex::new(&regex_str)
        .map(|re| re.is_match(uri))
        .unwrap_or(false)
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
