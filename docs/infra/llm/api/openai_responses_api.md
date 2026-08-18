# OpenAI Responses API (GPT-5) 详细参考

## 概述

OpenAI Responses API 是 GPT-5 系列模型的新一代端点，提供了比传统 Chat Completions API 更先进的架构和功能。Responses API 支持链式思考（Chain of Thought）、改进的推理控制和更高效的参数组织。

## 核心差异

| 特性 | Chat Completions API | Responses API |
|------|-------------------|---------------|
| **输入格式** | `messages` (数组) | `input` (字符串) |
| **推理控制** | `reasoning_effort` (平铺) | `reasoning.effort` (嵌套) |
| **链式思考** | 不支持 | 原生支持 |
| **文本控制** | `temperature`, `top_p` | `text.verbosity` |
| **工具定义** | 平铺结构 | 嵌套结构 |
| **多轮对话** | 通过 `messages` 数组 | 通过 `previous_response_id` |

## 端点信息

- **端点：** `POST https://api.openai.com/v1/responses`
- **支持模型：** `gpt-5.1`, `gpt-5`, `gpt-5-mini`, `gpt-5-nano`
- **认证：** Bearer Token

## 请求参数

### 必需参数

| 参数名 | 类型 | 描述 | 示例值 |
|--------|------|------|--------|
| `model` | string | 要使用的 GPT-5 模型 ID | `"gpt-5.1"` |
| `input` | string | 用户输入或提示 | `"解释量子纠缠的概念"` |

### 可选参数

#### 1. 推理配置 (reasoning)

| 参数名 | 类型 | 默认值 | 选项 | 描述 |
|--------|------|--------|------|------|
| `reasoning.effort` | string | "medium" | `"none"`, `"low"`, `"medium"`, `"high"` | 推理努力程度 |

**推理努力级别说明：**
- `"none"` - 最小推理，最快响应
- `"low"` - 轻度推理，平衡速度和质量
- `"medium"` - 中等推理，标准质量
- `"high"` - 深度推理，最高质量

#### 2. 文本配置 (text)

| 参数名 | 类型 | 默认值 | 选项 | 描述 |
|--------|------|--------|------|------|
| `text.verbosity` | string | "medium" | `"low"`, `"medium"`, `"high"` | 输出文本的详细程度 |

**文本详细程度说明：**
- `"low"` - 简洁回答，关键信息
- `"medium"` - 标准回答，适度详细
- `"high"` - 详细回答，丰富信息

#### 3. 工具配置 (tools)

| 参数名 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `tools` | array | null | 可用工具列表 |

**工具格式：**
```json
{
  "type": "custom",
  "name": "tool_name",
  "description": "工具描述"
}
```

#### 4. 对话连续性

| 参数名 | 类型 | 默认值 | 描述 |
|--------|------|--------|------|
| `previous_response_id` | string | null | 前一个响应的 ID，用于链式思考 |

## 请求示例

### 基础请求

```json
{
  "model": "gpt-5.1",
  "input": "解释量子纠缠的概念"
}
```

### 带推理配置的请求

```json
{
  "model": "gpt-5.1",
  "input": "如何计算覆盖自由女神像1毫米黄金层所需的黄金量？",
  "reasoning": {
    "effort": "high"
  }
}
```

### 带文本控制的请求

```json
{
  "model": "gpt-5.1",
  "input": "生命、宇宙和一切的终极答案是什么？",
  "text": {
    "verbosity": "low"
  }
}
```

### 带工具的请求

```json
{
  "model": "gpt-5.1",
  "input": "使用 code_exec 工具计算半径等于 blueberry 中 r 字母数量的圆的面积",
  "tools": [
    {
      "type": "custom",
      "name": "code_exec",
      "description": "执行任意 Python 代码"
    }
  ]
}
```

### 完整配置请求

```json
{
  "model": "gpt-5.1",
  "input": "分析这个复杂的数学问题并提供详细解答",
  "reasoning": {
    "effort": "high"
  },
  "text": {
    "verbosity": "high"
  },
  "tools": [
    {
      "type": "custom",
      "name": "calculator",
      "description": "执行数学计算"
    },
    {
      "type": "custom",
      "name": "analyzer",
      "description": "分析问题结构"
    }
  ],
  "previous_response_id": "resp_123456789"
}
```

## 响应格式

### 标准响应结构

```json
{
  "id": "resp_abc123def456",
  "object": "response",
  "created": 1678901234,
  "model": "gpt-5.1",
  "choices": [
    {
      "message": {
        "role": "assistant",
        "content": "量子纠缠是量子物理学中的一种现象..."
      }
    }
  ],
  "usage": {
    "prompt_tokens": 15,
    "completion_tokens": 120,
    "total_tokens": 135
  }
}
```

