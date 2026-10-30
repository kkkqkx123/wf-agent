# example-llm-auth-claude-subscription · 契约样板（sample）

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

## 样板定位（必读）

本包是**契约样板**，不是产品登录链路：

- **是什么**：`PluginLlmAuth` JSON 同步契约 + native FFI 注册 + provider
  bundle（`llm-auth` × `llm_providers`）写法的唯一官方示例，同时是
  manifest 快照测试与 `ffi_smoke.py` 的测试夹具。
- **不是什么**：不随产品构建分发，不在默认插件目录安装，不作为任何
  内置登录链路的运行时依赖（仓库根 `Cargo.toml` 的 `exclude` 即可证明）。
- **产品路径**：Claude Pro/Max 订阅登录（含 browser 双模式）的运行时
  实现是进程内的 `anthropic` preset
  （`crates/infra/wf-llm/src/provider_flows.rs::AnthropicAuth`），
  参数与 pi 对齐，能力是本包的超集。
- **维护规则**：本包登录逻辑冻结，只随 `PluginLlmAuth` 契约本身变更而
  更新；新增登录能力一律改进程内驱动，不要扩展本包。`wf-plugin.toml`
  中的 OAuth 参数是历史示意值（见下节 Q5），产品取值以进程内 preset
  为准。

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
cd .wf-user/plugins/example-llm-auth-claude-subscription
cargo build --release          # produces target/release/libexample_llm_auth_claude_subscription.so
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
   (which registers the `example-claude-subscription` `llm-auth` contribution and
   the `example-claude-subscription` provider definition), and on activation the
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

## ⚠️ 参数为历史示意值（原 open point Q5 结论）

`wf-plugin.toml` 中的 OAuth 参数（`client_id`、`redirect_uri`、
`authorize_url`、`token_url`、`scopes`）是早期跟随 Claude Code 桌面端
的取值，未经真实订阅账号验证，且**已被冻结，不再更新**。生产环境以
进程内 `anthropic` preset 的参数为准（与 pi 实现对齐并经 e2e 覆盖），
不要把本包的参数复制到生产配置中。
