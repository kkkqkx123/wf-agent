# wf-llm 完善方案

## 设计原则（与 TS 的差异）

Rust 版不应照搬 TS 实现，以下是关键设计调整：

| TS 的设计 | Rust 的调整 | 理由 |
|---|---|---|
| `MessageStream` 事件总线（945 行，10 种事件） | **保留 `Stream<Item=Result<MessageStreamEvent>>` trait，不搞事件总线** | Rust 有原生 `Stream` trait 和 `futures`，组合能力远强于 JS 回调。需要累积消息等特性的上游应自行组合 |
| `LLMClient` retry 内嵌在 HttpClient | **在 `LlmClientImpl` 做 retry loop**，或提取为 `RetryClient` wrapper | 更清晰的责任分离，避免 reqwest 层面的侵入 |
| `AbortSignal` 回调式取消 | **`CancellationToken`**（tokio_util） | Rust 异步取消的惯用方式 |
| `ClientFactory.registerMockClient` | **无需额外 API**，测试时直接构造 mock impl | Rust trait 系统原生支持测试替身 |
| `buildCustomHeaders/applyCustomBody` | **不抽象**，通过 `profile.headers` 直接传给 reqwest | 没有框架能完全泛化，过度抽象得不偿失 |
| TS 的 cost 计算混在 formatter 里 | **formatter 只解析 token 值，cost 由单独模块或调用方计算** | 价格随时间变化，formatter 不应包含定价逻辑 |

## 分阶段实施

### Phase 0 — Bugfix（P0）

| # | 修改 | 文件 | 说明 |
|---|---|---|---|
| 0.1 | 修复 `GeminiOpenaiFormatter::build_request` 消息格式 | `formatters/gemini_openai.rs` | 当前直接将 `request.messages`（Rust struct）序列化，应改为 OpenAI 格式 `{role, content}` |
| 0.2 | `ClientFactory::get_or_create` 每次新建 `reqwest::Client` | `client_factory.rs:38` | 每次创建 profile 都 new 一个 HTTP client，应缓存共享 |

### Phase 1 — 核心能力补全（P0-P1）

| # | 修改 | 文件 | 说明 |
|---|---|---|---|
| 1.1 | Retry 逻辑（指数退避） | `client.rs:generate/generate_stream` | 对 `ProviderError`/`HttpError`（网络/5xx）重试，`max_retries`/`retry_delay` 取自 profile |
| 1.2 | CancellationToken 支持 | `request.rs` + `client.rs` + 各 formatter | `LlmRequest` 加 `cancellation_token`，reqwest 请求绑定 token，流式时检查取消 |
| 1.3 | `LlmError::Timeout`/`AuthError` 实际被 raise | `client.rs` + `error.rs` | 超时直接 case `reqwest::Error` 判断；401 响应转为 `AuthError`；`ProviderError` 按 status 分类 |
| 1.4 | Image 支持（Anthropic + OpenAI Chat） | `formatters/anthropic.rs`, `formatters/openai_chat.rs` | Anthropic 的 `convert_messages` 处理 `MessageContent::ImageUrl`；OpenAI 处理 content array 中的 image_url |
| 1.5 | Gemini Native reasoning/thought 解析 | `formatters/gemini_native.rs` | `parse_response` 和 `parse_stream_chunk` 中提取 thought 内容映射到 `reasoning_content` |
| 1.6 | 提取共享消息转换函数 | 新建 `formatters/shared.rs` | OpenAI Chat / OpenAI Response / Gemini OpenAI 共用同一消息格式，避免代码重复 |

### Phase 2 — 生产化完善（P1）

