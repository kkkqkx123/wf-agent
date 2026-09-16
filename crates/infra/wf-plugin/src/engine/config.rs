use std::path::PathBuf;

use serde_json::Value;

use super::PluginEngine;
use crate::contributions::OverridePolicy;
use crate::error::{PluginError, PluginResult};
use crate::manifest::{PluginManifest, PluginPermission};
use crate::signing::TrustedKeys;

pub struct PluginSystemConfig {
    pub enabled: bool,
    pub paths: Vec<PathBuf>,
    pub auto_activate: bool,
    pub guard_timeout_ms: u64,
    pub override_policy: OverridePolicy,
    pub allow_list: Vec<String>,
    pub block_list: Vec<String>,
    /// Plugins declaring any of these permissions are refused at load time.
    pub required_permissions_blocklist: Vec<PluginPermission>,
    pub config: std::collections::HashMap<String, Value>,
    /// How to treat an unparseable `sdk_version` requirement (or host
    /// version): `false` (default) keeps the historical fail-open skip,
    /// `true` rejects the plugin with `InvalidManifest` instead.
    pub strict_sdk_version: bool,
    pub signing: TrustedKeys,
}

impl Default for PluginSystemConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            paths: vec![PathBuf::from("./plugins")],
            auto_activate: true,
            guard_timeout_ms: 10000,
            override_policy: OverridePolicy::Forbid,
            allow_list: vec![],
            block_list: vec![],
            required_permissions_blocklist: vec![],
            config: std::collections::HashMap::new(),
            strict_sdk_version: false,
            signing: TrustedKeys::default(),
        }
    }
}

impl PluginEngine {
    pub(crate) fn is_allowed(&self, plugin_id: &str) -> bool {
        if !self.options.allow_list.is_empty() {
            return self.options.allow_list.contains(&plugin_id.to_owned());
        }
        if !self.options.block_list.is_empty() {
            return !self.options.block_list.contains(&plugin_id.to_owned());
        }
        true
    }

    /// Reject plugins declaring permissions refused by host policy.
    pub(crate) fn check_permissions(&self, manifest: &PluginManifest) -> PluginResult<()> {
        let blocked = manifest
            .permissions
            .iter()
            .any(|p| self.options.required_permissions_blocklist.contains(p));
        if blocked {
            return Err(PluginError::PermissionDenied {
                plugin_id: manifest.id.clone(),
                reason: format!(
                    "plugin declares permissions blocked by host policy: {:?}",
                    manifest.permissions
                ),
            });
        }
        Ok(())
    }

    /// Enforce the manifest's `sdk_version` requirement against the host
    /// version. A mismatch always rejects the plugin; unparseable
    /// requirements (or host version) reject only under
    /// `strict_sdk_version`, otherwise they are skipped with a warning
    /// (historical fail-open behavior).
    pub(crate) fn check_sdk_version(&self, manifest: &PluginManifest) -> PluginResult<()> {
        let sdk_req = match manifest.sdk_version.as_deref() {
            Some(req) => req,
            None => return Ok(()),
        };
        let req = match semver::VersionReq::parse(sdk_req) {
            Ok(req) => req,
            Err(e) => {
                let message = format!(
                    "plugin '{}' declares unparseable sdk_version '{}': {}",
                    manifest.id, sdk_req, e
                );
                if self.options.strict_sdk_version {
                    return Err(PluginError::InvalidManifest(message));
                }
                tracing::warn!("{message}; skipping sdk_version check (fail-open)");
                return Ok(());
            }
        };
        let host = match semver::Version::parse(&self.sdk_version) {
            Ok(host) => host,
            Err(e) => {
                let message = format!(
                    "host sdk_version '{}' is unparseable: {}",
                    self.sdk_version, e
                );
                if self.options.strict_sdk_version {
                    return Err(PluginError::InvalidManifest(message));
                }
                tracing::warn!("{message}; skipping sdk_version check (fail-open)");
                return Ok(());
            }
        };
        if !req.matches(&host) {
            return Err(PluginError::InvalidManifest(format!(
                "sdk version '{}' not satisfied by host '{}'",
                sdk_req, self.sdk_version
            )));
        }
        Ok(())
    }
}

pub(crate) fn validate_manifest(manifest: &PluginManifest) -> Option<Vec<String>> {
    let mut errors = Vec::new();
    if manifest.id.is_empty() {
        errors.push("id is required".into());
    }
    if manifest.version.is_empty() {
        errors.push("version is required".into());
    }
    if manifest.entry_point.is_empty() {
        errors.push("entry_point is required".into());
    }
    if manifest.sdk_version.is_some() && manifest.sdk_version.as_deref() == Some("") {
        errors.push("sdk_version must not be empty when present".into());
    }
    if errors.is_empty() {
        None
    } else {
        Some(errors)
    }
}
