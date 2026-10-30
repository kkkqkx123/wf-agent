# 声明式 OAuth 配置与 Preset

本文件说明配置层（`wf-types/src/config/llm_auth.rs`）的 `AuthProviderConfig`、
7 个内置 preset 模板，以及校验层（`wf-config/src/processor/llm_auth.rs`）的
展开与校验规则。

## `AuthProviderConfig` 字段

`id`（键，匹配 provider 定义 id 并 keying 凭据存储）、`method`（`api_key` /
`oauth`）、`display_name`、`login_label`、`env`、`flow`（`device` / `pkce`）、
`is_subscription`、各端点（`device_authorization_url` / `authorize_url` /
`token_url`）、`client_id`、`scope`、`redirect_uri`、`loopback`、
`token_content_type`（`None` 即 form）、`preset`（模板名，用后即清除）。

## Preset 参数表

| preset | flow | 授权/设备端点 | token 端点 | client | scope | 编码 | 备注 |
|---|---|---|---|---|---|---|---|
| `openai-codex` | PKCE loopback | `auth.openai.com/authorize` | `auth.openai.com/oauth/token` | `codex-cli` | `openid profile email offline_access` | form | 订阅；redirect 固定 `localhost:1455` |
| `github-copilot` | device | `github.com/login/device/code` | `github.com/login/oauth/access_token` | `Iv1.b507a08c87ec2d4b` | `read:user` | form | 绑定驱动：第二段交换见 `provider-drivers.md` |
| `google-vertex` | PKCE | `accounts.google.com/.../auth` | `oauth2.googleapis.com/token` | `vertex-cli` | `cloud-platform` | form | 非订阅 |
| `xai` | device | `auth.x.ai/oauth2/device/code` | `auth.x.ai/oauth2/token` | `b1a00492-…` | `openid profile email offline_access grok-cli:access api:access` | form | 标准 RFC 8628 |
| `kimi-coding` | device | `auth.kimi.com/.../device_authorization` | `auth.kimi.com/.../token` | `17e5f671-…` | 无（服务端发 coding 域 token） | form | 标准 RFC 8628 |
| `anthropic` | PKCE | `claude.ai/oauth/authorize` | `platform.claude.com/v1/oauth/token` | `0d1c250a-…` | `org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload` | JSON | 绑定驱动：双模式选择、`state` 回传、`code=true`、beta 头 |
| `meta-muse` | device | `auth.meta.com/oidc/device/authorization/` | `auth.meta.com/oidc/device/token/` | `1031625952748946` | 无（device 请求只带 `client_id`，与 pi 一致；显式 scope 暂不发送） | form | 绑定驱动：identity→API key mint |

运行时（`wf-llm/src/auth.rs::oauth_presets`）有与上表一一对应的纯数据构造，
`preset_named` 按名解析（大小写敏感，未知返回 `None`），`preset_names`
与配置层名单保持同步（有覆盖测试）。

## 展开与校验规则

- `expand_auth_preset`：`preset` 有值时按名取模板，**缺失字段逐个填补，
  显式值永远优先**；成功后清除 `preset` 标记，下游看到的是普通声明；
  未知模板名直接报错。
- `wf-config` 的 `expand_auth_presets` 对 providers 批量展开 + 未知名即配置错误；
  `token_content_type` 无需新增校验（`None` 视为 form）。
- bootstrap 在展开后仍保留原始声明的 `preset` 标记，专供"绑定驱动的入口别名"
  做分发（见 `provider-drivers.md`）；显式端点字段在驱动内同样优先。