| # | 修改 | 文件 | 说明 |
|---|---|---|---|
| 2.1 | Token 计数对接 Anthropic API | `client.rs:count_tokens` | 对 Anthropic 调 `/v1/messages/count_tokens`，其余 provider 保留估算 |
| 2.2 | Stream stats 填充 | `client.rs:generate_stream` | 统计 `chunk_count`, `time_to_first_chunk`, `stream_duration`, `total_duration` |
| 2.3 | Cost 追踪 | `client.rs` / 新模块 | token 值 × 价格表 → `prompt_tokens_cost`/`completion_tokens_cost` |
| 2.4 | Auth 类型选择 | `formatters/mod.rs` + helpers | profile 支持 `auth_type: enum {Bearer, XApiKey}`，各 formatter 按类型选认证头 |

### Phase 3 — 扩展功能（P2）

| # | 修改 | 文件 | 说明 |
|---|---|---|---|
| 3.1 | XML 工具调用解析器 | `tool_call_parser.rs` | 与 TS 的 XML tool call parser 对齐，支持 `<tool_use><tool_name>...</parameters>` |
| 3.2 | 文本模式工具调用（JSON wrapped/raw） | `formatters/openai_chat.rs` | 当前仅支持 XML injection，补充 JSON wrapped/raw |
| 3.3 | Anthropic count_tokens endpoint | `client.rs` | 专为 Anthropic 新增 `POST /v1/messages/count_tokens` |
| 3.4 | Reqwest Client 共享 | `client_factory.rs` | 全局共享一个 `reqwest::Client`，按 profile 复用连接池 |

## Rust 设计建议（不照搬 TS）

### 1. MessageStream 保持轻量

```rust
// 当前设计：只产 MessageStreamEvent，不做事件分发。
// 这是正确的。上游如果需要累计消息：
pub async fn accumulate(stream: impl MessageStream) -> LlmResult<LlmResponseType> {
    let mut text = String::new();
    let mut usage = None;
    while let Some(event) = stream.next().await {
        match event? {
            MessageStreamEvent::Text(t) => text.push_str(&t.text),
            MessageStreamEvent::End(_) => break,
            _ => {}
        }
    }
    // 构建 LlmResponseType
}
```

不需要 TS 的 `tee()`、`accumulateMessage()`、10 种事件监听器——这些都是 JS 生态的妥协。

### 2. CancellationToken 优于 AbortSignal

```rust
// LlmRequest 新增字段：
pub cancellation_token: Option<tokio_util::sync::CancellationToken>,

// 在 generate() / generate_stream() 中使用：
if let Some(ref token) = request.cancellation_token {
    let fut = reqwest_client.execute(http_request);
    let result = tokio::select! {
        result = fut => result?,
        _ = token.cancelled() => return Err(LlmError::Cancelled),
    };
}
```

### 3. Retry 逻辑

```rust
// 采用带指数退避的简单 retry 循环：
async fn generate_with_retry(&self, request: &LlmRequest) -> LlmResult<...> {
    let max_retries = self.profile.max_retries.unwrap_or(3);
    let base_delay = self.profile.retry_delay.unwrap_or(1000);
    let mut last_err = None;
    for attempt in 0..=max_retries {
        match self.generate_inner(request).await {
            Ok(result) => return Ok(result),
            Err(e) if e.is_retryable() && attempt < max_retries => {
                last_err = Some(e);
                tokio::time::sleep(Duration::from_millis(base_delay * 2u64.pow(attempt))).await;
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap())
}
```

`is_retryable()` 判断：网络错误、5xx 可重试；4xx（除 429）、配置错误、认证错误不重试。

### 4. Formatter 共享基类模式

TS 用类继承（`BaseFormatter`），Rust 用 **组合 + 共享函数**：

```rust
// formatters/shared.rs — 纯函数集合
pub fn convert_openai_style_messages(messages: &[Message]) -> Vec<serde_json::Value> { ... }
pub fn parse_openai_style_response(json: &Value) -> LlmResponseType { ... }
pub fn parse_openai_style_stream_chunk(data: &str) -> Option<MessageStreamEvent> { ... }
```

三个 OpenAI 兼容 formatter 直接调用这些函数，无需继承关系。