### 带推理过程的响应

```json
{
  "id": "resp_abc123def456",
  "object": "response",
  "created": 1678901234,
  "model": "gpt-5.1",
  "choices": [
    {
      "message": {
        "role": "assistant",
        "content": "经过分析，答案是42"
      }
    }
  ],
  "reasoning": {
    "chain_of_thought": "让我分析这个问题：首先，这是来自《银河系漫游指南》的经典问题...",
    "effort_used": "medium",
    "steps": [
      "识别问题来源",
      "分析上下文",
      "得出结论"
    ]
  },
  "usage": {
    "prompt_tokens": 20,
    "completion_tokens": 80,
    "reasoning_tokens": 45,
    "total_tokens": 145
  }
}
```

### 工具使用响应

```json
{
  "id": "resp_abc123def456",
  "object": "response",
  "created": 1678901234,
  "model": "gpt-5.1",
  "choices": [
    {
      "message": {
        "role": "assistant",
        "content": "我将使用计算工具来解决这个问题",
        "tool_calls": [
          {
            "id": "call_123",
            "type": "function",
            "function": {
              "name": "calculator",
              "arguments": "{\"expression\": \"pi * 9^2\"}"
            }
          }
        ]
      }
    }
  ]
}
```

## 与 Chat Completions API 的对比

### 参数映射

| Responses API | Chat Completions API | 说明 |
|---------------|-------------------|------|
| `input` | `messages` | 字符串 vs 数组 |
| `reasoning.effort` | `reasoning_effort` | 嵌套 vs 平铺 |
| `text.verbosity` | `temperature` | 详细程度 vs 随机性 |
| `tools[].name` | `tools[].function.name` | 简化结构 |
| `previous_response_id` | 无 | 新增功能 |

### 功能对比

| 功能 | Chat Completions | Responses | 优势 |
|------|-----------------|-----------|------|
| **链式思考** | ❌ | ✅ | 更好的推理连续性 |
| **推理控制** | ⚠️ | ✅ | 更精细的控制 |
| **参数组织** | ⚠️ | ✅ | 更清晰的结构 |
| **多轮对话** | ✅ | ⚠️ | 传统方式更成熟 |
| **兼容性** | ✅ | ⚠️ | 更广泛的工具支持 |

## 迁移指南

### 从 Chat Completions 迁移到 Responses

#### 1. 简单对话迁移

**Chat Completions:**
```json
{
  "model": "gpt-5.1",
  "messages": [
    {"role": "user", "content": "你好"}
  ]
}
```

**Responses:**
```json
{
  "model": "gpt-5.1",
  "input": "你好"
}
```

#### 2. 推理控制迁移

**Chat Completions:**
```json
{
  "model": "gpt-5.1",
  "messages": [
    {"role": "user", "content": "解决这个复杂问题"}
  ],
  "reasoning_effort": "high"
}
```

**Responses:**
```json
{
  "model": "gpt-5.1",
  "input": "解决这个复杂问题",
  "reasoning": {
    "effort": "high"
  }
}
```

#### 3. 工具使用迁移

**Chat Completions:**
```json
{
  "model": "gpt-5.1",
  "messages": [
    {"role": "user", "content": "计算圆的面积"}
  ],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "calculator",
        "description": "数学计算工具"
      }
    }
  ]
}
```

**Responses:**
```json
{
  "model": "gpt-5.1",
  "input": "计算圆的面积",
  "tools": [
    {
      "type": "custom",
      "name": "calculator",
      "description": "数学计算工具"
    }
  ]
}
```

## 最佳实践

### 1. 选择合适的推理努力

- **快速响应**：使用 `"none"` 或 `"low"`
- **复杂问题**：使用 `"medium"` 或 `"high"`
- **成本敏感**：优先使用较低的推理努力

### 2. 控制文本详细程度

- **简洁回答**：使用 `"low"`
- **标准回答**：使用 `"medium"`
- **详细解释**：使用 `"high"`

### 3. 利用链式思考

```json
{
  "model": "gpt-5.1",
  "input": "继续分析上一个问题",
  "previous_response_id": "resp_previous",
  "reasoning": {
    "effort": "medium"
  }
}
```

### 4. 工具集成

```json
{
  "model": "gpt-5.1",
  "input": "需要多种工具协作的复杂任务",
  "tools": [
    {
      "type": "custom",
      "name": "web_search",
      "description": "网络搜索"
    },
    {
      "type": "custom", 
      "name": "data_analyzer",
      "description": "数据分析"
    }
  ]
}
```

