# 进程内 Provider 专用驱动

本文件说明 `wf-llm/src/provider_flows.rs` 的三个专用驱动、bootstrap 分发、
`to_auth` 语义与测试矩阵，以及第三方扩展路径。

## 为什么是进程内驱动

Provider-specific 语义（二次 token 交换、API key mint、JSON 交换带 `state`、
动态参数）既不能进通用声明式流程，也不适合外部 native 插件：插件默认关闭、
多平台产物矩阵与签名负担重、进程内 `dlopen` 无沙箱。决策全文见
`docs/plan/builtin-plugin-placement-design.md`。驱动实现宿主
`ExternalFlowDriver` 契约，随二进制版本化。

## 分发

`wf-runtime/src/bootstrap/llm.rs::provider_auth_surface` 按**原始声明的 preset 名**
选择驱动（`provider_flow_surface`）：`github-copilot`、`anthropic`、`meta-muse`
走专用驱动，其余走通用声明式流程。显式值优先在驱动内依然成立，但粒度各异：
client_id / scope / token 相关 URL 按驱动逐个透传（`AnthropicParams.token_url`、
`MetaMuseParams.device/token_url` 等，e2e 即靠该覆盖指向 MockServer）；唯独
Copilot 的三端点生产上恒按登录 domain 派生（`endpoints` 字段只做测试 seam，
配置层改 device/token URL 不会生效，domain 输入框才是其覆盖机制）。

## 三个驱动

### GitHub Copilot（`GithubCopilotAuth`）

Device flow（企业域名可选，空即 github.com）→ 单步 `poll`
（`authorization_pending` 继续、`slow_down` 加倍间隔、`access_denied`/
`expired_token` 失败）→ 成功后用 Copilot 专有 headers
（`Editor-Version` 等）`GET copilot_internal/v2/token` 做第二段交换。
GitHub token 存为 `refresh`（refresh 即用存量重 mint），Copilot token 存为
`access`；`to_auth` 从 token 内 `proxy-ep` 派生 per-request base URL
（如 `proxy.individual.githubcopilot.com` → `api.individual.githubcopilot.com`）。
401/403 视为登录失效。

### Anthropic（`AnthropicAuth`）

`initial` 即 `Select`（browser / copy-code，与 pi 同文案）。browser 模式起
loopback 监听（首选 53692，占用回退随机端口，redirect_uri 随绑定口改写），
后台任务单次 accept 后 `poll` 非阻塞收码；copy-code 模式接受回调 URL /
`code#state` / 裸码并做 `state` 一致性校验。授权 URL 带 `code=true` + PKCE
S256；code 交换与 refresh 均为 JSON 体且携带 `state`；到期取 5 分钟余量。
`to_auth` 携带 `anthropic-beta: oauth-2025-04-20`
（`anthropic-version` 由 ANTHROPIC codec 另行设置）。

### Meta Muse（`MetaMuseAuth`）

Device flow（优先 `verification_uri_complete`）→ identity token →
`POST api.meta.ai/muse-code/key` mint（`x-api-version: 1.0.0`，空 JSON 体）。
identity 存 `refresh`、mint key 存 `access`（`is_subscription=true`）；
mint 返回 401/403 即判会话死亡；缺 `api_key` 时附 `action_url` hint。
refresh 即用存量 identity 重 mint。

## 第三方扩展路径

外部 `PluginLlmAuth` 契约保留给第三方作者（注册/桥接见
`docs/infra/plugin/custom-provider.md`）。唯一官方样板
`example-llm-auth-claude-subscription` 已冻结为契约示例与测试夹具：
登录逻辑不再扩展，OAuth 参数为历史示意值，新增登录能力一律走本文件所述
进程内驱动。

## 测试矩阵

- 驱动内 `#[cfg(test)]`：参数默认/覆盖、域名归一化、https 校验、轮询分支、
  proxy 派生、authorize URL 组装、粘贴解析、回调解析、method 选择。
- `wf-llm/tests/provider_flows_e2e.rs`（MockServer 全链路）：copilot 轮询→交换→
  refresh、meta 轮询→mint→refresh、anthropic copy-code 交换、browser 真 loopback
  回调、refresh 复用旧 token。
- `wf-runtime` bootstrap 测试：三 preset 分发到 External flow、显式声明保持
  通用 device flow。
