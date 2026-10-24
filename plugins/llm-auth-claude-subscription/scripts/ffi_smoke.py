#!/usr/bin/env python3
"""End-to-end FFI smoke test for the reference plugin cdylib.

Drives the exact C ABI v1 protocol the host uses: two-phase manifest
retrieval, ABI version negotiation, lifecycle hooks, and the `llm-auth`
dispatch ops (describe / login_start / login_poll / login_submit).
Run after `cargo build --release` in the plugin directory.
"""

import ctypes
import json
import sys
import tomllib

SO_PATH = sys.argv[1] if len(sys.argv) > 1 else "target/release/libllm_auth_claude_subscription.so"

lib = ctypes.CDLL(SO_PATH)

# wf_plugin_get_manifest(out: *mut u8, len: *mut usize) -> i32
lib.wf_plugin_get_manifest.argtypes = [ctypes.POINTER(ctypes.c_uint8), ctypes.POINTER(ctypes.c_size_t)]
lib.wf_plugin_get_manifest.restype = ctypes.c_int32
# wf_plugin_supported_abi_versions(out: *mut u32, len: *mut usize) -> i32
lib.wf_plugin_supported_abi_versions.argtypes = [ctypes.POINTER(ctypes.c_uint32), ctypes.POINTER(ctypes.c_size_t)]
lib.wf_plugin_supported_abi_versions.restype = ctypes.c_int32
# wf_plugin_on_load(ctx: *const PluginContextC) -> i32
lib.wf_plugin_on_load.argtypes = [ctypes.c_void_p]
lib.wf_plugin_on_load.restype = ctypes.c_int32
lib.wf_plugin_on_unload.argtypes = [ctypes.c_void_p]
lib.wf_plugin_on_unload.restype = ctypes.c_int32
# wf_plugin_dispatch_handler(type, name, input_json, out, out_len) -> i32
lib.wf_plugin_dispatch_handler.argtypes = [
    ctypes.c_char_p, ctypes.c_char_p, ctypes.c_char_p,
    ctypes.POINTER(ctypes.c_uint8), ctypes.POINTER(ctypes.c_size_t),
]
lib.wf_plugin_dispatch_handler.restype = ctypes.c_int32


def call_two_phase(fn, elem):
    """Shared host pattern: size query with null buffer, then fill."""
    length = ctypes.c_size_t(0)
    rc = fn(None, ctypes.byref(length))
    assert rc == 0, f"size query failed: {rc}"
    assert length.value > 0, "zero size"
    buf = (elem * length.value)()
    written = ctypes.c_size_t(length.value)
    rc = fn(buf, ctypes.byref(written))
    assert rc == 0, f"fill failed: {rc}"
    return bytes(buf[: written.value])


def dispatch(handler_type, handler_name, payload):
    raw = json.dumps(payload).encode()
    return call_two_phase_raw(
        lib.wf_plugin_dispatch_handler,
        handler_type.encode(), handler_name.encode(), raw,
    )


def call_two_phase_raw(fn, a, b, c):
    """Two-phase host dispatch that honours the `rc == 2` grow-and-retry
    contract: the size query (null buffer) may answer with `2` and the
    required length, so we grow the buffer and repeat until `rc == 0`."""
    length = ctypes.c_size_t(0)
    buf = None
    while True:
        if buf is None:
            rc = fn(a, b, c, None, ctypes.byref(length))
        else:
            written = ctypes.c_size_t(length.value)
            rc = fn(a, b, c, buf, ctypes.byref(written))
            if rc == 0:
                return bytes(buf[: written.value])
            length = written
        if rc == 2:
            buf = (ctypes.c_uint8 * length.value)()
            continue
        raise AssertionError(f"dispatch failed with rc={rc}")


class PluginContextC(ctypes.Structure):
    _fields_ = [
        ("abi_version", ctypes.c_uint32),
        ("plugin_id", ctypes.c_char_p),
        ("config_json", ctypes.c_char_p),
    ]


