# wf-llm 能力文档

本目录描述 infra 层 `wf-llm` 门面 crate 及其底层 `llm-suite/*` 能力。`wf-llm`
本身不承载业务逻辑，只把传输、编解码、消息、token、代理、配置、发现、图像生成、
决策、工具调用协议、内置 provider 目录与认证等能力统一 re-export，使上层只依赖
一个 crate。认证（OAuth）子系统已有独立专题文档，见本目录的 `oauth-*.md` 与
`provider-drivers.md`。

## 分层与依赖

```text
wf-types / wf-common / wf-metrics / wf-plugin-sdk
        ↑
wf-llm-catalog（内置 provider 定义 + 模型元数据）
        ↑
llm-suite/*（传输、编解码、消息、token、代理、配置、发现、图像、决策、工具调用、认证）
        ↑
wf-llm（门面：re-export + 进程内 provider 驱动 + 插件桥接 + 指标适配器）
```

上层（`wf-agent` / `wf-workflow` / `wf-api` / `wf-runtime`）只依赖 `wf-llm` 门面；
`wf-llm` 内部再按能力拆分到 `llm-suite/*` 子 crate，保持 DAG 无环。

## 能力索引

| 文档 | 覆盖内容 |
|---|---|
| `oauth-flows.md` | 通用 OAuth 流程：Device / PKCE loopback / PKCE paste、token 双编码、会话状态机、交互协议、凭据与存储、`ModelAuth` |
| `oauth-presets.md` | 声明式认证配置：`AuthProviderConfig` 字段、7 preset 参数表、展开规则（显式值优先）、校验 |
| `provider-drivers.md` | 进程内专用驱动：`ExternalFlowDriver` 契约、bootstrap 分发、Copilot / Anthropic / Meta 三个驱动、`to_auth` 语义、测试矩阵、第三方扩展 |
| `client-gateway.md` | 传输与网关：`LlmClient` / `LlmClientImpl`、`LlmGateway`、流式与取消、熔断与限流、重试、死循环检测、测试替身 |
| `codec.md` | 编解码与生成参数：`LlmCodec`、四种内置 codec、`CodecRegistry`、生成参数映射/校验、错误类型 |
| `message.md` | 消息构造与变换：消息 builder、helper、历史转换、历史文本化、跨目标 transform |
| `token.md` | Token 估算、计数、定价、用量归一化与记账 |
| `proxy.md` | 代理与 HTTP 客户端构造：支持的 scheme、凭据组合、bypass、传输调优、错误不泄露凭据 |
| `config-discovery.md` | 配置注册表（profile/provider）、请求合并、catalog 元数据、网络模型发现与缓存 |
| `images-decision.md` | 图像生成与决策能力：provider trait、OpenAI 兼容传输、mock；OpenAI / Jev 决策 provider |
| `tool-call.md` | 工具调用协议：partial JSON 恢复、XML/JSON 文本协议解析与渲染、模板 |
| `catalog-builtins.md` | 内置 provider 定义与 id、模型元数据解析器 |
| `plugin-adapters.md` | 插件桥接：auth / codec / images 适配器，以及指标适配器 |

## 模块分组

| 分组 | 关键导出 |
|---|---|
| 传输 | `LlmClient`, `LlmClientImpl`, `FetchHook`, `RetryObserver`, `MessageStream`, `TokenRecordingStream`, `CircuitBreaker`, `RateLimiter`, `DeadLoopDetector` |
| 网关 | `LlmGateway`, `RequestProbe` |
| 编解码 | `LlmCodec`, `CodecRegistry`, `create_codec`, `AnthropicCodec`, `GeminiNativeCodec`, `OpenaiChatCodec`, `OpenaiResponseCodec`, `LlmError`, `LlmResult` |
| 消息 | `message_builder`, `helper`, `history_converter`, `history_text`, `transform` |
| Token | `count`, `estimation`, `pricing`, `usage`, `PricingResolver` |
| 代理 | `compose_proxy_url`, `credential_key`, `has_userinfo`, `reject_userinfo`, `build_proxy`, `build_http_client` |
| 配置/发现 | `ProfileManager`, `ProviderDefinitionRegistry`, `merge_request`, `ModelCatalog`, `filter_models*` |
| 图像 | `ImageGenerationProvider`, `OpenAiCompatibleImagesProvider`, `ImagesContext`, `ImagesOptions`, `ImagesResult` |
| 决策 | `DecisionProvider`, `AnyDecisionProvider`, `DecisionProfileRegistry`, `SharedDecisionProfileRegistry` |
| 工具调用 | `parse_partial_json`, `recover_partial_json`, `parse_from_text`, `render_tool_declaration`, `get_tool_protocol_templates` |
| 内置目录 | `builtin_provider_definitions`, `builtin_provider_ids`, `register_builtin_providers`, `ModelMetadataResolver` |
| 认证 | `AuthRegistry`, `CredentialStore`, `FileCredentialStore`, `ModelAuth`, `OAuthAuth`, `ApiKeyAuth` |
| 插件桥接 | `PluginAuthAdapter`, `PluginCodecAdapter`, `PluginImagesAdapter`, `MetricsSinkAdapter` |

## 关键不变量

- **门面不新增业务逻辑**：`wf-llm` 只 re-export 与少量进程内驱动/桥接，能力实现留在 `llm-suite/*`。
- **profile 是最小调用单位**：`LlmGateway` 强制 `profile_id`，无 fallback 分支；空 id 仅解析为默认 profile。
- **指标是旁路**：所有采集器经 `Option<Arc<...>>` 访问，缺失不阻断执行；每采集器也有 `enabled` 开关。
- **代理凭据不落日志**：错误消息不包含代理 URL 或凭据；持久化路由禁止内嵌 userinfo，凭据只在内存合成地址中出现。
- **显式值优先于 preset**：认证 preset 只填缺失项（`expand_auth_preset`）；profile 显式字段优先于 provider 默认值。
- **通用层不为单个 provider 加一次性开关**：provider 专用协议偏差一律归进程内驱动，不进入 `llm-suite-auth`。