## 错误处理

### 常见错误码

| 错误码 | 描述 | 解决方案 |
|--------|------|----------|
| 400 | 参数格式错误 | 检查 JSON 结构 |
| 401 | 认证失败 | 检查 API 密钥 |
| 429 | 请求频率限制 | 降低请求频率 |
| 500 | 服务器内部错误 | 重试请求 |

### 特殊错误

**不支持的参数：**
```json
{
  "error": {
    "message": "temperature is not supported for GPT-5 models",
    "type": "invalid_parameter_error"
  }
}
```

**推理努力冲突：**
```json
{
  "error": {
    "message": "reasoning.effort and reasoning_effort cannot be used together",
    "type": "parameter_conflict_error"
  }
}
```

## 性能优化

### 1. 推理努力优化

- **简单查询**：`"none"` - 最快响应
- **中等复杂度**：`"low"` - 平衡性能
- **高复杂度**：`"medium"` 或 `"high"` - 最佳质量

### 2. 文本详细程度优化

- **API 调用**：`"low"` - 减少 token 使用
- **用户交互**：`"medium"` - 标准体验
- **文档生成**：`"high"` - 丰富内容

### 3. 成本控制

```json
{
  "model": "gpt-5.1",
  "input": "简洁查询",
  "reasoning": {
    "effort": "none"
  },
  "text": {
    "verbosity": "low"
  }
}
```

## 限制和注意事项

1. **模型限制**：只支持 GPT-5 系列模型
2. **参数限制**：不支持传统参数如 `temperature`, `top_p`
3. **工具限制**：工具格式与 Chat Completions 不同
4. **对话管理**：多轮对话需要使用 `previous_response_id`
5. **兼容性**：某些功能可能在不同版本中有差异

## 总结

Responses API 代表了 OpenAI API 的演进方向，提供了更直观的参数组织和更强大的推理能力。对于新的 GPT-5 应用，建议优先使用 Responses API 以获得最佳性能和功能支持。

## 多模态支持

### 图像输入格式

Responses API 支持多模态输入，包括文本和图像。图像输入需要使用特定的格式：

```json
{
  "model": "gpt-5.1",
  "input": [
    {
      "role": "user",
      "content": "描述这张图片"
    },
    {
      "role": "user",
      "content": [
        {
          "type": "input_image",
          "image_url": "https://example.com/image.jpg"
        }
      ]
    }
  ]
}
```

### 多模态输入结构

与 Chat Completions API 不同，Responses API 的多模态输入使用以下结构：

| 字段 | 类型 | 描述 |
|------|------|------|
| `type` | string | 内容类型，如 `"input_text"` 或 `"input_image"` |
| `text` | string | 文本内容（当 type 为 `"input_text"` 时） |
| `image_url` | string | 图像 URL（当 type 为 `"input_image"` 时） |

### 支持的图像格式

- **URL 格式**：HTTP/HTTPS URL
- **Base64 格式**：`data:image/jpeg;base64,<base64_data>`
- **支持格式**：JPEG, PNG, GIF, WebP
- **大小限制**：最大 20MB

### 多模态示例

```json
{
  "model": "gpt-5.1",
  "input": [
    {
      "role": "system",
      "content": [
        {
          "type": "input_text",
          "text": "你是一个图像分析专家"
        }
      ]
    },
    {
      "role": "user",
      "content": [
        {
          "type": "input_text",
          "text": "分析这张图片中的主要内容"
        },
        {
          "type": "input_image",
          "image_url": "https://example.com/complex-image.jpg"
        }
      ]
    }
  ],
  "reasoning": {
    "effort": "medium"
  }
}
```

## 流式处理

### 启用流式响应

Responses API 支持服务器发送事件 (SSE) 流式响应：

```python
from openai import OpenAI

client = OpenAI()

stream = client.responses.create(
    model="gpt-5.1",
    input="写一个关于人工智能的短故事",
    stream=True
)

for event in stream:
    if event.delta:
        print(event.delta, end="", flush=True)
```

### 流式事件类型

Responses API 的流式事件包含以下类型：

| 事件类型 | 描述 |
|----------|------|
| `response.created` | 响应创建 |
| `response.output_text.delta` | 文本增量 |
| `response.output_text.done` | 文本完成 |
| `response.tool_call.delta` | 工具调用增量 |
| `response.tool_call.done` | 工具调用完成 |
| `response.done` | 响应完成 |

### 流式事件结构

