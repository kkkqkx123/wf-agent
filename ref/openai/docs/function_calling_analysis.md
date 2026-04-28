# Function Calling 功能分析报告

本文档分析了 OpenAI Python SDK 项目中与 Function Calling（函数调用）相关的功能定义位置。

## 1. 概述

Function Calling 是 OpenAI API 的一项功能，允许模型调用用户定义的函数来获取结果。该 SDK 中 Function Calling 相关的功能分散在多个模块中，主要涉及：

- 类型定义（types）
- 解析库（lib/_parsing）
- 流式处理（lib/streaming）
- 工具辅助（lib/_tools.py）

## 2. 类型定义

### 2.1 Chat Completions 相关类型

| 文件路径 | 类型名称 | 说明 |
|---------|----------|------|
| `types/chat/chat_completion_message_function_tool_call.py` | `Function` | 函数调用参数 |
| `types/chat/chat_completion_message_function_tool_call.py` | `ChatCompletionMessageFunctionToolCall` | 函数工具调用响应 |
| `types/chat/chat_completion_message_function_tool_call_param.py` | `ChatCompletionMessageFunctionToolCallParam` | 函数工具调用参数（请求用） |
| `types/chat/chat_completion_message_tool_call.py` | `ChatCompletionMessageToolCall` | 工具调用联合类型 |
| `types/chat/parsed_function_tool_call.py` | `ParsedFunctionToolCall` | 解析后的函数调用 |
| `types/shared/function_definition.py` | `FunctionDefinition` | 函数定义 |
| `types/shared/function_parameters.py` | `FunctionParameters` | 函数参数模式 |

### 2.2 Responses API 相关类型

| 文件路径 | 类型名称 | 说明 |
|---------|----------|------|
| `types/responses/response_function_tool_call.py` | `ResponseFunctionToolCall` | Responses API 函数调用 |
| `types/responses/response_function_tool_call_param.py` | `ResponseFunctionToolCallParam` | 函数调用参数（请求用） |
| `types/responses/response_function_tool_call_item.py` | `ResponseFunctionToolCallItem` | 函数调用项 |
| `types/responses/response_function_tool_call_output_item.py` | `ResponseFunctionToolCallOutputItem` | 函数调用输出项 |
| `types/responses/function_tool.py` | `FunctionTool` | 函数工具定义 |
| `types/responses/function_tool_param.py` | `FunctionToolParam` | 函数工具参数 |

### 2.3 Beta / Threads 相关类型

| 文件路径 | 类型名称 | 说明 |
|---------|----------|------|
| `types/beta/threads/runs/function_tool_call.py` | `FunctionToolCall` | Threads API 函数调用 |
| `types/beta/threads/runs/function_tool_call_delta.py` | `FunctionToolCallDelta` | 函数调用增量事件 |
| `types/beta/threads/required_action_function_tool_call.py` | `RequiredActionFunctionToolCall` | 待处理的函数调用 |
| `types/beta/function_tool.py` | `FunctionTool` | Beta 函数工具 |
| `types/beta/function_tool_param.py` | `FunctionToolParam` | Beta 函数工具参数 |

### 2.4 实时 API 相关类型

在 `resources/realtime/realtime.py` 中处理实时 API 的函数调用事件。

## 3. 解析库（lib/_parsing）

### 3.1 Chat Completions 解析

**文件**: `lib/_parsing/_completions.py`

关键函数：
- `parse_chat_completion()` - 解析聊天完成响应的函数调用
- `parse_function_tool_arguments()` - 解析函数参数
- `validate_input_tools()` - 验证输入工具

### 3.2 Responses API 解析

**文件**: `lib/_parsing/_responses.py`

关键函数：
- `parse_response()` - 解析 Responses API 响应中的函数调用
- `parse_function_tool_arguments()` - 解析函数参数

## 4. 流式事件处理（lib/streaming）

### 4.1 Chat Completions 流式事件

**文件**: `lib/streaming/chat/_events.py`

