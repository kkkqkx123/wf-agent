# Anthropic Claude Messages API 参数完整参考

## 概述

Anthropic Claude Messages API 提供了丰富的参数配置选项，用于控制模型行为、输出格式、工具使用和多模态处理等。Claude 以其安全性和对齐性而闻名，支持长上下文和高级工具使用功能。

## 核心参数

### 必需参数

| 参数名 | 类型 | 描述 | 示例值 |
|--------|------|------|--------|
| `model` | string | 要使用的模型 ID | `"claude-sonnet-4-5"`, `"claude-3-haiku"`, `"claude-3-opus"` |
| `max_tokens` | integer | 生成的最大 token 数 | 1024, 2048, 4096 |
| `messages` | array | 对话消息列表 | `[{"role": "user", "content": "Hello"}]` |

### 消息格式

```json
{
  "role": "user|assistant",
  "content": "消息内容" | [
    {
      "type": "text",
      "text": "文本内容"
    },
    {
      "type": "image",
      "source": {
        "type": "base64",
        "media_type": "image/jpeg",
        "data": "base64_encoded_image"
      }
    }
  ]
}
```

## 可选参数

### 1. 基础控制参数

| 参数名 | 类型 | 默认值 | 范围/选项 | 描述 |
|--------|------|--------|-----------|------|
| `temperature` | number | 1.0 | 0.0 - 1.0 | 采样温度，控制输出随机性 |
| `top_p` | number | - | 0.0 - 1.0 | 核采样参数，控制多样性 |
| `top_k` | integer | - | 1 - 模型最大值 | 采样候选数量 |
| `stop_sequences` | array | null | 最多4个序列 | 停止生成的序列 |

### 2. 系统和上下文参数

| 参数名 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `system` | string | null | 系统提示词，设置 AI 行为和角色 |
| `metadata` | object | null | 用户元数据，用于标识和追踪 |

### 3. 工具使用参数

| 参数名 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `tools` | array | null | 可用工具列表 |
| `tool_choice` | object/string | "auto" | 工具选择策略 |

#### tool_choice 选项

```json
// 自动选择工具
"auto"

// 不使用工具
"none"

// 必须使用工具
{
  "type": "any"
}

// 指定工具
{
  "type": "tool",
  "name": "tool_name"
}
```

#### tools 格式

```json
[
  {
    "name": "function_name",
    "description": "函数描述",
    "input_schema": {
      "type": "object",
      "properties": {
        "param1": {
          "type": "string",
          "description": "参数描述"
        }
      },
      "required": ["param1"]
    }
  }
]
```

### 4. 流式和响应参数

| 参数名 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `stream` | boolean | false | 是否启用流式响应 |

## 模型特定参数

### Claude 3.5 Sonnet (claude-sonnet-4-5)

| 参数名 | 默认值 | 限制 | 特殊功能 |
|--------|--------|------|----------|
| `max_tokens` | 4096 | 1 - 8192 | 平衡的性能和成本 |
| `context_window` | 200K | - | 大上下文窗口 |
| `temperature` | 1.0 | 0.0 - 1.0 | 精细控制 |
| `tools` | 支持 | - | 高级工具使用 |

### Claude 3 Opus (claude-3-opus)

| 参数名 | 默认值 | 限制 | 特殊功能 |
|--------|--------|------|----------|
| `max_tokens` | 4096 | 1 - 4096 | 最高质量 |
| `context_window` | 200K | - | 大上下文窗口 |
| `temperature` | 1.0 | 0.0 - 1.0 | 高精度输出 |

### Claude 3 Haiku (claude-3-haiku)

| 参数名 | 默认值 | 限制 | 特殊功能 |
|--------|--------|------|----------|
| `max_tokens` | 4096 | 1 - 4096 | 快速响应 |
| `context_window` | 200K | - | 大上下文窗口 |
| `temperature` | 1.0 | 0.0 - 1.0 | 高效处理 |

## 多模态参数

### 图像输入支持

