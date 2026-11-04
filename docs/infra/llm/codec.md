# 编解码与生成参数

本篇覆盖 `wf-llm` 暴露的 wire 协议层：`LlmCodec` 契约、四种内置 codec、
`CodecRegistry`、生成参数映射与校验、以及 `LlmError`。

## LlmCodec

`llm_codec::LlmCodec` 是 provider wire 协议的抽象，主要方法：

- `build_request(request, profile) -> reqwest::Request`：把 `LlmRequest` + `LlmProfile`
  组装成 HTTP 请求（含 auth、headers、body）。
- `parse_response(body, request) -> LlmResult`：解析响应为 `LlmResult`。
- `convert_tools(tools) -> Vec<Value>`：工具声明转换为 provider 格式。
- `parse_tool_calls(result) -> Vec<LlmToolCall>`：从响应中提取工具调用。
- `count_tokens(request) -> TokenCountResult`（可选）。

插件 codec 通过 `PluginCodecAdapter` 桥接（见 `plugin-adapters.md`）；HTTP 请求始终由
host 构造，插件只描述请求。

## 内置 codec

| Codec | 适用格式 | 说明 |
|---|---|---|
| `OpenaiChatCodec` | `OPENAI_CHAT` | OpenAI Chat Completions |
| `OpenaiResponseCodec` | `OPENAI_RESPONSE` | OpenAI Responses API |
| `AnthropicCodec` | `ANTHROPIC` | Anthropic Messages |
| `GeminiNativeCodec` | `GEMINI_NATIVE` | Gemini native generateContent |

工厂：`create_codec(format)` 根据 `LlmFormat` 创建；`CodecRegistry` 在运行时按 format
字符串注册/查找 codec（支持自定义格式）。`wf-llm` re-export `apply_compat_strict` 等
兼容策略。

## CodecRegistry

- `CodecRegistry::new()`：空注册表（内置 codec 由 gateway 默认注册）。
- `register(name, codec)`：注册自定义 codec；重复注册失败。
- 网关通过 `new_with_codec_registry` 注入自定义注册表；自定义格式须在首次请求前注册。

## 生成参数

`llm_codec::generation` 提供生成参数的映射、解析与校验：

- `apply_openai_chat` / `apply_openai_responses` / `apply_anthropic` /
  `apply_gemini_generation_config`：把通用 `Generation` 映射到 provider 特定 body。
- `apply_compat_strict`：严格兼容模式映射。
- `resolve_generation(profile, request)`：合并 profile 与 request 的生成参数，request 优先。
- `validate_generation`：校验生成参数合法性。
- `parse_legacy_generation`：解析旧格式生成参数。
- `is_typed_param_key`：判断是否为类型化参数键。
- `gemini_tool_config`：Gemini 工具配置构造。
- `ANTHROPIC_DEFAULT_MAX_TOKENS`：Anthropic 默认 max_tokens 常量。

## 错误类型

`llm_codec::LlmError` / `LlmResult` 是全链路统一错误类型。关键变体：

- 传输：`HttpError`, `Timeout`, `Cancelled`, `ProxyError`, `CircuitOpen`
- Provider：`ProviderError{status,message,retry_after_ms}`, `RateLimited{retry_after_ms}`,
  `ContextLengthExceeded`, `AuthError`
- 编解码：`SerializationError`, `InvalidResponse`, `UnsupportedFormat`, `CodecNotFound`
- 配置：`ConfigError`, `ProfileNotFound`, `ToolNotFound`
- 流：`StreamError`

提供 `is_retryable()`、`metric_class()`、`is_context_length_exceeded()` 辅助方法。
重试判定基于结构化状态，不解析消息文本（除上下文长度安全网）。

## 语义约定

- **codec 无状态**：codec 只描述 wire 协议，不持有 HTTP 客户端或 profile。
- **注册表优先**：网关按 format 字符串查 registry；自定义格式未注册则返回 `CodecNotFound`。
- **生成参数合并**：request 级覆盖 profile 级；`resolve_generation` 是唯一合并入口。
