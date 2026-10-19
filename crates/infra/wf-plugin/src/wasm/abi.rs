use serde_json::Value;
use wasmtime::{Engine, Memory, Store};

use wf_plugin_sdk::wasm::{
    export, WasmContributionDecl, WasmHookInput, WF_WASM_SUPPORTED_ABI_VERSIONS,
};

use super::policy::WasmLimits;
use super::pool::{outer_timeout_ms, PooledSession, WasmStoreState};
use crate::error::{PluginError, PluginResult};

pub use export::*;

/// Upper bound for a single message crossing the host/guest boundary.
/// Guards the host against a malicious guest returning huge `(ptr, len)`.
pub const MAX_GUEST_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

/// Pack a `(ptr, len)` pair into the i64 the guest ABI uses for returns.
pub fn pack_ptr_len(ptr: u32, len: u32) -> u64 {
    (ptr as u64) | ((len as u64) << 32)
}

/// Unpack a guest-returned i64 into `(ptr, len)`.
pub fn unpack_ptr_len(packed: u64) -> (u32, u32) {
    (packed as u32, (packed >> 32) as u32)
}

/// Read `len` bytes at `ptr` from guest linear memory.
pub fn read_bytes(
    store: &Store<WasmStoreState>,
    memory: &Memory,
    ptr: u32,
    len: u32,
) -> PluginResult<Vec<u8>> {
    let len = len as usize;
    if len > MAX_GUEST_MESSAGE_BYTES {
        return Err(PluginError::WasmError(format!(
            "guest returned oversized message: {len} bytes"
        )));
    }
    let mut buf = vec![0u8; len];
    memory
        .read(store, ptr as usize, &mut buf)
        .map_err(|e| PluginError::WasmError(format!("guest memory read failed: {e}")))?;
    Ok(buf)
}

/// Write `data` at `offset` in guest linear memory.
pub fn write_bytes(
    store: &mut Store<WasmStoreState>,
    memory: &Memory,
    offset: u32,
    data: &[u8],
) -> PluginResult<()> {
    if data.len() > MAX_GUEST_MESSAGE_BYTES {
        return Err(PluginError::WasmError(format!(
            "host message too large: {} bytes",
            data.len()
        )));
    }
    memory
        .write(store, offset as usize, data)
        .map_err(|e| PluginError::WasmError(format!("guest memory write failed: {e}")))?;
    Ok(())
}

/// Serialize a lifecycle-hook input envelope.
pub fn encode_hook_input(plugin_id: &str, config: &Value) -> PluginResult<Vec<u8>> {
    let input = WasmHookInput {
        plugin_id: plugin_id.to_owned(),
        config: config.clone(),
    };
    serde_json::to_vec(&input)
        .map_err(|e| PluginError::WasmError(format!("hook input serialize failed: {e}")))
}

/// Parse the JSON returned by the `wf_register` export.
pub fn decode_decl(bytes: &[u8]) -> PluginResult<WasmContributionDecl> {
    serde_json::from_slice(bytes)
        .map_err(|e| PluginError::WasmError(format!("register decl parse failed: {e}")))
}

/// Map a guest hook status code to a host result.
pub fn check_hook_status(export_name: &str, plugin_id: &str, code: u32) -> PluginResult<()> {
    check_hook_status_with_detail(export_name, plugin_id, code, None)
}

/// Map a guest hook status code plus an optional guest-provided detail
/// string (from the `wf_last_error` export) to a host result.
pub fn check_hook_status_with_detail(
    export_name: &str,
    plugin_id: &str,
    code: u32,
    detail: Option<&str>,
) -> PluginResult<()> {
    if code == 0 {
        Ok(())
    } else {
        match detail {
            Some(text) if !text.is_empty() => Err(PluginError::WasmError(format!(
                "plugin '{plugin_id}' hook '{export_name}' failed with code {code}: {text}"
            ))),
            _ => Err(PluginError::WasmError(format!(
                "plugin '{plugin_id}' hook '{export_name}' failed with code {code}"
            ))),
        }
    }
}

