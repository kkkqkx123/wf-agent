//! Detached ed25519 signatures for plugin package files.
//!
//! Signing is detached: signing `plugin.wasm` writes a `plugin.wasm.sig.json`
//! sidecar next to it, and the signed bytes are the artifact itself. The host
//! never trusts a sidecar path supplied by the caller; it is always derived
//! from the artifact path, so a signature for one file cannot be replayed as
//! another file's.
//!
//! Verification happens at the package layer (`PluginPackageManager`), not on
//! the load hot path. Enforcement is opt-in: without configured trust the
//! manager behaves exactly as before.

use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{PluginError, PluginResult};

/// Sidecar algorithm identifier. Only this value is accepted; anything else
/// is reported as an invalid signature rather than silently skipped.
const SIGNATURE_ALGORITHM: &str = "ed25519";

/// Detached signature sidecar, serialized as `<artifact>.sig.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignatureSidecar {
    algorithm: String,
    public_key: String,
    signature: String,
    sha256: String,
    signed_at: String,
}

/// Enforcement mode for package installation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Enforcement {
    /// Missing or invalid signatures only warn; installation proceeds.
    /// This is the default, preserving pre-signature behavior.
    #[default]
    Permissive,
    /// Missing or invalid signatures reject the installation.
    Enforcing,
}

/// Trusted keys plus the enforcement mode. The host only ever holds public
/// keys; private keys stay with the publisher.
#[derive(Debug, Clone, Default)]
pub struct TrustedKeys {
    keys: Vec<[u8; 32]>,
    mode: Enforcement,
}

impl TrustedKeys {
    pub fn new(keys: Vec<[u8; 32]>, mode: Enforcement) -> Self {
        Self { keys, mode }
    }

    /// Parse hex-encoded public keys, rejecting malformed entries.
    pub fn from_hex_keys(keys: &[&str], mode: Enforcement) -> PluginResult<Self> {
        let mut parsed = Vec::with_capacity(keys.len());
        for key in keys {
            let raw = hex::decode(key).map_err(|e| {
                PluginError::LoadFailed(format!("malformed trusted public key '{key}': {e}"))
            })?;
            let bytes: [u8; 32] = raw.try_into().map_err(|_| {
                PluginError::LoadFailed(format!("trusted public key '{key}' is not 32 bytes"))
            })?;
            parsed.push(bytes);
        }
        Ok(Self::new(parsed, mode))
    }

    pub fn mode(&self) -> Enforcement {
        self.mode
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// Outcome of verifying one artifact file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureStatus {
    /// Signed by a trusted key; carries the hex-encoded signer key.
    Valid { key: String },
    /// No sidecar present next to the artifact.
    Unsigned,
    /// Sidecar present but unusable; carries a human-readable reason.
    Invalid { reason: String },
}

impl SignatureStatus {
    pub fn is_valid(&self) -> bool {
        matches!(self, SignatureStatus::Valid { .. })
    }
}

/// Apply `trust` to one verification outcome. `Valid` passes; `Unsigned`
/// and `Invalid` reject in `Enforcing` mode and warn-and-continue in
/// `Permissive` mode. `action` names the ongoing operation for the warning
/// (e.g. "installing", "loading"). Shared by package installation and
/// opt-in verified loads so both layers enforce identical semantics.
pub fn enforce_signature(
    plugin_id: &str,
    artifact: &Path,
    status: &SignatureStatus,
    trust: &TrustedKeys,
    action: &str,
) -> PluginResult<()> {
    match status {
        SignatureStatus::Valid { key } => {
            tracing::info!("plugin '{plugin_id}' signature valid (signer {key})");
            Ok(())
        }
        SignatureStatus::Unsigned => {
            let msg = format!(
                "plugin '{plugin_id}' has no signature for '{}'",
                artifact.display()
            );
            if trust.mode() == Enforcement::Enforcing {
                return Err(PluginError::LoadFailed(msg));
            }
            tracing::warn!("{msg}; {action} without verification");
            Ok(())
        }
        SignatureStatus::Invalid { reason } => {
            let msg = format!(
                "plugin '{plugin_id}' signature invalid for '{}': {reason}",
                artifact.display()
            );
            if trust.mode() == Enforcement::Enforcing {
                return Err(PluginError::LoadFailed(msg));
            }
            tracing::warn!("{msg}; {action} without verification");
            Ok(())
        }
    }
}

/// Publisher-side keypair. Serialize the bytes with your own secret
/// management; the host API only accepts public keys.
pub struct SigningKeypair {
    pub signing: SigningKey,
    pub verifying: VerifyingKey,
}

/// Generate a fresh keypair from OS entropy.
pub fn generate_keypair() -> SigningKeypair {
    let signing = SigningKey::generate(&mut rand::thread_rng());
    let verifying = signing.verifying_key();
    SigningKeypair { signing, verifying }
}

/// Derive the sidecar path for an artifact. Always `<artifact>.sig.json` in
/// the same directory; callers cannot override it.
pub fn sidecar_path(artifact: &Path) -> PathBuf {
    let mut name = artifact
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".sig.json");
    artifact.with_file_name(name)
}