```json
{
  "type": "response.output_text.delta",
  "delta": "这是流式输出的文本片段"
}
```

### 异步流式处理

```python
import asyncio
from openai import AsyncOpenAI

client = AsyncOpenAI()

async def stream_responses():
    stream = await client.responses.create(
        model="gpt-5.1",
        input="解释量子计算的基本原理",
        stream=True
    )
    
    async for event in stream:
        if event.type == "response.output_text.delta":
            print(event.delta, end="", flush=True)

asyncio.run(stream_responses())
```

### 结构化输出流式处理

```python
from pydantic import BaseModel
from typing import List

class Step(BaseModel):
    explanation: str
    output: str

class MathResponse(BaseModel):
    steps: List[Step]
    final_answer: str

with client.responses.stream(
    input="解决方程式: 2x + 5 = 15",
    model="gpt-5.1",
    text_format=MathResponse,
) as stream:
    for event in stream:
        if event.type == "response.output_text.delta":
            print(event.delta)
    
    # 获取最终结构化响应
    final_response = stream.get_final_response()
    print(f"答案: {final_response.output_text}")
```

## 多模态流式处理

### 图像分析的流式响应

```python
stream = client.responses.create(
    model="gpt-5.1",
    input=[
        {
            "role": "user",
            "content": [
                {
                    "type": "input_text",
                    "text": "详细分析这张图片"
                },
                {
                    "type": "input_image",
                    "image_url": "https://example.com/analysis-image.jpg"
                }
            ]
        }
    ],
    stream=True,
    reasoning={"effort": "high"}
)

for event in stream:
    if event.type == "response.output_text.delta":
        print(event.delta, end="", flush=True)
    elif event.type == "response.reasoning.delta":
        print(f"[推理] {event.delta}")
```

## 高级功能

### 链式多模态对话

```json
{
  "model": "gpt-5.1",
  "input": [
    {
      "role": "user",
      "content": [
        {
          "type": "input_text",
          "text": "基于之前的分析，现在分析这张新图片"
        },
        {
          "type": "input_image",
          "image_url": "https://example.com/new-image.jpg"
        }
      ]
    }
  ],
  "previous_response_id": "resp_abc123",
  "reasoning": {
    "effort": "high"
  }
}
```

### 工具与多模态结合

```json
{
  "model": "gpt-5.1",
  "input": [
    {
      "role": "user",
      "content": [
        {
          "type": "input_text",
          "text": "使用图像分析工具处理这张图片"
        },
        {
          "type": "input_image",
          "image_url": "https://example.com/processing-image.jpg"
        }
      ]
    }
  ],
  "tools": [
    {
      "type": "custom",
      "name": "image_analyzer",
      "description": "深度图像分析工具"
    }
  ],
  "stream": true
}
```

## 性能优化建议

### 多模态优化

1. **图像大小控制**：使用适当的图像分辨率
2. **格式选择**：JPEG 适合照片，PNG 适合图形
3. **缓存策略**：对重复使用的图像进行缓存
4. **预处理**：在发送前进行必要的图像优化

### 流式处理优化

1. **缓冲策略**：合理设置客户端缓冲区大小
2. **错误处理**：实现流式连接的重试机制
3. **超时设置**：根据内容复杂度调整超时时间
4. **并发控制**：限制同时进行的流式请求数量

### 成本控制

```json
{
  "model": "gpt-5.1",
  "input": "简洁的多模态查询",
  "reasoning": {
    "effort": "low"
  },
  "text": {
    "verbosity": "low"
  },
  "stream": true
}
```

## 错误处理增强

### 多模态相关错误

```json
{
  "error": {
    "message": "Image format not supported: webp",
    "type": "invalid_image_format",
    "code": "invalid_image_format"
  }
}
```

```json
{
  "error": {
    "message": "Image size exceeds limit: 25MB > 20MB",
    "type": "image_too_large",
    "code": "image_too_large"
  }
}
```

### 流式处理错误

```json
{
  "error": {
    "message": "Stream connection interrupted",
    "type": "stream_interrupted",
    "code": "stream_interrupted"
  }
}
```

## 最佳实践总结

1. **多模态输入**：使用结构化的 input 数组格式
2. **流式处理**：对长内容启用流式响应
3. **推理控制**：根据任务复杂度选择合适的努力程度
4. **错误处理**：实现完善的错误恢复机制
5. **性能优化**：合理使用缓存和预处理策略
6. **成本控制**：平衡功能需求与资源消耗

Responses API 的多模态和流式处理功能为开发者提供了更强大和灵活的 AI 应用构建能力，特别适合需要实时交互和多媒体处理的场景。