# Google Gemini API 参数完整参考

## 概述

Google Gemini API 提供了丰富的参数配置选项，用于控制模型行为、输出格式、多模态处理和推理努力等。Gemini API 支持原生多模态输入和独特的思考配置功能。

## 核心参数

### 必需参数

| 参数名 | 类型 | 描述 | 示例值 |
|--------|------|------|--------|
| `model` | string | 要使用的模型 ID | `"gemini-2.0-flash"`, `"gemini-2.5-flash"`, `"gemini-2.5-pro"` |
| `contents` | array | 对话内容列表 | `[{"parts": [{"text": "Hello"}]}]` |

### 内容格式

```json
{
  "role": "user|model",
  "parts": [
    {
      "text": "文本内容"
    },
    {
      "inline_data": {
        "mime_type": "image/jpeg",
        "data": "base64_encoded_image"
      }
    }
  ]
}
```

## 生成配置参数 (generationConfig)

### 1. 基础控制参数

| 参数名 | 类型 | 默认值 | 范围/选项 | 描述 |
|--------|------|--------|-----------|------|
| `temperature` | number | 0.9 | 0.0 - 2.0 | 采样温度，控制输出随机性 |
| `top_p` | number | 0.95 | 0.0 - 1.0 | 核采样参数，控制多样性 |
| `top_k` | integer | 40 | 1 - 100 | 采样候选数量 |
| `max_output_tokens` | integer | 8192 | 1 - 模型最大值 | 生成的最大 token 数 |
| `candidate_count` | integer | 1 | 1 - 8 | 生成的候选响应数量 |

### 2. 停止和格式控制参数

| 参数名 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `stop_sequences` | array | null | 停止生成的序列列表 |
| `response_mime_type` | string | "text/plain" | 输出响应的 MIME 类型 |

#### response_mime_type 选项

```json
// 纯文本
"text/plain"

// JSON 格式
"application/json"

// 其他格式
"text/html", "application/xml", 等
```

### 3. 惩罚参数

| 参数名 | 类型 | 默认值 | 范围 | 描述 |
|--------|------|--------|------|------|
| `presence_penalty` | number | 0.0 | -2.0 - 2.0 | 存在惩罚，减少重复内容 |
| `frequency_penalty` | number | 0.0 | -2.0 - 2.0 | 频率惩罚，减少频繁出现的 token |

### 4. 确定性参数

| 参数名 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `seed` | integer | null | 确定性采样种子 |

## OpenAI 兼容端点参数

### 基础参数

| 参数名 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `model` | string | - | 模型 ID |
| `messages` | array | - | OpenAI 格式的消息列表 |
| `stream` | boolean | false | 是否启用流式响应 |
| `stream_options` | object | null | 流式响应选项 |

### 高级参数

| 参数名 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `reasoning_effort` | string | "medium" | 推理努力程度 |
| `temperature` | number | 0.7 | 采样温度 |
| `max_tokens` | integer | 2048 | 最大 token 数 |
| `top_p` | number | 0.9 | 核采样参数 |
| `stop` | string/array | null | 停止序列 |

## Gemini 特有参数

### 1. 思考配置 (thinking_config)

通过 `extra_body` 参数传递：

```json
{
  "extra_body": {
    "google": {
      "thinking_config": {
        "thinking_budget": "low|medium|high",
        "include_thoughts": true
      }
    }
  }
}
```

#### thinking_budget 选项

| 值 | 描述 | 相当于 token 数 |
|----|------|----------------|
| "low" | 最小推理努力 | 1,024 |
| "medium" | 中等推理努力 | 8,192 |
| "high" | 高推理努力 | 24,576 |

### 2. 缓存内容

```json
{
  "extra_body": {
    "google": {
      "cached_content": "cachedContents/CONTENT_ID"
    }
  }
}
```

### 3. 系统指令

```json
{
  "system_instruction": {
    "parts": [
      {
        "text": "You are a helpful assistant."
      }
    ]
  }
}
```

## 多模态参数

### 图像输入

```json
{
  "contents": [
    {
      "role": "user",
      "parts": [
        {
          "text": "What's in this image?"
        },
        {
          "inline_data": {
            "mime_type": "image/jpeg",
            "data": "/9j/4AAQSkZJRgABAQAAAQ..."
          }
        }
      ]
    }
  ]
}
```

### 支持的图像格式

| 格式 | MIME 类型 |
|------|-----------|
| JPEG | "image/jpeg" |
| PNG | "image/png" |
| WebP | "image/webp" |
| HEIC | "image/heic" |
| HEIF | "image/heif" |

## 模型特定参数

### Gemini 2.5 系列

| 参数名 | 默认值 | 限制 | 特殊功能 |
|--------|--------|------|----------|
| `max_output_tokens` | 8192 | 1 - 8192 | 支持思考配置 |
| `thinking_budget` | "low" | "low", "medium", "high" | 内置思考能力 |
| `candidate_count` | 1 | 1 - 8 | 多候选响应 |

### Gemini 2.0 系列

