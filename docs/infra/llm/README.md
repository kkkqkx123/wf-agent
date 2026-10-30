# LLM 认证（OAuth）文档

本目录是 LLM 侧认证能力的说明文档：通用 OAuth 流程、声明式 preset、进程内
provider 专用驱动。定义以代码实现为准；设计背景见
`docs/plan/oauth-login-forms-design.md`，
承载形态决策见 `docs/plan/builtin-plugin-placement-design.md`。

## 分层总览

```text
llm-suite-auth（独立仓库，经 submodule + path 依赖消费）
  通用 RFC 流程：Device / PKCE loopback / PKCE paste，token 请求 form/JSON
  会话状态机 LoginSession + UI-neutral 交互协议 + 凭据存储抽象
        ↑
wf-types（配置层：AuthProviderConfig + 7 preset 模板 + expand 规则）
        ↑
wf-llm（运行时：DeclaredOAuth 通用声明式实现 + oauth_presets + provider_flows 专用驱动）
        ↑
wf-config（processor：批量展开 + 校验） → wf-runtime/bootstrap（按 preset 名分发驱动）
```

MCP 的 OAuth 与此完全独立（存储、会话均不复用），见其设计方案
（`docs/plan/oauth-login-forms-design.md` §5.5），本目录不覆盖。

## 文档索引

| 文件 | 内容 |
|---|---|
| `oauth-flows.md` | 通用流程：Device / PKCE loopback / PKCE paste、token 双编码、会话状态机、交互协议、凭据与存储、`ModelAuth` |
| `oauth-presets.md` | 声明式配置：`AuthProviderConfig` 字段、7 preset 参数表、展开规则（显式值优先）、校验 |
| `provider-drivers.md` | 进程内专用驱动：`ExternalFlowDriver` 契约、bootstrap 分发、Copilot / Anthropic / Meta 三个驱动的行为、`to_auth` 语义、测试矩阵、第三方扩展路径 |

## 关键不变量

- **显式值永远优先于 preset 模板**：preset 只填缺失项（`expand_auth_preset`）。
- **preset 分两类**：纯声明式（`openai-codex`、`google-vertex`、`xai`、`kimi-coding`，
  标准 RFC 流程）与绑定驱动的入口别名（`github-copilot`、`anthropic`、`meta-muse`，
  bootstrap 按名分发到专用驱动）。
- **通用层不为单个 provider 加一次性开关**：authorize 额外参数、token 额外字段、
  非标准响应映射一律归进程内驱动，不进 `llm-suite-auth`。
- **不新增外部插件承载内置登录**：第三方仍可用 `PluginLlmAuth` 契约扩展，
  唯一官方样板是冻结的 `example-llm-auth-claude-subscription`（只教契约写法）。
