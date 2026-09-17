//! Plugin package management: install registry + enable/disable state.
//!
//! The manager tracks two orthogonal dimensions and persists them in a single
//! state file (`installed-plugins.json`) under the engine's first scan path:
//! - which plugin directories are installed (install/uninstall)
//! - whether an installed plugin is enabled (enable/disable)
//!
//! Plugin files are never copied or deleted here — install registers a source
//! path, uninstall removes the registry entry. Filesystem mutations stay with
//! the caller (CLI layer). A missing or corrupt state file degrades to an
//! empty registry (warn, do not block startup).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::{PluginError, PluginResult};
use crate::manifest::PluginManifest;
use crate::signing::{enforce_signature, verify_file, SignatureStatus, TrustedKeys};

pub const PACKAGE_STATE_FILE: &str = "installed-plugins.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub source_path: String,
    pub enabled: bool,
    /// RFC 3339 timestamp of the install moment.
    pub installed_at: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackageState {
    #[serde(default)]
    pub installed: Vec<InstalledPlugin>,
}

/// Interior-mutable package manager. All mutations persist the state file
/// atomically (temp file + rename).
pub struct PluginPackageManager {
    state_path: PathBuf,
    state: Mutex<PackageState>,
    trust: Mutex<TrustedKeys>,
}

impl PluginPackageManager {
    /// Create a manager persisting into `state_dir`. Loads the existing state
    /// file synchronously; a missing file yields an empty registry.
    pub fn new(state_dir: &Path) -> Self {
        let state_path = state_dir.join(PACKAGE_STATE_FILE);
        let state = match std::fs::read_to_string(&state_path) {
            Ok(content) => match serde_json::from_str::<PackageState>(&content) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        "corrupt plugin package state {}: {}; starting empty",
                        state_path.display(),
                        e
                    );
                    PackageState::default()
                }
            },
            Err(_) => PackageState::default(),
        };
        Self {
            state_path,
            state: Mutex::new(state),
            trust: Mutex::new(TrustedKeys::default()),
        }
    }

    pub fn state_path(&self) -> &Path {
        &self.state_path
    }

    pub fn installed(&self) -> Vec<InstalledPlugin> {
        self.state
            .lock()
            .expect("package state poisoned")
            .installed
            .clone()
    }

    /// Enabled is the default for plugins absent from the registry, so
    /// directory-discovered plugins keep loading unchanged.
    pub fn is_enabled(&self, plugin_id: &str) -> bool {
        self.state
            .lock()
            .expect("package state poisoned")
            .installed
            .iter()
            .find(|p| p.id == plugin_id)
            .map(|p| p.enabled)
            .unwrap_or(true)
    }

    /// Register a plugin directory. Fails when the id is already installed.
    pub fn install(&self, manifest: &PluginManifest, source_path: &Path) -> PluginResult<()> {
        let mut state = self.state.lock().expect("package state poisoned");
        if state.installed.iter().any(|p| p.id == manifest.id) {
            return Err(PluginError::AlreadyExists(manifest.id.clone()));
        }
        state.installed.push(InstalledPlugin {
            id: manifest.id.clone(),
            source_path: source_path.display().to_string(),
            enabled: true,
            installed_at: chrono::Utc::now().to_rfc3339(),
            version: manifest.version.clone(),
        });
        persist(&self.state_path, &state)
    }

    /// Remove the registry entry. Returns false when the id is not installed.
    pub fn uninstall(&self, plugin_id: &str) -> PluginResult<bool> {
        let mut state = self.state.lock().expect("package state poisoned");
        let before = state.installed.len();
        state.installed.retain(|p| p.id != plugin_id);
        if state.installed.len() == before {
            return Ok(false);
        }
        persist(&self.state_path, &state)?;
        Ok(true)
    }

    /// Update the enabled flag. Returns false when the id is not installed.
    pub fn set_enabled(&self, plugin_id: &str, enabled: bool) -> PluginResult<bool> {
        let mut state = self.state.lock().expect("package state poisoned");
        let mut changed = false;
        for p in state.installed.iter_mut().filter(|p| p.id == plugin_id) {
            if p.enabled != enabled {
                p.enabled = enabled;
                changed = true;
            }
        }
        if changed {
            persist(&self.state_path, &state)?;
        }
        Ok(changed)
    }

    /// Configure signature trust. Defaults to empty keys plus `Permissive`,
    /// which preserves pre-signature install behavior.
    pub fn set_trust(&self, trust: TrustedKeys) {
        *self.trust.lock().expect("trust poisoned") = trust;
    }

    /// Register a plugin directory after verifying its entry-point artifact
    /// against the configured trust. `Permissive` warns and proceeds on
    /// missing/invalid signatures; `Enforcing` rejects them.
    pub fn install_verified(
        &self,
        manifest: &PluginManifest,
        source_path: &Path,
    ) -> PluginResult<()> {
        let artifact = source_path.join(&manifest.entry_point);
        let trust = self.trust.lock().expect("trust poisoned").clone();
        let status = verify_file(&artifact, &trust);
        enforce_signature(&manifest.id, &artifact, &status, &trust, "installing")?;
        self.install(manifest, source_path)
    }

    /// Re-verify an installed plugin's current entry-point artifact (files
    /// may change after installation). Returns `None` when the id is not
    /// installed.
    pub fn verify_installed(&self, plugin_id: &str) -> Option<SignatureStatus> {
        let source = self
            .state
            .lock()
            .expect("package state poisoned")
            .installed
            .iter()
            .find(|p| p.id == plugin_id)
            .map(|p| PathBuf::from(&p.source_path))?;
        let manifest_path = source.join("plugin.toml");
        let entry = std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|content| toml::from_str::<PluginManifest>(&content).ok())
            .map(|m| m.entry_point)
            .unwrap_or_default();
        let trust = self.trust.lock().expect("trust poisoned").clone();
        Some(verify_file(&source.join(entry), &trust))
    }
}

