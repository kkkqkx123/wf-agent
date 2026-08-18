# LLM API 参数对比参考

## 概述

本文档对比了 OpenAI、Google Gemini 和 Anthropic Claude 三大 LLM 提供商的 API 参数，帮助开发者理解各平台的差异和共同点，便于实现统一的基础设施层。

## 参数对比表

### 核心参数对比

| 参数名 | OpenAI (Chat) | OpenAI (Responses) | Gemini | Anthropic | 描述 |
|--------|---------------|-------------------|--------|-----------|------|
| **模型标识** | `model` | `model` | `model` | `model` | 指定使用的模型 |
| **输入格式** | `messages` | `input` | `contents` | `messages` | 输入容器 |
| **最大输出** | `max_tokens` | `max_output_tokens` | `max_output_tokens` | `max_tokens` | 生成 token 数限制 |
| **温度** | `temperature` | - | `temperature` | `temperature` | 采样温度 |
| **核采样** | `top_p` | - | `top_p` | `top_p` | 核采样参数 |
| **候选数量** | `n` | - | `candidate_count` | - | 生成候选数量 |
| **停止序列** | `stop` | - | `stop_sequences` | `stop_sequences` | 停止生成序列 |
| **流式响应** | `stream` | `stream` | `stream` | `stream` | 启用流式传输 |

### 独有参数对比

| 功能 | OpenAI (Chat) | OpenAI (Responses) | Gemini | Anthropic |
|------|---------------|-------------------|--------|-----------|
| **推理努力** | `reasoning_effort` | `reasoning.effort` | `thinking_config` | - |
| **系统提示** | `system` 消息 | - | `system_instruction` | `system` 参数 |
| **输出格式** | `response_format` | - | `response_mime_type` | - |
| **文本控制** | `temperature` | `text.verbosity` | - | - |
| **工具使用** | `tools`, `tool_choice` | `tools` | `tools` | `tools`, `tool_choice` |
| **存储功能** | `store` | - | `cached_content` | - |
| **链式思考** | - | `previous_response_id` | - | - |
| **服务层级** | `service_tier` | - | - | - |
| **确定性** | `seed` | - | `seed` | - |
| **安全标识** | `safety_identifier` | - | - | `metadata` |
| **惩罚参数** | `presence_penalty`, `frequency_penalty` | - | `presence_penalty`, `frequency_penalty` | - |

## 详细参数分析

### 1. 消息格式差异

#### OpenAI Chat Completions 格式
```json
{
  "messages": [
    {
      "role": "system|user|assistant",
      "content": "消息内容"
    }
  ]
}
```

#### OpenAI Responses 格式
```json
{
  "input": "用户输入内容"
}
```

#### Gemini 格式
```json
{
  "contents": [
    {
      "role": "user|model",
      "parts": [
        {
          "text": "消息内容"
        }
      ]
    }
  ]
}
```

#### Anthropic 格式
```json
{
  "messages": [
    {
      "role": "user|assistant",
      "content": "消息内容" | [
        {
          "type": "text",
          "text": "文本内容"
        }
      ]
    }
  ]
}
```

### 2. 系统提示差异

| 平台 | 参数名 | 位置 | 示例 |
|------|--------|------|------|
| OpenAI (Chat) | `system` | 消息数组中 | `{"role": "system", "content": "You are helpful"}` |
| OpenAI (Responses) | - | 不支持 | - |
| Gemini | `system_instruction` | 顶级参数 | `{"system_instruction": {"parts": [{"text": "You are helpful"}]}}` |
| Anthropic | `system` | 顶级参数 | `{"system": "You are helpful"}` |

### 3. 温度参数范围

| 平台 | 默认值 | 范围 | 特点 |
|------|--------|------|------|
| OpenAI (Chat) | 1.0 | 0.0 - 2.0 | 范围最广 |
| OpenAI (Responses) | - | 不支持 | 使用 `text.verbosity` 替代 |
| Gemini | 0.9 | 0.0 - 2.0 | 与 OpenAI 相同 |
| Anthropic | 1.0 | 0.0 - 1.0 | 范围较窄 |

### 4. 工具使用对比

#### OpenAI Chat Completions 工具格式
```json
{
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "function_name",
        "description": "描述",
        "parameters": {
          "type": "object",
          "properties": {...},
          "required": [...]
        }
      }
    }
  ],
  "tool_choice": "auto|none|required|specific"
}
```

#### OpenAI Responses 工具格式
```json
{
  "tools": [
    {
      "type": "custom",
      "name": "function_name",
      "description": "描述"
    }
  ]
}
```

#### Gemini 工具格式
```json
{
  "tools": [
    {
      "function_declarations": [
        {
          "name": "function_name",
          "description": "描述",
          "parameters": {
            "type": "object",
            "properties": {...},
            "required": [...]
          }
        }
      ]
    }
  ]
}
```

#### Anthropic 工具格式
```json
{
  "tools": [
    {
      "name": "function_name",
      "description": "描述",
      "input_schema": {
        "type": "object",
        "properties": {...},
        "required": [...]
      }
    }
  ],
  "tool_choice": "auto|none|any|specific"
}
```

### 5. 多模态支持对比

