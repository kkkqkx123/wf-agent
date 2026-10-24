# LLM 认证声明（configs/llm-auth）

声明**内置 provider 如何认证**：端点、client_id、环境变量名——**不含任何 secret**。
令牌与 API key 保存在凭据存储（默认 `~/.wf/auth.json`，0600 权限、原子替换），
本目录只描述"去哪里登录、用什么流"。

## 文件形态

每个 `*.auth.example.toml` 声明一个 provider 的认证面（`AuthProviderConfig`）：

| 字段 | 方法 | 说明 |
| --- | --- | --- |
| `id` | 全部 | provider id，与 `configs/llm-providers/*.provider.toml` 的 `id` 对应 |
| `method` | 全部 | `api_key`（默认）或 `oauth` |
| `display_name` | 全部 | 展示名，如 `"Acme API key"` |
| `login_label` | 全部 | 登录选项的选择器文案，如 `"Sign in with Claude Pro/Max"` |
| `env` | api_key | 无存储凭据时回退读取的环境变量名 |
| `flow` | oauth | `device`（RFC 8628）或 `pkce`（RFC 7636） |
| `is_subscription` | oauth | `true` 表示订阅套餐计费，宿主跳过按 token 计价 |
| `device_authorization_url` | oauth+device | 设备授权端点 |
| `authorize_url` | oauth+pkce | 授权端点 |
| `token_url` | oauth | 令牌端点（两种流都需要） |
| `client_id` | oauth | OAuth client id（公开信息，非 secret） |
| `scope` | oauth | 请求的 scope |
| `redirect_uri` | oauth+pkce | 回调地址 |
| `loopback` | oauth+pkce | `true` 时绑定本地回环监听而非粘贴回调 |

顶层 `store_path` 可覆盖默认凭据存储位置（相对路径按项目根解析）；
不设置即用 `~/.wf/auth.json`——token 是用户身份，跟随人不跟随项目。

## 环境变量插值

所有字符串字段支持 `${VAR}` / `${VAR:default}` 插值（与 workflow 表达式
`${input.a}` 不冲突），便于把端点/client_id 外置到环境：

```toml
token_url = "https://${ACME_TOKEN_HOST}/token"
```

## 校验规则（加载时 fail-fast）

- `method = "api_key"` 必须提供非空 `env`
- `method = "oauth"` 必须提供 `flow`、`token_url`、`client_id`
- `flow = "device"` 必须提供 `device_authorization_url`
- `flow = "pkce"` 必须提供 `authorize_url` 与 `redirect_uri`
- provider `id` 不得重复（重复会静默遮蔽前一个）

## 示例清单

- `api-key-provider.auth.example.toml`：第三方网关，环境变量 API key
- `device-flow-subscription.auth.example.toml`：设备流订阅登录（Claude 风格，参数为占位草案）
- `pkce-subscription.auth.example.toml`：PKCE 订阅登录（GitLab Duo 风格，粘贴回调模式）

插件可以提供更丰富的认证面（同一 provider id 覆盖内置声明），
参见 `docs/infra/plugin/custom-provider.md`。
