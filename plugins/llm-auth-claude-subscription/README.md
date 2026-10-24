# llm-auth-claude-subscription

Reference **native** plugin for wf-agent that wires a subscription-backed
LLM provider (Claude Pro/Max) into the host through the `llm-auth`
contribution point.

It is the canonical sample for the "provider bundle" integration path
(see `docs/infra/plugin/custom-provider.md`): one plugin contributes both

- an **`llm-auth`** implementation — the OAuth 2.0 login / refresh /
  resolve logic, registered from `src/lib.rs` over the native C ABI, and
- an **`llm_providers`** entry — the connection template (ANTHROPIC wire
  format), whose `auth` section binds the provider to the auth
  contribution above and carries the public OAuth parameters.

The host owns the login session state machine, the credential store and
the request pipeline. This crate only speaks the synchronous JSON
contract of `PluginLlmAuth` and performs the HTTP token exchange against
the provider's OAuth endpoints.

## Why native (not Lua or wasm)

Subscription login requires the plugin to make outbound HTTPS calls
(token exchange, refresh). wf-agent's Lua sandbox injects only a safe
`print` and disables `require`; the wasm sandbox imports only `host-log`.
Neither backend provides an HTTP capability, so the token exchange can
only run in a **native** cdylib. The auth *registration* path exists for
all three backends, but a plugin that actually performs OAuth must be
native.

## Build

This directory is a standalone cargo workspace (it must not join the
wf-agent workspace). It links `wf-plugin-sdk` via a relative path.

```bash
cd plugins/llm-auth-claude-subscription
cargo build --release          # produces target/release/libllm_auth_claude_subscription.so
cargo test                     # 14 unit tests + TOML manifest snapshot check
python3 scripts/ffi_smoke.py   # drives the C ABI end-to-end (needs the release .so)
```

Requires the `stable` Rust toolchain. The build pulls `reqwest` (rustls,
blocking), `sha2`, `base64`, `getrandom`, `url`, `serde` and
`wf-plugin-sdk`.

## Install

1. Build the release `.so` as above.
2. Copy the whole plugin directory (including `wf-plugin.toml` and the
   built `target/release/*.so`) into the host's plugin directory so the
   `entry_point` in `wf-plugin.toml` resolves relative to the directory.
3. The host loads the manifest, calls `wf_plugin_register_contributions`
   (which registers the `claude-subscription` `llm-auth` contribution and
   the `claude-subscription` provider definition), and on activation the
   bridge registers the auth into the gateway's auth registry.

## Login flow (PKCE "paste" variant)

`flow = "pkce_paste"`:

1. `login_start` generates a PKCE S256 `code_verifier`/`code_challenge`
   and a random `state`, renders the provider authorize URL, and returns
   an `AwaitingInput` step with a `ManualCode` prompt. No local loopback
   server is started, so it works headless.
2. The user opens the URL, approves, and the provider shows a callback
   URL (or bare code) which the user pastes back.
3. `login_submit` parses the pasted input, **validates `state`** (rejects
   mismatches loudly), and performs the token exchange (`POST token_url`,
   `grant_type=authorization_code`) to obtain the access/refresh tokens.
   The OAuth parameters are embedded into the credential's `extra` so
   `refresh` can rebuild the request without them.
4. `login_poll` returns a `Pending` heartbeat while waiting for the paste.
5. `refresh` issues a `grant_type=refresh_token` call using the embedded
   parameters.
6. `resolve` turns the stored credential into a `ModelAuth`
   (`api_key = access_token`, `is_subscription = true`) for the request
   pipeline.

## Credential contract

The plugin emits the flat credential JSON the host expects
(`{ access, refresh, expires, is_subscription, extra? }`). `expires` is
`expires_in` minus a 60s margin so a near-boundary token is refreshed one
request early. Credentials are persisted by the host's credential store;
this plugin never writes them to disk.

## ⚠️ Real-account verification required (open point Q5)

The OAuth parameters in `wf-plugin.toml` (`client_id`, `redirect_uri`,
`authorize_url`, `token_url`, `scopes`) follow the public Claude Code
desktop client and are **unverified**. Confirm them against a real Claude
Pro/Max subscription account and the current Anthropic OAuth
documentation before shipping. The plugin logic is backend-neutral; only
those parameters may need to change.