| 特性 | OpenAI (Chat) | OpenAI (Responses) | Gemini | Anthropic |
|------|---------------|-------------------|--------|-----------|
| **图像输入** | 支持 | 支持 | 原生支持 | 原生支持 |
| **图像格式** | Base64 | Base64 | Base64 | Base64 |
| **图像大小** | 20MB | 20MB | 10MB | 5MB |
| **多图像** | 支持 | 支持 | 支持 | 支持（最多5张） |
| **视频输入** | 有限支持 | 有限支持 | 支持 | 有限支持 |
| **音频输入** | 支持 | 支持 | 支持 | 支持 |

### 6. 流式响应对比

#### OpenAI Chat Completions 流式格式
```json
{
  "id": "chatcmpl-123",
  "object": "chat.completion.chunk",
  "choices": [
    {
      "delta": {
        "content": "文本片段"
      }
    }
  ]
}
```

#### OpenAI Responses 流式格式
```json
{
  "id": "resp-123",
  "object": "response.chunk",
  "choices": [
    {
      "delta": {
        "content": "文本片段"
      }
    }
  ]
}
```

#### Gemini 流式格式
```json
{
  "candidates": [
    {
      "content": {
        "parts": [
          {
            "text": "文本片段"
          }
        ]
      }
    }
  ]
}
```

#### Anthropic 流式格式
```json
{
  "type": "content_block_delta",
  "delta": {
    "type": "text_delta",
    "text": "文本片段"
  }
}
```

## 统一抽象设计

### 1. 通用参数映射

```python
class UnifiedLLMConfig:
    # 通用参数
    model: str
    input: Optional[str] = None  # Responses API
    messages: Optional[List[Dict]] = None  # Chat Completions API
    max_tokens: int
    temperature: Optional[float] = 0.7  # 不支持 Responses API
    
    # 可选参数
    top_p: Optional[float] = None
    stream: bool = False
    stop_sequences: Optional[List[str]] = None
    
    # OpenAI 特定
    reasoning_effort: Optional[str] = None  # Chat Completions
    reasoning_config: Optional[Dict[str, str]] = None  # Responses
    text_verbosity: Optional[str] = None  # Responses
    previous_response_id: Optional[str] = None  # Responses
    
    # 平台特定参数
    platform_specific: Dict[str, Any] = {}
```

### 2. 参数转换器

```python
class ParameterConverter:
    @staticmethod
    def to_openai_chat(config: UnifiedLLMConfig) -> Dict:
        result = {
            "model": config.model,
            "messages": config.messages,
            "max_tokens": config.max_tokens,
            "temperature": config.temperature,
        }
        
        if config.top_p:
            result["top_p"] = config.top_p
        if config.stream:
            result["stream"] = config.stream
        if config.stop_sequences:
            result["stop"] = config.stop_sequences
        if config.reasoning_effort:
            result["reasoning_effort"] = config.reasoning_effort
            
        # 添加平台特定参数
        result.update(config.platform_specific.get("openai_chat", {}))
        return result
    
    @staticmethod
    def to_openai_responses(config: UnifiedLLMConfig) -> Dict:
        result = {
            "model": config.model,
            "input": config.input,
            "max_output_tokens": config.max_tokens,
        }
        
        # 推理配置
        if config.reasoning_config:
            result["reasoning"] = config.reasoning_config
        if config.text_verbosity:
            result["text"] = {"verbosity": config.text_verbosity}
        if config.previous_response_id:
            result["previous_response_id"] = config.previous_response_id
        if config.stream:
            result["stream"] = config.stream
            
        # 添加平台特定参数
        result.update(config.platform_specific.get("openai_responses", {}))
        return result
    
    @staticmethod
    def to_gemini(config: UnifiedLLMConfig) -> Dict:
        # 转换消息格式
        contents = []
        for msg in config.messages:
            content = {
                "role": "user" if msg["role"] == "user" else "model",
                "parts": [{"text": msg["content"]}]
            }
            contents.append(content)
        
        result = {
            "model": config.model,
            "contents": contents,
            "generationConfig": {
                "temperature": config.temperature,
                "max_output_tokens": config.max_tokens,
            }
        }
        
        if config.top_p:
            result["generationConfig"]["top_p"] = config.top_p
        if config.stop_sequences:
            result["generationConfig"]["stop_sequences"] = config.stop_sequences
            
        # 添加平台特定参数
        result.update(config.platform_specific.get("gemini", {}))
        return result
    
    @staticmethod
    def to_anthropic(config: UnifiedLLMConfig) -> Dict:
        result = {
            "model": config.model,
            "messages": config.messages,
            "max_tokens": config.max_tokens,
            "temperature": config.temperature,
        }
        
        if config.top_p:
            result["top_p"] = config.top_p
        if config.stream:
            result["stream"] = config.stream
        if config.stop_sequences:
            result["stop_sequences"] = config.stop_sequences
            
        # 添加平台特定参数
        result.update(config.platform_specific.get("anthropic", {}))
        return result
```

### 3. 响应统一格式

```python
class UnifiedLLMResponse:
    id: str
    content: str
    finish_reason: str
    usage: Dict[str, int]
    model: str
    platform: str
    
    # 平台特定字段
    platform_specific: Dict[str, Any] = {}
```