```json
{
  "role": "user",
  "content": [
    {
      "type": "text",
      "text": "What's in this image?"
    },
    {
      "type": "image",
      "source": {
        "type": "base64",
        "media_type": "image/jpeg",
        "data": "/9j/4AAQSkZJRgABAQAAAQ..."
      }
    }
  ]
}
```

### 支持的图像格式

| 格式 | MIME 类型 |
|------|-----------|
| JPEG | "image/jpeg" |
| PNG | "image/png" |
| GIF | "image/gif" |
| WebP | "image/webp" |

### 图像限制

| 限制项 | 限制值 |
|--------|--------|
| 文件大小 | 5MB |
| 分辨率 | 建议 < 1000x1000 |
| 数量 | 每条消息最多 5 张 |

## 请求示例

### 基础请求

```json
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 1024,
  "messages": [
    {
      "role": "user",
      "content": "Hello, Claude!"
    }
  ],
  "temperature": 0.7
}
```

### 带系统提示的请求

```json
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 2048,
  "system": "You are a helpful coding assistant with expertise in Python and JavaScript.",
  "messages": [
    {
      "role": "user",
      "content": "Write a function to merge two sorted arrays"
    }
  ],
  "temperature": 0.3
}
```

### 多模态请求

```json
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 1024,
  "messages": [
    {
      "role": "user",
      "content": [
        {
          "type": "text",
          "text": "What's in this image?"
        },
        {
          "type": "image",
          "source": {
            "type": "base64",
            "media_type": "image/jpeg",
            "data": "base64_encoded_image_data"
          }
        }
      ]
    }
  ]
}
```

### 工具使用请求

```json
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 1024,
  "tools": [
    {
      "name": "get_weather",
      "description": "Get current weather for a location",
      "input_schema": {
        "type": "object",
        "properties": {
          "location": {
            "type": "string",
            "description": "City name"
          }
        },
        "required": ["location"]
      }
    }
  ],
  "messages": [
    {
      "role": "user",
      "content": "What's the weather like in San Francisco?"
    }
  ],
  "tool_choice": "auto"
}
```

### 流式请求

```json
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 1024,
  "messages": [
    {
      "role": "user",
      "content": "Tell me a story"
    }
  ],
  "stream": true,
  "temperature": 0.8
}
```

### 高级配置请求

```json
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 4096,
  "system": "You are a research assistant with expertise in scientific analysis.",
  "messages": [
    {
      "role": "user",
      "content": "Analyze this research paper and summarize the key findings."
    }
  ],
  "temperature": 0.2,
  "top_p": 0.9,
  "top_k": 40,
  "stop_sequences": ["REFERENCES", "BIBLIOGRAPHY"],
  "metadata": {
    "user_id": "user_123",
    "session_id": "session_456"
  }
}
```

## 响应格式

### 标准响应

```json
{
  "id": "msg_01XFDUDYJgAACzvnptvVbrqX",
  "type": "message",
  "role": "assistant",
  "content": [
    {
      "type": "text",
      "text": "Response content"
    }
  ],
  "model": "claude-sonnet-4-5",
  "stop_reason": "end_turn",
  "stop_sequence": null,
  "usage": {
    "input_tokens": 10,
    "output_tokens": 25
  }
}
```

### 工具使用响应

```json
{
  "id": "msg_123",
  "type": "message",
  "role": "assistant",
  "content": [
    {
      "type": "text",
      "text": "I'll check the weather in San Francisco for you."
    },
    {
      "type": "tool_use",
      "id": "toolu_01A09qVqPDqAmAmaFmC5Vx3E",
      "name": "get_weather",
      "input": {
        "location": "San Francisco"
      }
    }
  ],
  "model": "claude-sonnet-4-5",
  "stop_reason": "tool_use",
  "usage": {
    "input_tokens": 15,
    "output_tokens": 20
  }
}
```

### 流式响应

```json
{
  "type": "content_block_start",
  "index": 0,
  "content_block": {
    "type": "text",
    "text": ""
  }
}
{
  "type": "content_block_delta",
  "index": 0,
  "delta": {
    "type": "text_delta",
    "text": "Hello"
  }
}
{
  "type": "content_block_stop",
  "index": 0
}
{
  "type": "message_stop"
}
```

