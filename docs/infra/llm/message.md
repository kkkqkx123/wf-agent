# 消息构造与变换

本篇覆盖 `wf-llm` 暴露的消息能力：消息 builder、helper、历史转换、历史文本化、
跨目标 transform。这些模块是纯函数，不依赖 HTTP 或 provider。

## 模块概览

| 模块 | 职责 |
|---|---|
| `message_builder` | 构造 `Message` / 内容块，统一消息装配入口 |
| `helper` | 提取文本内容等便捷函数（`extract_text_content`） |
| `history_converter` | assistant/tool 消息转换、文本模式转换、工具调用/结果渲染 |
| `history_text` | 历史消息纯文本化与计数摘要 |
| `transform` | 跨 provider/目标的消息变换、工具调用 id 截断 |

## history_converter

- `convert_assistant_message`：assistant 消息格式转换。
- `convert_tool_result_message`：tool result 消息转换。
- `convert_to_text_mode`：把结构化消息转为文本模式（用于 text-based tool protocol）。
- `render_tool_calls` / `render_tool_result`：把工具调用/结果渲染为文本。

## history_text

- `to_plain_text`：消息历史转纯文本。
- `summarize_counts`：计数摘要（消息数、token 估算等）。

## transform

- `transform_for_target(target)`：把消息变换为目标 provider 的兼容形态。
- `TransformTarget`：变换目标枚举。
- `MAX_TOOL_CALL_ID_LEN`：工具调用 id 最大长度；超长截断。

## helper

- `extract_text_content(message)`：从消息中提取纯文本内容。

## 语义约定

- **纯函数**：消息模块无副作用、无 I/O，可在任意上下文调用。
- **text mode**：text-based tool protocol 场景下，结构化工具调用被渲染为文本，
  由 `transform` / `history_converter` 协同完成。
- **id 截断**：跨 provider 迁移时工具调用 id 超过 `MAX_TOOL_CALL_ID_LEN` 会被截断，
  调用方需保证唯一性。