# 1. manifest: two phases, TOML bytes, id cross-check
manifest_bytes = call_two_phase(lib.wf_plugin_get_manifest, ctypes.c_uint8)
manifest = tomllib.loads(manifest_bytes.decode())
print(f"manifest id           : {manifest['id']}")
print(f"manifest plugin_type  : {manifest['plugin_type']}")
print(f"manifest permissions  : {manifest['permissions']}")
print(f"manifest llm_providers: {[p['id'] for p in manifest['llm_providers']]}")
assert manifest["id"] == "llm-auth-claude-subscription"
assert manifest["plugin_type"] == "native"
assert manifest["permissions"] == ["llm_auth", "network"]

# 2. abi negotiation: returns a u32 array (element count first, then values)
def read_abi_versions():
    length = ctypes.c_size_t(0)
    rc = lib.wf_plugin_supported_abi_versions(None, ctypes.byref(length))
    assert rc == 0, f"abi size query failed: {rc}"
    assert length.value >= 1, "no supported abi versions reported"
    arr = (ctypes.c_uint32 * length.value)()
    written = ctypes.c_size_t(length.value)
    rc = lib.wf_plugin_supported_abi_versions(arr, ctypes.byref(written))
    assert rc == 0, f"abi fill failed: {rc}"
    return [arr[i] for i in range(written.value)]


versions = read_abi_versions()
print(f"supported abi versions: {versions}")
assert versions == [1], versions

# 3. lifecycle: on_load then dispatch, finally on_unload
ctx = PluginContextC(1, b"llm-auth-claude-subscription", b"{}")
assert lib.wf_plugin_on_load(ctypes.byref(ctx)) == 0, "on_load failed"

# 4. describe
described = json.loads(dispatch("llm-auth", "claude-subscription/describe", {}))
print(f"describe name         : {described['name']}")
assert described["name"] == "Claude (Pro/Max Subscription)"
assert described["is_subscription"] is True

# 5. login_start: params come from the manifest's auth.params verbatim
auth_params = manifest["llm_providers"][0]["auth"]["params"]
started = json.loads(dispatch("llm-auth", "claude-subscription/login_start", {"config": auth_params}))
print(f"login_start next      : {started['next']['next']}")
print(f"login_start stage     : {started['session']['stage']}")
assert started["next"]["next"] == "awaiting_input"
assert started["next"]["prompt"]["kind"] == "manual_code"
assert started["session"]["stage"] == "awaiting_code"
assert started["session"]["verifier"]
assert started["session"]["state"]
auth_urls = [e for e in started["events"] if e["kind"] == "auth_url"]
assert auth_urls and "code_challenge_method=S256" in auth_urls[0]["url"]

# 6. login_poll: pending heartbeat with the session echoed back
polled = json.loads(dispatch("llm-auth", "claude-subscription/login_poll", {"session": started["session"]}))
assert polled["next"]["next"] == "pending"
assert polled["session"] == started["session"]

# 7. login_submit: state mismatch must fail loudly, no network involved
bad = json.loads(dispatch(
    "llm-auth", "claude-subscription/login_submit",
    {"session": started["session"], "input": "https://console.anthropic.com/oauth/code/callback?code=x&state=wrong"},
))
print(f"login_submit (mismatch): {bad['next']['next']} - {bad['next']['error'][:60]}...")
assert bad["next"]["next"] == "failed"
assert "state mismatch" in bad["next"]["error"]

# 8. unknown op rejected
try:
    dispatch("llm-auth", "claude-subscription/bogus", {})
    raise AssertionError("unknown op should fail")
except AssertionError as error:
    if "should fail" in str(error):
        raise
    print("unknown op rejected   : ok")

assert lib.wf_plugin_on_unload(ctypes.byref(ctx)) == 0, "on_unload failed"
print("\nFFI SMOKE TEST PASSED")
