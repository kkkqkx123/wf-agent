# 传输与网关

本篇覆盖 `wf-llm` 暴露的 HTTP 传输层与网关编排：`LlmClient` / `LlmClientImpl`、
`LlmGateway`、流式与取消、熔断与限流、重试、死循环检测、以及测试替身。传输
层是 leaf：它不感知 profile 注册或指标采集，这些由网关注入。

## LlmClient

`llm_client::LlmClient` 是传输契约，三个方法：

- `generate(request, cancel)`：非流式生成，返回 `LlmResult<LlmResult>`（响应包）。
- `generate_stream(request, cancel)`：流式生成，返回 `Box<dyn MessageStream>`。
- `count_tokens(request, cancel)`：token 计数（部分 provider 支持）。

全部方法接受可选 `CancellationToken`；取消时返回 `LlmError::Cancelled`。

## LlmClientImpl

`llm_client::client::LlmClientImpl` 是默认 HTTP 实现，持有：

- `client: reqwest::Client`（共享/专用客户端）
- `codec: Arc<dyn LlmCodec>`（编解码器）
- `profile: LlmProfile`（端点、超时、重试、生成参数等）
- `retry_observer: Option<RetryObserver>`（重试事件回调）
- `fetch_hook: Option<FetchHook>`（发送前重写请求的函数指针）

构造：`LlmClientImpl::new(client, codec, profile)`；可通过 `with_retry_observer` /
`with_fetch_hook` 附加可选钩子。

### 请求生命周期

1. `codec.build_request(request, profile)` 构造 HTTP 请求。
2. `finalize_request` 合并 per-request `extra_headers`（请求级覆盖 profile/codec 头），
   再执行 `fetch_hook`（最后阶段）。
3. `client.execute`，带超时（`profile.timeout`，默认 60s；请求级 `timeout_ms` 可覆盖）。
4. 非 2xx：映射为 `LlmError`（401/403 → AuthError；408/504 → Timeout；429 → RateLimited
   并解析 `Retry-After`；其余 → ProviderError）；上下文长度超限会被识别为
   `ContextLengthExceeded`。
5. 2xx：`codec.parse_response` 解析响应；填充 `duration`。

重试策略：`RetryPolicy` 默认 `max_retries=3`、`base_delay_ms=1000`、指数退避；
per-request 的 `max_retries` / `retry_delay_ms` 覆盖 profile 默认值。重试事件通过
`RetryObserver` 回调上报。

### 错误映射要点

- 取消：`LlmError::Cancelled`
- 超时：`LlmError::Timeout(ms)`
- 代理失败：`LlmError::ProxyError`（消息不泄露 URL/凭据）
- 熔断打开：`LlmError::CircuitOpen`

## LlmGateway

`llm_gateway::LlmGateway` 是所有 LLM 调用的单一门面，职责：

- 强制 profile 解析（`profile_id` 必填，无 fallback 分支；空 id 解析为默认 profile）；
- 路由到 mock 客户端（测试注入，feature `mock`）或真实客户端；
- 通过 `CodecRegistry` 解析编解码器（内置 + 运行时注册的自定义）；
- 记录 token 用量指标（生成与流式两条路径）；
- 客户端缓存（按 profile/端点键）；
- 熔断器与限流器按端点 base URL 共享；
- 可选定价解析器富化用量成本；
- 可选认证注册表解析存储凭据与 OAuth token；
- 可选请求观测钩子（`RequestProbe`，函数指针，无动态分发）；
- 可选 fetch hook。

构造：

- `LlmGateway::new()`：默认 codec 注册表。
- `LlmGateway::new_with_codec_registry(codecs)`：自定义 codec 注册表（自定义格式须先注册）。

可选附件（builder 风格，返回 `Self`）：

- `with_token_metrics(SharedLlmMetricsSink)`：token 用量采集（空时零开销）。
- `with_pricing_resolver(Arc<dyn PricingResolver>)`：用量成本富化。
- `with_request_probe(RequestProbe)`：请求观测。
- `with_fetch_hook(FetchHook)`：发送前重写请求。

注册表访问：

- `codec_registry()`：注册自定义格式。
- `register_profile` / `remove_profile` / `has_profile` / `profiles`：profile 管理；`remove_profile` 会驱逐该 profile 的缓存客户端。
- `register_provider_definition`：provider 定义（默认值合并到 profile）。
- `register_mock_client`（feature `mock`）：测试注入。

`RequestProbe` 是一组具体函数指针，在分发前后调用；默认为空，热路径仅一次分支检查。

## 流式与取消

- `stream::MessageStream`：流式消息抽象，由客户端返回 `Box<dyn MessageStream>`。
- `token_stream::TokenRecordingStream`：包装一个 `MessageStream`，在流结束时记录 token 用量（供网关 metrics）。
- `token_stream` 模块还定义 `TokenUsageSink` / `LlmMetricsSink` / `Shared*` 共享句柄，网关经此把用量与指标解耦。
- 取消：所有生成/流式方法接受 `Option<CancellationToken>`；取消返回 `LlmError::Cancelled`。

## 熔断与限流

- `circuit::CircuitBreaker` / `CircuitBreakerConfig`：按端点 base URL 共享，打开时请求直接返回 `CircuitOpen`。
- `resilience::RateLimiter` / `Resilience`：限流与弹性策略。

## 死循环检测

`dead_loop_detector::DeadLoopDetector` / `DeadLoopDetectorConfig` / `DeadLoopDetectionResult`：
在 agent 循环中检测重复工具调用/无进展迭代，返回检测结果供上层决策（是否中止）。

## 代理与传输

代理与 HTTP 客户端构造详见 `proxy.md`。传输调优（connect timeout、无效证书 opt-in）通过
`HttpTransportOptions` 传入 `build_http_client_with_options`。

## 测试替身

feature `mock` 启用时：

- `mock::MockLlmClient` / `MockMessageStream`：可编程的 LLM 客户端替身。
- `LlmResponseSpec`：声明式响应规格。
- `http_mock`：HTTP 层 mock 辅助。

`wf-llm` 在 feature `mock` 下 re-export `llm_client::http_mock` 与 `llm_client::mock`，
以及 `MockLlmClient` / `MockMessageStream` / `LlmResponseSpec`。

## 语义约定

- **profile 强制**：网关不接受匿名调用；空 `profile_id` 仅解析为默认 profile，无默认时返回 `ProfileNotFound`。
- **客户端缓存**：按 profile/端点键缓存；`remove_profile` 驱逐对应客户端，不影响其他 profile。
- **指标旁路**：`token_metrics` 为 `Option`；空时零开销；每采集器也有 `enabled` 开关。
- **重试覆盖**：请求级 `max_retries` / `retry_delay_ms` 优先于 profile 默认值。
- **上下文长度安全网**：provider 返回上下文超限时识别为 `ContextLengthExceeded`，供上层触发压缩。