/// Sign `artifact` and write its sidecar. Returns the sidecar path.
pub fn sign_file(artifact: &Path, key: &SigningKey) -> PluginResult<PathBuf> {
    let bytes = std::fs::read(artifact).map_err(PluginError::Io)?;
    let digest = hex::encode(Sha256::digest(&bytes));
    let signature = key.sign(&bytes);
    let sidecar = SignatureSidecar {
        algorithm: SIGNATURE_ALGORITHM.into(),
        public_key: hex::encode(key.verifying_key().as_bytes()),
        signature: hex::encode(signature.to_bytes()),
        sha256: digest,
        signed_at: chrono::Utc::now().to_rfc3339(),
    };
    let content =
        serde_json::to_string_pretty(&sidecar).map_err(|e| PluginError::Internal(e.to_string()))?;
    let path = sidecar_path(artifact);
    std::fs::write(&path, content).map_err(PluginError::Io)?;
    Ok(path)
}

/// Verify `artifact` against `trust`. Reads the artifact and its derived
/// sidecar; never follows caller-supplied sidecar paths.
pub fn verify_file(artifact: &Path, trust: &TrustedKeys) -> SignatureStatus {
    let bytes = match std::fs::read(artifact) {
        Ok(b) => b,
        Err(e) => {
            return SignatureStatus::Invalid {
                reason: format!("cannot read artifact '{}': {e}", artifact.display()),
            };
        }
    };
    let sidecar_path = sidecar_path(artifact);
    let content = match std::fs::read_to_string(&sidecar_path) {
        Ok(c) => c,
        Err(_) => return SignatureStatus::Unsigned,
    };
    let sidecar: SignatureSidecar = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(e) => {
            return SignatureStatus::Invalid {
                reason: format!("sidecar is not valid JSON: {e}"),
            };
        }
    };
    if sidecar.algorithm != SIGNATURE_ALGORITHM {
        return SignatureStatus::Invalid {
            reason: format!("unsupported algorithm '{}'", sidecar.algorithm),
        };
    }
    let expected_digest = hex::encode(Sha256::digest(&bytes));
    if sidecar.sha256 != expected_digest {
        return SignatureStatus::Invalid {
            reason: "artifact digest does not match the signed digest".into(),
        };
    }
    let key_bytes = match hex::decode(&sidecar.public_key) {
        Ok(b) => b,
        Err(_) => {
            return SignatureStatus::Invalid {
                reason: "sidecar public key is not valid hex".into(),
            };
        }
    };
    let verifying = match <[u8; 32]>::try_from(key_bytes)
        .ok()
        .and_then(|b| VerifyingKey::from_bytes(&b).ok())
    {
        Some(k) => k,
        None => {
            return SignatureStatus::Invalid {
                reason: "sidecar public key is not a valid ed25519 key".into(),
            };
        }
    };
    if !trust.keys.iter().any(|k| k == verifying.as_bytes()) {
        return SignatureStatus::Invalid {
            reason: format!(
                "signer {} is not trusted",
                hex::encode(verifying.as_bytes())
            ),
        };
    }
    let sig_bytes = match hex::decode(&sidecar.signature) {
        Ok(b) => b,
        Err(_) => {
            return SignatureStatus::Invalid {
                reason: "sidecar signature is not valid hex".into(),
            };
        }
    };
    let signature = match <[u8; 64]>::try_from(sig_bytes)
        .ok()
        .map(|b| Signature::from_bytes(&b))
    {
        Some(s) => s,
        None => {
            return SignatureStatus::Invalid {
                reason: "sidecar signature is malformed".into(),
            };
        }
    };
    match verifying.verify(&bytes, &signature) {
        Ok(()) => SignatureStatus::Valid {
            key: hex::encode(verifying.as_bytes()),
        },
        Err(_) => SignatureStatus::Invalid {
            reason: "signature does not verify against the artifact".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keypair() -> SigningKeypair {
        // Deterministic key for tests; production callers use generate_keypair.
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let verifying = signing.verifying_key();
        SigningKeypair { signing, verifying }
    }

    fn trust_for(pair: &SigningKeypair, mode: Enforcement) -> TrustedKeys {
        TrustedKeys::new(vec![pair.verifying.to_bytes()], mode)
    }

    fn temp_artifact(tag: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wf-sign-{tag}"));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("plugin.wasm");
        std::fs::write(&path, bytes).expect("write artifact");
        path
    }

    #[test]
    fn sign_verify_roundtrip_reports_signer() {
        let pair = test_keypair();
        let artifact = temp_artifact("roundtrip", b"fake-wasm-bytes");
        sign_file(&artifact, &pair.signing).expect("sign");
        let status = verify_file(&artifact, &trust_for(&pair, Enforcement::Enforcing));
        assert_eq!(
            status,
            SignatureStatus::Valid {
                key: hex::encode(pair.verifying.to_bytes()),
            }
        );
        let _ = std::fs::remove_dir_all(artifact.parent().expect("parent"));
    }

    #[test]
    fn tampered_artifact_is_invalid() {
        let pair = test_keypair();
        let artifact = temp_artifact("tamper", b"fake-wasm-bytes");
        sign_file(&artifact, &pair.signing).expect("sign");
        std::fs::write(&artifact, b"fake-wasm-BYTES").expect("tamper");
        let status = verify_file(&artifact, &trust_for(&pair, Enforcement::Enforcing));
        assert!(
            matches!(status, SignatureStatus::Invalid { ref reason } if reason.contains("digest")),
            "{status:?}"
        );
        let _ = std::fs::remove_dir_all(artifact.parent().expect("parent"));
    }

    #[test]
    fn untrusted_signer_is_invalid() {
        let pair = test_keypair();
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let artifact = temp_artifact("untrusted", b"fake-wasm-bytes");
        sign_file(&artifact, &other).expect("sign");
        let status = verify_file(&artifact, &trust_for(&pair, Enforcement::Enforcing));
        assert!(
            matches!(status, SignatureStatus::Invalid { ref reason } if reason.contains("not trusted")),
            "{status:?}"
        );
        let _ = std::fs::remove_dir_all(artifact.parent().expect("parent"));
    }

    #[test]
    fn missing_sidecar_is_unsigned() {
        let pair = test_keypair();
        let artifact = temp_artifact("unsigned", b"fake-wasm-bytes");
        assert_eq!(
            verify_file(&artifact, &trust_for(&pair, Enforcement::Enforcing)),
            SignatureStatus::Unsigned
        );
        let _ = std::fs::remove_dir_all(artifact.parent().expect("parent"));
    }

    #[test]
    fn corrupt_sidecar_is_invalid() {
        let pair = test_keypair();
        let artifact = temp_artifact("corrupt", b"fake-wasm-bytes");
        std::fs::write(sidecar_path(&artifact), "{not json").expect("write corrupt sidecar");
        let status = verify_file(&artifact, &trust_for(&pair, Enforcement::Enforcing));
        assert!(
            matches!(status, SignatureStatus::Invalid { .. }),
            "{status:?}"
        );
        let _ = std::fs::remove_dir_all(artifact.parent().expect("parent"));
    }

    #[test]
    fn generated_keypair_is_self_consistent() {
        let pair = generate_keypair();
        let artifact = temp_artifact("keygen", b"fake-wasm-bytes");
        sign_file(&artifact, &pair.signing).expect("sign");
        assert!(verify_file(&artifact, &trust_for(&pair, Enforcement::Enforcing)).is_valid());
        let _ = std::fs::remove_dir_all(artifact.parent().expect("parent"));
    }

    #[test]
    fn hex_key_parsing_rejects_malformed_keys() {
        assert!(TrustedKeys::from_hex_keys(&["zz"], Enforcement::Enforcing).is_err());
        assert!(TrustedKeys::from_hex_keys(&["abcd"], Enforcement::Enforcing).is_err());
        let valid = hex::encode([1u8; 32]);
        assert!(TrustedKeys::from_hex_keys(&[valid.as_str()], Enforcement::Enforcing).is_ok());
    }
}
