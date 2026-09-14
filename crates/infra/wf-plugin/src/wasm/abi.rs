use serde_json::Value;
use wasmtime::{Memory, Store};

use wf_plugin_sdk::wasm::{export, WasmContributionDecl, WasmHookInput};

use super::pool::WasmStoreState;
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
    if code == 0 {
        Ok(())
    } else {
        Err(PluginError::WasmError(format!(
            "plugin '{plugin_id}' hook '{export_name}' returned {code}"
        )))
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
    fn decl_rejects_invalid_json() {
        assert!(decode_decl(b"not json").is_err());
        let decl = decode_decl(br#"{"tool_types":["t"]}"#).expect("valid decl");
        assert_eq!(decl.tool_types, vec!["t".to_owned()]);
    }
}
