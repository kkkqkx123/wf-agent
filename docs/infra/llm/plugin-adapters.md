# 插件桥接与指标适配

本篇覆盖 `wf-llm` 提供的插件桥接适配器与指标适配器：把插件贡献的 auth/codec/images
能力桥接到 host 契约，以及把通用网关 metrics 钩子转发到具体采集器。

## PluginAuthAdapter

- 桥接 `PluginLlmAuth` → host `OAuthAuth`。
- 声明式 auth 参数（params/provider 端点/client id/scopes/flow）原样转发到 `login_start`；
  插件拥有协议，host 拥有会话生命周期。
- 结构化值跨边界为 JSON：auth 参数进，事件与 prompt 出，最终 credential 解析为 host `Credential`。
- `refresh` / `resolve` 为 pass-through（credential 作为 JSON）。
- 构造：`PluginAuthAdapter::new(auth, params)`；`params=None` 变为 JSON null。

## PluginCodecAdapter

- 桥接 `PluginLlmCodec` → host `LlmCodec`。
- request/profile 序列化为 JSON 进入 codec；codec 回答反序列化为 host LLM 类型。
- `build_request` 返回请求描述（`CodecHttpRequest`）；HTTP 请求始终由 host 构造，插件不接触 HTTP 客户端。
- `PluginCodecAdapter::new(codec)`。

## PluginImagesAdapter

- 桥接 `PluginLlmImages` → host `ImageGenerationProvider`。
- context 在构造时绑定（类似 `OpenAiCompatibleImagesProvider`）；每次调用序列化 context + options 跨边界。
- `PluginImagesAdapter::new(images, context)`。

## MetricsSinkAdapter

- `token_stream_adapter::MetricsSinkAdapter`：把通用网关 metrics 钩子转发到
  `TokenMetricsCollector`（wf-metrics）。
- 实现 `TokenUsageSink`（用量记录）与 `LlmMetricsSink`（首字节耗时、流统计）。
- 由门面层 `with_token_metrics` 接线；空时零开销。

## 语义约定

- **插件不接触 HTTP**：网络实现必须在 native backend；Lua/Wasm 沙箱无 HTTP。
- **JSON 边界**：所有结构化值跨边界为 JSON；host 拥有 async 边界与类型安全。
- **插件契约同步**：插件贡献契约为同步；host 负责 async 包装与会话生命周期。
- **官方样板**：唯一官方样板是冻结的 `example-llm-auth-claude-subscription`（只教契约写法）；
  第三方通过 `PluginLlmAuth` / `PluginLlmCodec` / `PluginLlmImages` 扩展。
