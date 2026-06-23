# OpenAI API 参数完整参考

## 概述

OpenAI 提供了两种主要的 API 端点：

1. **Chat Completions API** - 传统的对话补全 API，支持所有 GPT 模型
2. **Responses API** - 新一代 API（GPT-5 专用），提供链式思考和改进的推理控制

本文档涵盖两种 API 的完整参数参考。

## API 端点对比

| 特性 | Chat Completions API | Responses API |
|------|-------------------|---------------|
| **端点** | `/v1/chat/completions` | `/v1/responses` |
| **支持模型** | 所有 GPT 模型 | 仅 GPT-5 系列 |
| **输入格式** | `messages` (数组) | `input` (字符串) |
| **推理控制** | `reasoning_effort` | `reasoning.effort` |
| **链式思考** | 不支持 | 原生支持 |
| **参数组织** | 平铺结构 | 嵌套结构 |

---

## Chat Completions API 参数

### 必需参数

| 参数名 | 类型 | 描述 | 示例值 |
|--------|------|------|--------|
| `model` | string | 要使用的模型 ID | `"gpt-4"`, `"gpt-3.5-turbo"`, `"gpt-5.1"` |
| `messages` | array | 对话消息列表 | `[{"role": "user", "content": "Hello"}]` |

### 消息格式

```json
{
  "role": "user|assistant|system",
  "content": "消息内容"
}
```

---

## Responses API 参数 (GPT-5 专用)

### 必需参数

| 参数名 | 类型 | 描述 | 示例值 |
|--------|------|------|--------|
| `model` | string | 要使用的 GPT-5 模型 ID | `"gpt-5.1"`, `"gpt-5"` |
| `input` | string | 用户输入或提示 | `"解释量子纠缠的概念"` |

### 推理配置

| 参数名 | 类型 | 默认值 | 选项 | 描述 |
|--------|------|--------|------|------|
| `reasoning.effort` | string | "medium" | `"none"`, `"low"`, `"medium"`, `"high"` | 推理努力程度 |

### 文本配置

| 参数名 | 类型 | 默认值 | 选项 | 描述 |
|--------|------|--------|------|------|
| `text.verbosity` | string | "medium" | `"low"`, `"medium"`, `"high"` | 输出文本详细程度 |

### 对话连续性

| 参数名 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `previous_response_id` | string | null | 前一个响应 ID，用于链式思考 |

### 工具配置

| 参数名 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `tools` | array | null | 可用工具列表 |

**Responses API 工具格式：**
```json
{
  "type": "custom",
  "name": "tool_name",
  "description": "工具描述"
}
```

## 可选参数

### 1. 推理控制参数

| 参数名 | 类型 | 默认值 | 范围/选项 | 描述 |
|--------|------|--------|-----------|------|
| `reasoning_effort` | string | 模型相关 | `"none"`, `"minimal"`, `"low"`, `"medium"`, `"high"` | 限制推理模型的推理努力程度 |
| `temperature` | number | 1.0 | 0.0 - 2.0 | 采样温度，控制输出随机性(部分供应商可能使用的是0-1) |
| `top_p` | number | 1.0 | 0.0 - 1.0 | 核采样参数，控制多样性 |
| `top_logprobs` | integer | null | 0 - 20 | 返回每个位置最可能的 token 数量 |

### 2. 输出控制参数

| 参数名 | 类型 | 默认值 | 范围/选项 | 描述 |
|--------|------|--------|-----------|------|
| `max_tokens` | integer | 模型相关 | 1 - 模型最大值 | 生成的最大 token 数 |
| `stop` | string/array/null | null | 最多4个序列 | 停止生成的序列 |
| `response_format` | object | null | - | 指定输出格式 |

#### response_format 选项

```json
// JSON 模式
{
  "type": "json_object"
}

// 结构化输出
{
  "type": "json_schema",
  "json_schema": {
    "name": "schema_name",
    "schema": {...}
  }
}
```

### 3. 流式和存储参数

| 参数名 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `stream` | boolean | false | 是否启用流式响应 |
| `stream_options` | object | null | 流式响应选项 |
| `store` | boolean | false | 是否存储输出用于模型蒸馏 |

#### stream_options 选项

```json
{
  "include_usage": true
}
```

### 4. 工具使用参数

| 参数名 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `tools` | array | null | 可用工具列表 |
| `tool_choice` | string/object | "auto" | 工具选择策略 |

#### tool_choice 选项

```json
// 不使用工具
"none"

// 自动选择
"auto"

// 必须使用工具
"required"

// 指定工具
{
  "type": "function",
  "function": {
    "name": "my_function"
  }
}
```

#### tools 格式

```json
[
  {
    "type": "function",
    "function": {
      "name": "function_name",
      "description": "函数描述",
      "parameters": {
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
  }
]
```

### 5. 服务和质量参数

| 参数名 | 类型 | 默认值 | 选项 | 描述 |
|--------|------|--------|------|------|
| `service_tier` | string | "auto" | `"auto"`, `"default"`, `"flex"`, `"priority"` | 处理服务层级 |
| `seed` | integer/null | null | - | 确定性采样种子（已弃用） |
| `safety_identifier` | string | null | - | 用户安全标识符 |