## 工具使用流程

### 1. 工具调用请求

```json
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 1024,
  "tools": [
    {
      "name": "search_web",
      "description": "Search the web for information",
      "input_schema": {
        "type": "object",
        "properties": {
          "query": {"type": "string"}
        },
        "required": ["query"]
      }
    }
  ],
  "messages": [
    {
      "role": "user",
      "content": "Search for latest AI developments"
    }
  ]
}
```

### 2. 工具使用响应

```json
{
  "content": [
    {
      "type": "tool_use",
      "id": "toolu_123",
      "name": "search_web",
      "input": {
        "query": "latest AI developments"
      }
    }
  ],
  "stop_reason": "tool_use"
}
```

### 3. 工具结果回复

```json
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 1024,
  "messages": [
    {
      "role": "user",
      "content": "Search for latest AI developments"
    },
    {
      "role": "assistant",
      "content": [
        {
          "type": "tool_use",
          "id": "toolu_123",
          "name": "search_web",
          "input": {
            "query": "latest AI developments"
          }
        }
      ]
    },
    {
      "role": "user",
      "content": [
        {
          "type": "tool_result",
          "tool_use_id": "toolu_123",
          "content": "Found 5 recent developments in AI..."
        }
      ]
    }
  ]
}
```

## 错误处理

常见错误码和含义：

| 错误码 | 描述 | 解决方案 |
|--------|------|----------|
| 400 | 请求参数错误 | 检查参数格式和值 |
| 401 | 认证失败 | 检查 API 密钥 |
| 429 | 请求频率限制 | 降低请求频率 |
| 500 | 服务器内部错误 | 重试请求 |
| 529 | 服务过载 | 稍后重试 |

## 最佳实践

1. **温度设置**：创意任务使用较高温度（0.8-1.0），事实性任务使用较低温度（0.2-0.4）
2. **系统提示**：清晰定义 AI 角色和行为准则
3. **工具使用**：合理设计工具 schema，提供清晰的描述
4. **多模态处理**：优化图像大小和格式以提高处理速度
5. **上下文管理**：利用大上下文窗口进行复杂对话

## 限制和注意事项

1. **Token 限制**：不同模型有不同的输入输出 token 限制
2. **图像限制**：图像文件大小限制为 5MB
3. **工具限制**：每条消息最多 100 个工具
4. **停止序列**：最多 4 个停止序列
5. **并发限制**：有并发请求限制

## 与其他 API 的差异

| 特性 | OpenAI | Gemini | Anthropic |
|------|--------|--------|-----------|
| 消息格式 | `messages` 数组 | `contents` 数组 | `messages` 数组 |
| 系统提示 | `system` 消息 | `system_instruction` | `system` 参数 |
| 多模态支持 | 有限 | 原生支持 | 原生支持 |
| 工具使用 | `tools` 数组 | `tools` 数组 | `tools` 数组 |
| 流式格式 | Server-Sent Events | Server-Sent Events | Server-Sent Events |
| 停止序列 | `stop` 参数 | `stop_sequences` | `stop_sequences` |
| 温度范围 | 0.0 - 2.0 | 0.0 - 2.0 | 0.0 - 1.0 |
| 上下文窗口 | 模型相关 | 模型相关 | 统一 200K |
| 输出格式 | `response_format` | `response_mime_type` | 无专用参数 |

## 高级功能

### 1. 长上下文处理

Claude 支持高达 200K tokens 的上下文窗口：

```json
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 4096,
  "messages": [
    // 可以包含很长的对话历史
  ]
}
```

### 2. 元数据追踪

使用元数据进行用户识别和追踪：

```json
{
  "metadata": {
    "user_id": "user_123",
    "session_id": "session_456",
    "request_type": "chat"
  }
}
```

### 3. 内容安全

Claude 内置内容安全过滤，无需额外配置。

### 4. 确定性输出

虽然 Claude 没有专门的 `seed` 参数，但可以通过设置 `temperature: 0` 来获得更确定性的输出。