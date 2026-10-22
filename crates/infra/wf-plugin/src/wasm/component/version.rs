use super::*;

/// Pinned component-world version. The WIT package
/// (`wit/plugin.wit`, `wf:plugin@1.0.0`) and these bindings must agree.
/// Compatibility compares only the major version; minor differences use
/// feature detection. A major mismatch fails loading with a fix suggestion.
pub(crate) const WF_COMPONENT_WORLD_VERSION: &str = "1.0.0";

/// Major version of a dotted version string.
pub(crate) fn world_major(version: &str) -> Option<u64> {
    version.split('.').next()?.trim().parse().ok()
}

/// Host component-world major version.
pub(crate) fn host_world_major() -> u64 {
    world_major(WF_COMPONENT_WORLD_VERSION).unwrap_or(1)
}

/// Guest component-world majors found in the binary. Scans for embedded
/// `wf:plugin/...@<version>` names covering both exports and imports.
/// Empty when the binary carries no recognizable world name; callers let
/// compilation proceed so genuine format errors surface downstream.
pub(crate) fn guest_world_majors(bytes: &[u8]) -> Vec<u32> {
    const NEEDLE: &[u8] = b"wf:plugin/";
    let mut majors = Vec::new();
    let mut offset = 0;
    while offset + NEEDLE.len() < bytes.len() {
        let Some(found) = bytes[offset..]
            .windows(NEEDLE.len())
            .position(|w| w == NEEDLE)
        else {
            break;
        };
        let mut cursor = offset + found + NEEDLE.len();
        while cursor < bytes.len() && bytes[cursor] != b'@' && cursor < offset + found + 96 {
            cursor += 1;
        }
        if cursor < bytes.len() && bytes[cursor] == b'@' {
            cursor += 1;
            let mut major: u32 = 0;
            let mut digits = 0;
            while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                major = major
                    .saturating_mul(10)
                    .saturating_add((bytes[cursor] - b'0') as u32);
                cursor += 1;
                digits += 1;
            }
            if digits > 0 && !majors.contains(&major) {
                majors.push(major);
            }
        }
        offset += found + NEEDLE.len();
    }
    majors.sort_unstable();
    majors
}

/// Explicit world-major gate mirroring the core-module handshake error
/// shape. Runs before compilation so a skewed guest fails with both sides
/// listed instead of a raw bind error.
pub(crate) fn check_component_world_version(plugin_id: &str, bytes: &[u8]) -> PluginResult<()> {
    let guest = guest_world_majors(bytes);
    if guest.is_empty() {
        return Ok(());
    }
    let host = host_world_major() as u32;
    if guest.iter().all(|major| *major == host) {
        return Ok(());
    }
    Err(PluginError::LoadFailed(format!(
        "plugin '{plugin_id}' component world majors {guest:?}, host supports [{host}]; no compatible version found. Update the plugin or upgrade the host."
    )))
}