### 6. 高级参数

| 参数名 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `n` | integer | 1 | 生成的选择数量 |
| `presence_penalty` | number | 0.0 | -2.0 - 2.0 | 存在惩罚 |
| `frequency_penalty` | number | 0.0 | -2.0 - 2.0 | 频率惩罚 |
| `logit_bias` | object | null | token 偏置 |
| `user` | string | null | 用户标识符 |

## 模型特定参数

### GPT-5 系列模型

| 参数名 | 默认值 | 支持的值 | 特殊说明 |
|--------|--------|----------|----------|
| `reasoning_effort` | "none" (gpt-5.1) | `"none"`, `"low"`, `"medium"`, `"high"` | gpt-5-pro 只支持 "high" |
| `max_tokens` | 模型相关 | 1 - 100000+ | 支持更大的输出长度 |

### GPT-4 系列模型

| 参数名 | 默认值 | 支持的值 | 特殊说明 |
|--------|--------|----------|----------|
| `reasoning_effort` | "medium" | `"minimal"`, `"low"`, `"medium"`, `"high"` | 不支持 "none" |
| `max_tokens` | 模型相关 | 1 - 8192 | 较小的输出限制 |

### GPT-3.5 系列模型

| 参数名 | 默认值 | 支持的值 | 特殊说明 |
|--------|--------|----------|----------|
| `reasoning_effort` | 不支持 | - | 不支持推理努力参数 |
| `max_tokens` | 模型相关 | 1 - 4096 | 最小的输出限制 |

## 请求示例

### 基础请求

```json
{
  "model": "gpt-4",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello!"}
  ],
  "temperature": 0.7,
  "max_tokens": 150
}
```

### 高级配置请求

```json
{
  "model": "gpt-5.1",
  "messages": [
    {"role": "user", "content": "Solve this complex problem"}
  ],
  "temperature": 0.5,
  "max_tokens": 2000,
  "reasoning_effort": "high",
  "response_format": {
    "type": "json_schema",
    "json_schema": {
      "name": "solution",
      "schema": {
        "type": "object",
        "properties": {
          "answer": {"type": "string"},
          "confidence": {"type": "number"}
        }
      }
    }
  },
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "calculate",
        "description": "Perform calculations",
        "parameters": {
          "type": "object",
          "properties": {
            "expression": {"type": "string"}
          }
        }
      }
    }
  ],
  "tool_choice": "auto",
  "stream": false,
  "service_tier": "default"
}
```

### 流式请求

```json
{
  "model": "gpt-4",
  "messages": [
    {"role": "user", "content": "Tell me a story"}
  ],
  "stream": true,
  "stream_options": {
    "include_usage": true
  },
  "temperature": 0.8,
  "max_tokens": 500
}
```

## 响应格式

### 标准响应

```json
{
  "id": "chatcmpl-123",
  "object": "chat.completion",
  "created": 1677652288,
  "model": "gpt-4",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Response content"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 20,
    "total_tokens": 30
  },
  "system_fingerprint": "fp_123",
  "service_tier": "default"
}
```

### 流式响应

```json
{
  "id": "chatcmpl-123",
  "object": "chat.completion.chunk",
  "created": 1677652288,
  "model": "gpt-4",
  "choices": [
    {
      "index": 0,
      "delta": {
        "content": "Hello"
      },
      "finish_reason": null
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

1. **温度设置**：创意任务使用较高温度（0.8-1.0），事实性任务使用较低温度（0.2-0.4）
2. **推理努力**：复杂问题使用高推理努力，简单查询使用低推理努力
3. **输出格式**：使用结构化输出确保数据格式一致性
4. **流式处理**：长文本生成时使用流式响应改善用户体验
5. **工具使用**：合理配置工具选择策略，避免不必要的工具调用

## Chat Completions API 限制和注意事项

1. **Token 限制**：不同模型有不同的输入输出 token 限制
2. **参数兼容性**：某些参数只在特定模型上可用
3. **推理努力**：推理模型不支持 `stop` 参数
4. **存储功能**：图像输入超过 8MB 会被丢弃
5. **确定性**：即使使用相同种子，也不能保证完全确定性的输出

## Responses API 限制和注意事项

1. **模型限制**：只支持 GPT-5 系列模型（`gpt-5.1`, `gpt-5`, `gpt-5-mini`, `gpt-5-nano`）
2. **参数限制**：不支持传统参数如 `temperature`, `top_p`, `logprobs`
3. **工具格式**：工具定义格式与 Chat Completions 不同
4. **对话管理**：多轮对话需要使用 `previous_response_id`
5. **链式思考**：推理过程会在响应中返回，增加 token 使用量

## API 选择建议

### 使用 Chat Completions API 当：

- 需要支持多种模型（包括 GPT-4、GPT-3.5）
- 需要成熟的多轮对话功能
- 需要广泛的工具和生态系统支持
- 需要向后兼容性

### 使用 Responses API 当：

- 专门使用 GPT-5 系列模型
- 需要链式思考和推理连续性
- 需要更精细的推理控制
- 需要更清晰的参数组织结构

## 迁移建议

详细迁移指南请参考：[OpenAI Responses API 专门文档](./openai_responses_api.md)