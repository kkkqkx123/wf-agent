# 自定义 Provider 集成（插件侧）

本文件说明插件作者如何把一个 LLM provider（尤其是订阅套餐类、需要 OAuth 登录的 provider）接入 wf-agent。定义以代码实现为准；设计背景见 `docs/plan/llm-subscription-provider-integration-design.md`。

## 1. 最小集成决策表

先决定你要贡献到哪一层，再决定用什么后端。三后端（Lua / native / wasm）的 **auth 注册路径都已接通**，但能否真正完成 OAuth 取决于后端是否允许出站 HTTP。

| 你想做的事 | 是否需要 `llm_auth` 贡献 | 可选后端 | 说明 |
|---|---|---|---|
| 只贡献一个新 wire 协议（如新格式编解码） | 否 | Lua / native / wasm | 只写 `llm_codecs` / codec 注册，不碰登录 |
| 贡献一个 provider 连接模板（静态端点 + api_key） | 否 | Lua / native / wasm | manifest 声明式 `[[llm_providers]]`，`auth_type = "bearer"` 或 `"x-api-key"` |
| 贡献订阅套餐登录（OAuth token 交换 / refresh） | **是** | **仅 native** | Lua 沙箱只注入安全 `print` 且禁 `require`；wasm 沙箱只 import `host-log`；两者均无 HTTP，token 交换只能在 native cdylib 中完成 |
| 同时贡献 codec + 定义 + 登录（"provider bundle"） | 是 | 仅 native（若含 OAuth） | 见第 3 节形态 B |

> 结论：**凡是插件内要发起 OAuth 网络调用的订阅 provider，必须是 native 后端**。auth *注册* 三后端都支持，但 auth *实现* 需要网络时只能 native。

## 2. 两种注册形态

provider 集成有两种形态，由 manifest 声明与运行时注册共同决定。

### 形态 A：codec + 声明式 provider 定义（无登录）

- manifest 通过 `[[llm_providers]]` 声明连接模板（`format` / `base_url` / `auth_type`）。
- 若 provider 需要新 wire 协议，额外贡献 codec（native FFI `register_llm_provider`、Lua `contribs.llm_codecs`、wasm `decl.llm_providers`）。
- `auth` 段可省略，或只标 `auth_type` + `method = "api_key"`，凭据由用户直接填入 profile。
- 引擎在 `engine.rs` 的 `sync_manifest_llm_providers` 把声明式定义转成 `LlmProviderDefinition` 注册（**不再要求 `llm_codec` 权限**，仅当你真实贡献 codec 时才需要声明它，见第 5 节门控说明）。

### 形态 B：provider bundle（含订阅登录）

- 一个插件同时贡献：
  - 一个 `llm-auth` 实现（`PluginLlmAuth`：`describe` / `login_start` / `login_poll` / `login_submit` / `refresh` / `resolve`）；
  - 一个 `[[llm_providers]]` 条目，其 `[llm_providers.auth]` 的 `provider` 指向该 `llm-auth` 贡献名，`method = "oauth"`，`is_subscription = true`，并把 OAuth 公开参数放进 `[llm_providers.auth.params]`。
- 宿主在插件激活时：先 `sync_llm`（注册 provider 定义），再 `sync_auth`（把 `llm-auth` 贡献适配为 `ProviderAuth` 注册进 gateway 的 auth registry，键 = **auth 贡献名**，而非 provider definition id），随后校验"声明了 `auth` 的 provider 必须引用一个已注册的同名 `llm_auth` 贡献，否则激活失败（大声报错）"。
- 参考实现：`.wf-user/plugins/llm-auth-claude-subscription/`（native，PKCE 粘贴流），随仓库分发，是唯一官方样板。

## 3. 登录流程（llm-auth 契约）

`PluginLlmAuth` 走与 `PluginLlmCodec` 同风格的 **JSON 同步契约**。宿主把插件适配为 llm-suite `ProviderAuth` 后注册进 gateway；异步登录轮询由宿主驱动，插件侧保持无状态。

### 宿主侧对接点

- 入口：`wf-plugin-sdk` 的 `export_plugin!` 宏 → `register_llm_auth(ctx, name)`（`crates/foundation/wf-plugin-sdk/src/plugin.rs:57`）。
- native 运行时：`wf-plugin/src/native/plugin.rs` 的 `ffi_register_llm_auth`，dispatch 类型 `"llm-auth"`，handler 名 `"{贡献名}/{op}"`，输出缓冲沿用现有扩容重试协议（缓冲区不足返回 `2` 并带所需长度，宿主扩容重试）。
- Lua：`lua/plugin.rs` 读取 `contribs.llm_auths` 表并 `register_llm_auth`。
- wasm：`wasm/component.rs` / `wasm/plugin.rs` 遍历 `decl.llm_auths` 注册。
- 桥接注册：`crates/app/wf-runtime/src/plugin_bridge.rs` 的 `sync_auth`（按 owner 过滤 `manager.all_llm_auths()`，注册进 gateway，并在 login 完成 / logout 时按 `auth_name` 驱逐 client 缓存）。

### 插件侧六个 op 与输入 JSON 形状

