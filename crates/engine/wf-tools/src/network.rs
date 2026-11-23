//! Shared network transport policy for built-in web tools.
//!
//! Domain rules come from the host approval configuration. They are applied
//! to the concrete request URL as well as to every redirect target, so a
//! policy decision on the tool parameter cannot be undone by the transport.

use std::sync::LazyLock;

use regex::Regex;
use wf_types::tool::Tool;

/// Tool metadata key carrying hosts a network tool will actually contact.
pub const NETWORK_DOMAINS_METADATA_KEY: &str = "network_domains";

/// Attach resolved network endpoints to a tool definition for approval.
pub fn set_network_domains(tool: &mut Tool, domains: Vec<String>) {
    if tool.metadata.is_none() {
        tool.metadata = Some(wf_types::tool::ToolMetadata {
            category: None,
            tags: None,
            documentation_url: None,
            custom_fields: None,
            risk_level: None,
            auto_approvable: None,
            create_checkpoint: None,
            exposure: None,
        });
    }
    let metadata = tool
        .metadata
        .as_mut()
        .expect("invariant: metadata initialized");
    let custom = metadata.custom_fields.get_or_insert_default();
    custom.insert(
        NETWORK_DOMAINS_METADATA_KEY.to_string(),
        serde_json::to_value(domains).unwrap_or_else(|_| serde_json::Value::Null),
    );
}

/// Read resolved network endpoints from a tool definition.
pub fn network_domains(tool: &Tool) -> Vec<String> {
    tool.metadata
        .as_ref()
        .and_then(|metadata| metadata.custom_fields.as_ref())
        .and_then(|custom| custom.get(NETWORK_DOMAINS_METADATA_KEY))
        .and_then(|value| serde_json::from_value::<Vec<String>>(value.clone()).ok())
        .unwrap_or_default()
}

/// Runtime proxy route with credentials resolved by the host. The URL is
/// consumed while building an HTTP client and is never stored by a backend.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkProxy {
    pub url: Option<String>,
    pub no_proxy: Vec<String>,
}

impl NetworkProxy {
    pub fn new(url: Option<String>, no_proxy: Vec<String>) -> Self {
        Self { url, no_proxy }
    }
}

/// Domain rules applied by network transports before a request is sent.
#[derive(Debug, Clone, Default)]
pub struct NetworkDomainPolicy {
    pub allowed_domains: Option<Vec<String>>,
    pub denied_domains: Option<Vec<String>>,
}

impl NetworkDomainPolicy {
    pub fn new(allowed_domains: Option<Vec<String>>, denied_domains: Option<Vec<String>>) -> Self {
        Self {
            allowed_domains,
            denied_domains,
        }
    }

    /// Copy only the denial rules. Search backends may reach any engine
    /// endpoint while results still come from across the web; only explicit
    /// denials constrain those transports.
    pub fn denied_only(&self) -> Self {
        Self {
            allowed_domains: None,
            denied_domains: self.denied_domains.clone(),
        }
    }

    /// Reject when the URL host is denied, or when an allowlist is configured
    /// and does not contain it. An empty allowlist imposes no restriction.
    pub fn allows_url(&self, url: &str) -> Result<(), String> {
        let host =
            normalize_host(url).ok_or_else(|| format!("cannot resolve host from '{url}'"))?;
        self.allows_host(&host)
    }

    pub fn allows_host(&self, host: &str) -> Result<(), String> {
        let host = normalize_host(host).ok_or_else(|| format!("invalid host '{host}'"))?;
        if self
            .denied_domains
            .as_ref()
            .is_some_and(|rules| rules.iter().any(|rule| domain_matches(&host, rule)))
        {
            return Err(format!("domain '{host}' is denied"));
        }
        if let Some(allowed) = &self.allowed_domains {
            if !allowed.is_empty() && !allowed.iter().any(|rule| domain_matches(&host, rule)) {
                return Err(format!("domain '{host}' is not allowed"));
            }
        }
        Ok(())
    }
}

/// Normalize a URL, authority or bare host into a comparable host string.
/// IPv6 literals are unwrapped from brackets and ports and userinfo are
/// removed so a rule of `::1` matches `http://[::1]:8080/path`.
pub fn normalize_host(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    let authority = match value.split_once("://") {
        Some((_, rest)) => rest,
        None => value.as_str(),
    };
    let authority = authority.split(['/', '?', '#']).next().unwrap_or(authority);
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = if let Some(inner) = authority
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']').map(|(host, _)| host))
    {
        inner
    } else {
        authority.split(':').next().unwrap_or(authority)
    };
    let host = host.trim().trim_matches('.');
    (!host.is_empty()).then(|| host.to_string())
}

/// Match a host against an exact domain or any of its subdomains.
pub fn domain_matches(host: &str, rule: &str) -> bool {
    let Some(host) = normalize_host(host) else {
        return false;
    };
    let Some(rule) = normalize_host(rule) else {
        return false;
    };
    host == rule || host.ends_with(&format!(".{rule}"))
}

static USERINFO_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)([a-z][a-z0-9+.-]*://)[^/@\s]+@")
        .expect("invariant: fixed userinfo pattern must compile")
});

/// Format a transport failure without exposing proxy userinfo.
pub fn redact_transport_error(error: impl std::fmt::Display) -> String {
    let text = error.to_string();
    USERINFO_RE.replace_all(&text, "$1[redacted]@").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_authority_forms() {
        assert_eq!(
            normalize_host("HTTPS://Api.Example.COM:8443/v1").as_deref(),
            Some("api.example.com")
        );
        assert_eq!(
            normalize_host("user@proxy.example:3128").as_deref(),
            Some("proxy.example")
        );
        assert_eq!(normalize_host("http://[::1]:8080/").as_deref(), Some("::1"));
        assert_eq!(normalize_host(""), None);
    }

    #[test]
    fn applies_denied_and_allowed_rules() {
        let policy = NetworkDomainPolicy::new(
            Some(vec!["example.com".into()]),
            Some(vec!["bad.example.com".into()]),
        );
        assert!(policy.allows_url("https://api.example.com/a").is_ok());
        assert!(policy.allows_url("https://bad.example.com/a").is_err());
        assert!(policy.allows_url("https://other.test/a").is_err());
    }

    #[test]
    fn denied_only_drops_allowlist() {
        let policy = NetworkDomainPolicy::new(
            Some(vec!["example.com".into()]),
            Some(vec!["bad.test".into()]),
        )
        .denied_only();
        assert!(policy.allows_host("any.search.engine").is_ok());
        assert!(policy.allows_host("bad.test").is_err());
    }

    #[test]
    fn empty_allowlist_is_unrestricted() {
        let policy = NetworkDomainPolicy::new(Some(Vec::new()), None);
        assert!(policy.allows_host("any.example").is_ok());
    }

    #[test]
    fn redacts_proxy_userinfo() {
        assert_eq!(
            redact_transport_error("connect http://u:secret@proxy.test:3128 failed"),
            "connect http://[redacted]@proxy.test:3128 failed"
        );
    }
}