| 参数名 | 默认值 | 限制 | 特殊功能 |
|--------|--------|------|----------|
| `max_output_tokens` | 8192 | 1 - 8192 | 更快的响应 |
| `temperature` | 0.9 | 0.0 - 2.0 | 平衡的随机性 |
| `top_k` | 40 | 1 - 100 | 广泛的候选选择 |

### Gemini 1.5 系列

| 参数名 | 默认值 | 限制 | 特殊功能 |
|--------|--------|------|----------|
| `max_output_tokens` | 8192 | 1 - 8192 | 稳定的性能 |
| `context_window` | 1M | - | 大上下文窗口 |

## 请求示例

### 基础请求

```json
{
  "model": "gemini-2.0-flash",
  "contents": [
    {
      "parts": [
        {"text": "Hello, how are you?"}
      ]
    }
  ],
  "generationConfig": {
    "temperature": 0.7,
    "max_output_tokens": 1000,
    "top_p": 0.9,
    "top_k": 40
  }
}
```

### 多模态请求

```json
{
  "model": "gemini-2.0-flash",
  "contents": [
    {
      "role": "user",
      "parts": [
        {
          "text": "What's in this image?"
        },
        {
          "inline_data": {
            "mime_type": "image/jpeg",
            "data": "base64_encoded_image_data"
          }
        }
      ]
    }
  ],
  "generationConfig": {
    "temperature": 0.5,
    "max_output_tokens": 500
  }
}
```

### 带思考配置的请求

```json
{
  "model": "gemini-2.5-flash",
  "contents": [
    {
      "parts": [
        {"text": "Explain quantum computing in detail"}
      ]
    }
  ],
  "generationConfig": {
    "temperature": 0.3,
    "max_output_tokens": 2000
  },
  "extra_body": {
    "google": {
      "thinking_config": {
        "thinking_budget": "high",
        "include_thoughts": true
      }
    }
  }
}
```

### OpenAI 兼容请求

```json
{
  "model": "gemini-2.0-flash",
  "messages": [
    {
      "role": "system",
      "content": "You are a helpful assistant."
    },
    {
      "role": "user",
      "content": "Hello!"
    }
  ],
  "temperature": 0.7,
  "max_tokens": 150,
  "stream": false
}
```

### JSON 输出请求

```json
{
  "model": "gemini-2.0-flash",
  "contents": [
    {
      "parts": [
        {"text": "Generate a user profile in JSON format"}
      ]
    }
  ],
  "generationConfig": {
    "temperature": 0.2,
    "max_output_tokens": 500,
    "response_mime_type": "application/json"
  }
}
```

## 响应格式

### 标准响应

```json
{
  "candidates": [
    {
      "content": {
        "parts": [
          {
            "text": "Response content"
          }
        ],
        "role": "model"
      },
      "finishReason": "STOP",
      "index": 0,
      "safetyRatings": [
        {
          "category": "HARM_CATEGORY_HARASSMENT",
          "probability": "NEGLIGIBLE"
        }
      ]
    }
  ],
  "usageMetadata": {
    "promptTokenCount": 10,
    "candidatesTokenCount": 20,
    "totalTokenCount": 30
  }
}
```

### 带思考过程的响应

```json
{
  "candidates": [
    {
      "content": {
        "parts": [
          {
            "text": "Main response content"
          },
          {
            "thought": "Internal reasoning process"
          }
        ],
        "role": "model"
      },
      "finishReason": "STOP"
    }
  ]
}
```

### 流式响应

```json
{
  "candidates": [
    {
      "content": {
        "parts": [
          {
            "text": "Hello"
          }
        ]
      }
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

## 最佳实践

1. **温度设置**：创意任务使用较高温度（0.8-1.2），事实性任务使用较低温度（0.2-0.5）
2. **思考预算**：复杂推理使用高预算，简单查询使用低预算
3. **多模态处理**：合理压缩图像以提高处理速度
4. **缓存使用**：重复查询使用缓存内容提高效率
5. **JSON 输出**：使用 `response_mime_type` 确保格式一致性

## 限制和注意事项

1. **Token 限制**：不同模型有不同的输入输出 token 限制
2. **图像大小**：图像文件大小限制通常为 10MB
3. **思考功能**：只在特定模型上可用
4. **缓存时效**：缓存内容有有效期限制
5. **安全过滤**：内置内容安全过滤器可能影响输出

## 与 OpenAI API 的差异

| 特性 | OpenAI | Gemini |
|------|--------|--------|
| 消息格式 | `messages` 数组 | `contents` 数组 |
| 多模态支持 | 有限 | 原生支持 |
| 思考配置 | `reasoning_effort` | `thinking_config` |
| 缓存机制 | `store` 参数 | `cached_content` |
| 系统提示 | `system` 消息 | `system_instruction` |
| 输出格式 | `response_format` | `response_mime_type` |
| 候选数量 | `n` 参数 | `candidate_count` |
| 停止序列 | `stop` 参数 | `stop_sequences` |
| 惩罚参数 | `presence_penalty`, `frequency_penalty` | 相同参数名 |
| 确定性 | `seed` 参数 | `seed` 参数 |