| op | 输入 JSON | 输出（核心字段） |
|---|---|---|
| `describe` | `{}` | `AuthDescriptor{ name, is_subscription, login_label }` |
| `login_start` | `{ "config": <auth.params> }` | `PluginLoginStep{ events, session, next }`，`next = AwaitingInput{ prompt: ManualCode }` |
| `login_poll` | `{ "session": <上一步 session> }` | `next = Pending`（心跳），`session` 原样回显 |
| `login_submit` | `{ "session": …, "input": <粘贴的回调 URL 或裸 code> }` | 成功 `next = Completed{ credential }`；失败 `next = Failed{ error }` |
| `refresh` | `{ "credential": <已存凭据> }` | 同 `login_submit` 输出形状 |
| `resolve` | `{ "credential": … }` | `ModelAuth{ api_key, is_subscription, source, … }` 供请求管线使用 |

凭据 JSON 为扁平结构：`{ access, refresh, expires, is_subscription, extra? }`，宿主解析为 `OAuthCredential` 再包成 `Credential::OAuth` 持久化；插件 **不** 写盘。

## 4. 一个 native 参考插件的最小骨架

```toml
# wf-plugin.toml
id = "llm-auth-claude-subscription"
plugin_type = "native"
entry_point = "target/release/lib<name>.so"
contributions = ["claude-subscription"]
permissions = ["llm_auth", "network"]

[[llm_providers]]
id = "claude-subscription"
format = "ANTHROPIC"
base_url = "https://api.anthropic.com"
auth_type = "bearer"

[llm_providers.auth]
provider = "claude-subscription"   # 指向本插件的 llm-auth 贡献名
method = "oauth"
is_subscription = true

[llm_providers.auth.params]
authorize_url = "…"
token_url = "…"
client_id = "…"
redirect_uri = "…"
scopes = "…"
flow = "pkce_paste"
```

```rust
// src/lib.rs（节选）
use wf_plugin_sdk::{export_plugin, AuthDescriptor, PluginLlmAuth, /* … */ WfNativePlugin};

struct ClaudeSubscriptionPlugin;
impl WfNativePlugin for ClaudeSubscriptionPlugin {
    fn manifest() -> PluginManifest { /* 与 wf-plugin.toml 对齐 */ }
    fn register(registrar: &NativeRegistrar) {
        registrar.register_llm_auth("claude-subscription");
    }
}
impl PluginLlmAuth for ClaudeSubscriptionAuth {
    // describe / login_start / login_poll / login_submit / refresh / resolve
}
export_plugin!(ClaudeSubscriptionPlugin);
```

`permissions` 必须含 `llm_auth`（要注册登录逻辑）与 `network`（要访问 provider OAuth 端点）。`llm_codec` **不是** provider bundle 的必需项——除非你真的贡献了新 wire 协议。

## 5. 权限门控说明（易错点）

- manifest 声明 `[[llm_providers]]` 不再要求 `llm_codec` 权限（`engine.rs` 的门控在 P4 已修正：provider 定义注册与 codec 权限解耦，缺 `llm_codec` 不再跳过整个 provider 列表）。只有当插件**确实贡献了 codec** 时才声明 `llm_codec`。
- 声明了 `auth` 段但缺 `llm_auth` 权限：宿主在转换时丢弃 auth 引用并告警（不会静默成功）。
- 声明了 `auth` 段却未注册同名 `llm_auth` 贡献：激活失败，错误信息含"缺少哪个贡献 + 如何补"，对齐插件系统既有的"声明优先、失败大声"。

## 6. 测试清单

插件作者应覆盖：

1. **manifest 快照**：`manifest()` 序列化的 TOML 与磁盘 `wf-plugin.toml` 逐字段一致（`manifest_snapshot_matches_toml_file`）。
2. **describe**：`is_subscription`、`login_label` 正确。
3. **login_start**：生成 PKCE S256 `code_challenge` 且其确为 `code_verifier` 的 S256；authorize URL 含 `code_challenge_method=S256`；返回 `AwaitingInput{ ManualCode }`。
4. **login_poll**：`Pending` 心跳，`session` 原样回显。
5. **login_submit**：
   - 解析粘贴的回调 URL（含 query 与 fragment）与裸 code 都成功；
   - `state` 不匹配必须 `Failed` 且错误信息含 "state mismatch"；
   - 缺 `code` 必须 `Failed`；
   - 真实 token 交换（需 mock HTTP）产出 `Completed{ credential }`。
6. **refresh**：从凭据 `extra` 恢复参数并重建 token 请求（无需再次传入 `auth.params`）。
7. **resolve**：产出 `ModelAuth`，`is_subscription = true`。
8. **credential 契约**：`refresh_token` 缺失应被拒绝；`expires` 已扣减提前刷新余量。
9. **FFI 端到端**：用 `python3 scripts/ffi_smoke.py` 直接加载 `.so`，驱动两阶段 manifest 取回、ABI 版本协商、`on_load`/`on_unload` 生命周期与 `llm-auth` 各 op，验证宿主所见即所得。

宿主侧（wf-runtime）应额外覆盖 `sync_auth` / `unsync_auth`：激活时注册、卸载时移除且 profile 快照保留；provider 引用未注册 auth 时激活失败。