fn persist(state_path: &Path, state: &PackageState) -> PluginResult<()> {
    if let Some(parent) = state_path.parent() {
        std::fs::create_dir_all(parent).map_err(PluginError::Io)?;
    }
    let content =
        serde_json::to_string_pretty(state).map_err(|e| PluginError::Internal(e.to_string()))?;
    let tmp = state_path.with_extension("json.tmp");
    std::fs::write(&tmp, content).map_err(PluginError::Io)?;
    std::fs::rename(&tmp, state_path).map_err(PluginError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::Enforcement;
    use std::sync::atomic::{AtomicU64, Ordering};

    static DIR_SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "wf-plugin-pkg-{}-{}",
            tag,
            DIR_SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn manifest(id: &str) -> PluginManifest {
        PluginManifest {
            id: id.to_owned(),
            version: "0.1.0".to_owned(),
            name: None,
            description: None,
            plugin_type: None,
            sdk_version: None,
            entry_point: "main.lua".to_owned(),
            dependencies: Default::default(),
            optional_dependencies: Default::default(),
            contributions: vec![],
            permissions: vec![],
            config_schema: None,
            config: None,
            hooks: None,
            llm_providers: vec![],
            wasm: None,
            lua: None,
        }
    }

    #[test]
    fn missing_state_file_starts_empty_and_enabled_by_default() {
        let dir = temp_dir("empty");
        let mgr = PluginPackageManager::new(&dir);
        assert!(mgr.installed().is_empty());
        assert!(mgr.is_enabled("anything"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn install_uninstall_roundtrip_persists() {
        let dir = temp_dir("roundtrip");
        let mgr = PluginPackageManager::new(&dir);
        mgr.install(&manifest("demo"), Path::new("./plugins/demo"))
            .expect("install");

        // A second manager instance sees the persisted state.
        let mgr2 = PluginPackageManager::new(&dir);
        assert_eq!(mgr2.installed().len(), 1);
        assert_eq!(mgr2.installed()[0].id, "demo");
        assert!(mgr2.installed()[0].enabled);

        assert!(mgr2.uninstall("demo").expect("uninstall"));
        assert!(!PluginPackageManager::new(&dir)
            .uninstall("demo")
            .expect("uninstall again"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn double_install_is_rejected() {
        let dir = temp_dir("double");
        let mgr = PluginPackageManager::new(&dir);
        mgr.install(&manifest("demo"), Path::new("./plugins/demo"))
            .expect("install");
        assert!(matches!(
            mgr.install(&manifest("demo"), Path::new("./plugins/demo")),
            Err(PluginError::AlreadyExists(_))
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn enable_disable_updates_persisted_flag() {
        let dir = temp_dir("toggle");
        let mgr = PluginPackageManager::new(&dir);
        mgr.install(&manifest("demo"), Path::new("./plugins/demo"))
            .expect("install");

        assert!(mgr.set_enabled("demo", false).expect("disable"));
        assert!(!mgr.is_enabled("demo"));

        let mgr2 = PluginPackageManager::new(&dir);
        assert!(!mgr2.is_enabled("demo"));

        // Disabling an already-disabled plugin reports no change.
        assert!(!mgr2.set_enabled("demo", false).expect("disable again"));
        assert!(mgr2.set_enabled("demo", true).expect("enable"));
        assert!(mgr2.is_enabled("demo"));

        // Unknown id reports no change.
        assert!(!mgr2.set_enabled("ghost", false).expect("disable unknown"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn corrupt_state_file_degrades_to_empty() {
        let dir = temp_dir("corrupt");
        std::fs::write(dir.join(PACKAGE_STATE_FILE), "{not json").expect("write corrupt file");
        let mgr = PluginPackageManager::new(&dir);
        assert!(mgr.installed().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    fn signed_plugin_dir(tag: &str) -> (PathBuf, crate::signing::SigningKeypair) {
        use crate::signing::{generate_keypair, sign_file};

        let dir = temp_dir(tag);
        let src = dir.join("src");
        std::fs::create_dir_all(&src).expect("create src");
        std::fs::write(src.join("main.lua"), "return {}").expect("write artifact");
        let pair = generate_keypair();
        sign_file(&src.join("main.lua"), &pair.signing).expect("sign");
        (src, pair)
    }

    #[test]
    fn install_verified_accepts_trusted_signature() {
        use crate::signing::TrustedKeys;

        let (src, pair) = signed_plugin_dir("sig-ok");
        let dir = temp_dir("sig-ok-state");
        let mgr = PluginPackageManager::new(&dir);
        mgr.set_trust(TrustedKeys::new(
            vec![pair.verifying.to_bytes()],
            Enforcement::Enforcing,
        ));
        mgr.install_verified(&manifest("demo"), &src)
            .expect("trusted install");
        assert_eq!(mgr.installed().len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn install_verified_enforcing_rejects_unsigned() {
        use crate::signing::TrustedKeys;

        let src = temp_dir("sig-unsigned");
        std::fs::write(src.join("main.lua"), "return {}").expect("write artifact");
        let dir = temp_dir("sig-unsigned-state");
        let mgr = PluginPackageManager::new(&dir);
        mgr.set_trust(TrustedKeys::new(vec![[1u8; 32]], Enforcement::Enforcing));
        assert!(matches!(
            mgr.install_verified(&manifest("demo"), &src),
            Err(PluginError::LoadFailed(_))
        ));
        assert!(mgr.installed().is_empty());

        // Permissive mode warns and proceeds.
        mgr.set_trust(TrustedKeys::new(vec![[1u8; 32]], Enforcement::Permissive));
        mgr.install_verified(&manifest("demo"), &src)
            .expect("permissive install");
        assert_eq!(mgr.installed().len(), 1);
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&src).ok();
    }

    #[test]
    fn install_verified_enforcing_rejects_tampered_artifact() {
        use crate::signing::TrustedKeys;

        let (src, pair) = signed_plugin_dir("sig-tamper");
        std::fs::write(src.join("main.lua"), "tampered").expect("tamper");
        let dir = temp_dir("sig-tamper-state");
        let mgr = PluginPackageManager::new(&dir);
        mgr.set_trust(TrustedKeys::new(
            vec![pair.verifying.to_bytes()],
            Enforcement::Enforcing,
        ));
        assert!(matches!(
            mgr.install_verified(&manifest("demo"), &src),
            Err(PluginError::LoadFailed(_))
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn verify_installed_detects_post_install_replacement() {
        use crate::signing::{SignatureStatus, TrustedKeys};

        let (src, pair) = signed_plugin_dir("sig-audit");
        let dir = temp_dir("sig-audit-state");
        let mgr = PluginPackageManager::new(&dir);
        mgr.set_trust(TrustedKeys::new(
            vec![pair.verifying.to_bytes()],
            Enforcement::Enforcing,
        ));
        mgr.install_verified(&manifest("demo"), &src)
            .expect("trusted install");
        // verify_installed needs plugin.toml to locate the entry point.
        std::fs::write(
            src.join("plugin.toml"),
            "id = \"demo\"\nversion = \"0.1.0\"\nentry_point = \"main.lua\"\n",
        )
        .expect("write manifest");
        assert!(matches!(
            mgr.verify_installed("demo"),
            Some(SignatureStatus::Valid { .. })
        ));

        std::fs::write(src.join("main.lua"), "replaced").expect("replace");
        assert!(matches!(
            mgr.verify_installed("demo"),
            Some(SignatureStatus::Invalid { .. })
        ));
        assert!(mgr.verify_installed("ghost").is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