/// Negotiate the core-module ABI version with one live session.
///
/// Guests must export `wf_supported_abi_versions() -> (ptr, len)` returning a
/// packed pointer to a `u32` array of supported ABI major versions. The host
/// selects the highest mutually-supported version. A missing export, an
/// empty list, or an empty intersection all fail loading with a fix
/// suggestion.
pub async fn negotiate_abi_version(
    session: &mut PooledSession,
    engine: &Engine,
    plugin_id: &str,
    limits: &WasmLimits,
) -> PluginResult<u32> {
    if session
        .instance
        .get_func(&mut session.store, export::SUPPORTED_ABI_VERSIONS)
        .is_none()
    {
        return Err(PluginError::LoadFailed(format!(
            "plugin '{plugin_id}' does not export '{}'; it must declare the ABI major versions it supports (host supports {WF_WASM_SUPPORTED_ABI_VERSIONS:?}). Update the plugin to implement the current handshake.",
            export::SUPPORTED_ABI_VERSIONS
        )));
    }
    negotiate_supported_versions(session, engine, plugin_id, limits).await
}

async fn negotiate_supported_versions(
    session: &mut PooledSession,
    engine: &Engine,
    plugin_id: &str,
    limits: &WasmLimits,
) -> PluginResult<u32> {
    let versions_fn = session
        .instance
        .get_typed_func::<(), u64>(&mut session.store, export::SUPPORTED_ABI_VERSIONS)
        .map_err(|e| {
            PluginError::LoadFailed(format!(
                "plugin '{plugin_id}' export '{}' has an unexpected signature: {e}",
                export::SUPPORTED_ABI_VERSIONS
            ))
        })?;
    super::pool::arm_epoch(engine, &mut session.store, limits.call_timeout_ms);
    let call = versions_fn.call_async(&mut session.store, ());
    let packed = match outer_timeout_ms(limits.call_timeout_ms) {
        Some(ms) => tokio::time::timeout(std::time::Duration::from_millis(ms), call)
            .await
            .map_err(|_| PluginError::Timeout {
                plugin_id: plugin_id.to_owned(),
            })?
            .map_err(|e| {
                super::pool::wasm_err(
                    &format!("plugin '{plugin_id}' supported abi versions probe failed"),
                    e,
                )
            })?,
        None => call.await.map_err(|e| {
            super::pool::wasm_err(
                &format!("plugin '{plugin_id}' supported abi versions probe failed"),
                e,
            )
        })?,
    };
    let (ptr, len) = unpack_ptr_len(packed);
    if len == 0 {
        return Err(PluginError::LoadFailed(format!(
            "plugin '{plugin_id}' exported empty supported abi versions list"
        )));
    }
    let memory = session
        .instance
        .get_memory(&mut session.store, "memory")
        .ok_or_else(|| PluginError::WasmError("guest has no memory export".into()))?;
    let bytes = read_bytes(&session.store, &memory, ptr, len * 4)?;
    let versions: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    let host_versions = WF_WASM_SUPPORTED_ABI_VERSIONS;
    let mut common: Vec<u32> = versions
        .iter()
        .copied()
        .filter(|v| host_versions.contains(v))
        .collect();
    common.sort_unstable();
    match common.pop() {
        Some(v) => Ok(v),
        None => Err(PluginError::LoadFailed(format!(
            "plugin '{plugin_id}' supports abi versions {versions:?}, host supports {host_versions:?}; no compatible version found. Update the plugin or upgrade the host."
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ptr_len_pack_round_trip() {
        for (ptr, len) in [(0, 0), (1, 27), (u32::MAX, u32::MAX)] {
            assert_eq!(unpack_ptr_len(pack_ptr_len(ptr, len)), (ptr, len));
        }
    }

    #[test]
    fn hook_status_zero_is_success() {
        assert!(check_hook_status("wf_on_load", "p", 0).is_ok());
        assert!(check_hook_status("wf_on_load", "p", 1).is_err());
    }

    #[test]
    fn hook_status_detail_carries_code_and_text() {
        let err = check_hook_status_with_detail("wf_on_load", "p", 7, None).expect_err("fails");
        let text = err.to_string();
        assert!(text.contains("code 7"), "got: {text}");
        assert!(!text.contains("bad config"));

        let err = check_hook_status_with_detail("wf_on_load", "p", 7, Some("bad config"))
            .expect_err("fails");
        let text = err.to_string();
        assert!(text.contains("code 7"), "got: {text}");
        assert!(text.contains("bad config"), "got: {text}");

        assert!(check_hook_status_with_detail("wf_on_load", "p", 0, Some("stale")).is_ok());
    }

    #[test]
    fn decl_rejects_invalid_json() {
        assert!(decode_decl(b"not json").is_err());
        let decl = decode_decl(br#"{"tool_types":["t"]}"#).expect("valid decl");
        assert_eq!(decl.tool_types, vec!["t".to_owned()]);
    }
}