| 事件类型 | 说明 |
|----------|------|
| `FunctionToolCallArgumentsDeltaEvent` | 函数参数增量事件 |
| `FunctionToolCallArgumentsDoneEvent` | 函数参数完成事件 |

### 4.2 Responses API 流式事件

**文件**: `lib/streaming/responses/_events.py`

| 事件类型 | 说明 |
|----------|------|
| `ResponseFunctionCallArgumentsDeltaEvent` | 函数参数增量事件 |
| `ResponseFunctionCallArgumentsDoneEvent` | 函数参数完成事件 |

## 5. 工具辅助（lib/_tools.py）

**文件**: `lib/_tools.py`

关键类和函数：
- `PydanticFunctionTool` - Pydantic 模型包装器
- `ResponsesPydanticFunctionTool` - Responses API Pydantic 模型包装器
- `pydantic_function_tool()` - 创建 Pydantic 函数工具的辅助函数

## 6. 资源端点

### 6.1 Chat Completions

**文件**: `resources/chat/completions/completions.py`

- `function_call` 参数已废弃，推荐使用 `tool_choice`
- 支持并行函数调用（parallel function calling）

### 6.2 Responses API

**文件**: `resources/responses/responses.py`

- 支持 function calling 配置

### 6.3 Realtime API

**文件**: `resources/realtime/realtime.py`

- 支持低延迟函数调用

## 7. 核心数据结构关系

```
ChatCompletionMessageFunctionToolCall
├── id: str
├── function: Function
│   ├── arguments: str
│   └── name: str
└── type: "function"

ResponseFunctionToolCall
├── call_id: str
├── arguments: str
├── name: str
├── type: "function_call"
└── status: "in_progress" | "completed" | "incomplete"

FunctionToolCall (Threads API)
├── id: str
├── function: Function
│   ├── arguments: str
│   ├── name: str
│   └── output: Optional[str]
└── type: "function"
```

## 8. 使用示例

### 8.1 定义函数工具

```python
from openai.types.chat import ChatCompletionFunctionToolParam

def get_weather(location: str, unit: str = "celsius") -> str:
    return f"Weather in {location} is 22 {unit}"

tool: ChatCompletionFunctionToolParam = {
    "type": "function",
    "function": {
        "name": "get_weather",
        "description": "Get weather for a location",
        "parameters": {
            "type": "object",
            "properties": {
                "location": {"type": "string"},
                "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]}
            },
            "required": ["location"]
        }
    }
}
```

### 8.2 使用 pydantic_function_tool

```python
from openai.lib import pydantic_function_tool

class WeatherOutput(BaseModel):
    location: str
    temperature: int
    unit: str

tool = pydantic_function_tool(WeatherOutput)
```

## 9. 文件索引汇总

| 模块 | 主要文件 | 用途 |
|------|----------|------|
| types/chat | chat_completion_message_function_tool_call.py | 聊天完成函数调用类型 |
| types/chat | parsed_function_tool_call.py | 解析后的函数调用 |
| types/responses | response_function_tool_call.py | Responses API 函数调用 |
| types/responses | function_tool.py | 函数工具定义 |
| types/beta/threads/runs | function_tool_call.py | Threads API 函数调用 |
| types/shared | function_definition.py | 函数定义 |
| lib/_parsing | _completions.py | 聊天完成响应解析 |
| lib/_parsing | _responses.py | Responses API 响应解析 |
| lib/_tools | _tools.py | 函数工具辅助 |
| lib/streaming | chat/_events.py | 流式函数调用事件 |
| resources/chat/completions | completions.py | 聊天完成 API 端点 |
| resources/responses | responses.py | Responses API 端点 |
| resources/realtime | realtime.py | 实时 API 端点 |

## 10. 相关文档链接

- [Function Calling 官方指南](https://platform.openai.com/docs/guides/function-calling)
- [Structured Outputs](https://platform.openai.com/docs/guides/function-calling#custom-tools)
- [Parallel Function Calling](https://platform.openai.com/docs/guides/function-calling#configuring-parallel-function-calling)