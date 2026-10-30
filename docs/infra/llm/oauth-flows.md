# 通用 OAuth 流程

本文件说明 `llm-suite-auth`（`crates/infra/llm-suite/crates/llm-suite-auth/`，
独立仓库）提供的通用 RFC 流程与会话模型。注意该 crate 是独立 workspace，
经 git submodule + path 依赖被主 workspace 消费。

## 支持的流程

| 流程 | 会话变体 | 说明 |
|---|---|---|
| Device authorization grant（RFC 8628） | `LoginFlow::Device` | 展示 user code + verification URL，轮询 token endpoint；`authorization_pending` 继续、`slow_down` 加倍间隔、`access_denied` / `expired_token` 失败 |
| PKCE loopback（RFC 7636） | `LoginFlow::PkceLoopback` | 本地回环监听自动接收 callback |
| PKCE paste | `LoginFlow::PkcePaste` | 用户粘贴回调 URL 或裸 code，`state` 一致性校验 |
| API key | `LoginFlow::ApiKey` | 显式粘贴 key，无 provider 交互握手 |
| 外部驱动 | `LoginFlow::External` | 状态与迁移在 crate 之外（进程内专用驱动或插件桥接），见 `provider-drivers.md` |

## Token 请求双编码

`TokenRequestContentType::{Form, Json}` + `token_request` 构造器覆盖三处：
PKCE code 交换、device token 轮询、`DeclaredOAuth::refresh`。两种编码字段集
相同，仅 `Content-Type` 与 body 格式不同；`None` 即 form，保持向后兼容。
device authorization 请求按 RFC 8628 恒为 form，不受该开关影响。

边界：这只解决"同字段集、不同编码"；authorize 额外参数、token 额外字段、
非标准响应映射不在通用层，归进程内驱动。

## 会话状态机

`LoginSession` 为宿主驱动：前端（CLI/TUI/HTTP）通过交互对象推进会话，
auth 层永不阻塞等待前端。外部驱动通过 `ExternalStep` 单步推进：

- `Pending{state, events}`：等待外部事件，事件进入会话日志；
- `AwaitingInput{state, prompt, events}`：宿主采集一轮输入后调 `submit`；
- `Completed{credential}`：产出凭据，会话负责持久化；
- `Failed{error, events}`：失败，已存凭据不受影响。

`state` 为驱动自定的不透明 JSON，会话只存储回放。`poll_interval` 由驱动
建议轮询间隔。

## 交互协议（UI-neutral）

`AuthEvent`：`Info`（文本 + 链接）、`AuthUrl`（待打开的授权 URL）、
`DeviceCode`（user code + verification_uri + 可选间隔/有效期）、
`Progress`（进度说明）。事件会回放给晚接入的前端。

`AuthPrompt`：`Text`、`Secret`、`Select`（多登录方式选择，如 browser /
copy-code）、`ManualCode`（粘贴 out-of-band 拿到的 code/URL）。

## 凭据、存储与解析

凭据为扁平 JSON：`{access, refresh, expires, is_subscription, extra?}`，
宿主解析为 `OAuthCredential` 后以 `Credential::OAuth` 持久化。
`extra` 供驱动存放 refresh 所需的 provider 参数（如动态 client_id）。
存储后端抽象为 `store` trait，文件实现为 `file_store`（路径由宿主指定）；
refresh 走 store 序列化路径，避免并发 double-refresh。

`OAuthAuth::to_auth` 把新鲜凭据转为当次请求的 `ModelAuth{api_key, headers,
base_url, is_subscription, source}`：只有这三者（key/头/base_url）属于认证，
其余都是 provider 配置。`headers` 会并入最终请求头（如 Anthropic 订阅的
`anthropic-beta`）；`is_subscription` 为真时宿主抑制按 token 计费。
