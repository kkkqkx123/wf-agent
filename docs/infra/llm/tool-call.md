# 工具调用协议

本篇覆盖 `wf-llm` 暴露的工具调用协议能力：partial JSON 恢复、XML/JSON 文本协议解析与
渲染、协议模板。这些能力供 agent/workflow 在 text-based tool mode 与流式 partial 解析中使用。

## partial_json_parser

- `parse_partial_json(input)`：解析可能不完整的 JSON，返回 `PartialParseResult`。
- `recover_partial_json(input)`：尽力恢复 partial JSON。
- `PartialParseResult`：完整/部分结果枚举。

用途：流式 tool call 参数到达时，增量解析参数；未闭合字符串/对象会被尽力恢复。

## tool::parser

- `has_json_tool_calls` / `has_raw_json_tool_calls` / `has_xml_tool_calls`：检测文本中是否含工具调用。
- `parse_from_text`：从文本解析工具调用。
- `parse_invoke_json_calls` / `parse_invoke_json_calls_detailed`：解析 `<invoke>` JSON 风格调用。
- `parse_json_tool_calls` / `parse_raw_json_tool_calls`：解析 JSON 风格调用。
- `parse_xml_tool_calls`：解析 XML 风格调用。
- `parse_partial`：partial 解析入口。
- `InvokeParseError` / `ParseFormat` / `ToolCallParseOptions`：错误、格式、选项。

## tool::protocol

- `build_text_mode_system_content`：构建 text mode 的系统内容。
- `extract_system_message`：提取系统消息。
- `get_tool_call_parser_options` / `get_tool_protocol_templates` / `get_tool_usage_instructions`。
- `is_text_based_tool_mode`：是否为 text-based tool mode。
- `render_tool_declaration` / `render_tool_list_description`：渲染工具声明/列表描述。
- `requires_prompt_tool_descriptions`：是否需要在 prompt 中附带工具描述。
- `ToolProtocolTemplateSet`：协议模板集合。

## 语义约定

- **特性门控**：`json-partial` 与 `xml` feature 分别控制 partial JSON 与 XML 协议；调用方只编译所需部分。
- **单一实现**：`llm-suite-tool-call` 是唯一规范实现；`wf-llm` re-export，agent 层不重复实现。
- **text mode**：当 provider 不支持 native tool calls 时，工具协议被渲染为文本，由模型输出 XML/JSON 文本，再由 parser 解析。
