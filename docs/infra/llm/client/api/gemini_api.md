# Gemini API 文档

## 概述

Google Gemini API 提供了多模态 AI 模型的访问接口，支持文本、图像、音频和视频的处理。Gemini API 提供了 OpenAI 兼容的端点，便于迁移和集成。

## 核心 API 端点

### Chat Completions API (OpenAI 兼容)

**端点：** `POST https://generativelanguage.googleapis.com/v1beta/openai/chat/completions`

**描述：** 使用 Gemini 模型生成对话式响应，提供 OpenAI 兼容的接口。

#### 请求参数

**必需参数：**
- `model` (string) - 要使用的 Gemini 模型（如 "gemini-2.0-flash", "gemini-2.5-flash"）
- `messages` (array) - 对话消息列表
  - `role` (string) - 消息角色（"system", "user", "assistant"）
  - `content` (string) - 消息内容

**可选参数：**
- `stream` (boolean) - 是否启用流式响应
- `stream_options` (object) - 流式选项（如 `{"include_usage": true}`）
- `reasoning_effort` (string) - 推理努力程度（"none", "low", "medium", "high"）
- `extra_body` (object) - Gemini 特定配置
  - `google.thinking_config` (object) - 思考配置
    - `thinking_budget` (string) - 思考预算（"low", "medium", "high"）
    - `include_thoughts` (boolean) - 是否包含思考过程
  - `google.cached_content` (string) - 缓存内容 ID

#### 请求示例

**基础请求：**
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
      "content": "Hello, how are you?"
    }
  ]
}
```

**带思考配置的请求：**
```json
{
  "model": "gemini-2.5-flash",
  "messages": [
    {
      "role": "user",
      "content": "Explain to me how AI works"
    }
  ],
  "extra_body": {
    "google": {
      "thinking_config": {
        "thinking_budget": "low",
        "include_thoughts": true
      }
    }
  }
}
```

**多模态请求（图像 + 文本）：**
```json
{
  "model": "gemini-2.0-flash",
  "messages": [
    {
      "role": "user",
      "content": [
        {
          "type": "text",
          "text": "What is in this image?"
        },
        {
          "type": "image_url",
          "image_url": {
            "url": "data:image/jpeg;base64,BASE64_ENCODED_IMAGE_DATA"
          }
        }
      ]
    }
  ]
}
```

#### 响应结构

**标准响应：**
```json
{
  "id": "chatcmpl-123",
  "object": "chat.completion",
  "created": 1677652288,
  "model": "gemini-2.0-flash",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello! I'm doing well, thank you for asking."
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 9,
    "completion_tokens": 12,
    "total_tokens": 21
  }
}
```

**带思考过程的响应：**
```json
{
  "choices": [
    {
      "message": {
        "role": "assistant",
        "content": "AI works by processing data through algorithms...",
        "thoughts": "The user is asking about how AI works. I should explain..."
      }
    }
  ]
}
```

**流式响应：**
```json
{
  "id": "chatcmpl-...",
  "object": "chat.completion.chunk",
  "created": 1700000000,
  "model": "gemini-2.0-flash",
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

## 认证

**方式：** Bearer Token

**头部：**
```
Authorization: Bearer GEMINI_API_KEY
Content-Type: application/json
```

**基础 URL：** `https://generativelanguage.googleapis.com/v1beta/openai/`

## Gemini 特有功能

### 1. 思考配置 (Thinking Config)

Gemini 提供了独特的思考配置功能：

**思考预算级别：**
- `low` - 最小推理努力（相当于 1,024 tokens）
- `medium` - 中等推理努力（相当于 8,192 tokens）
- `high` - 高推理努力（相当于 24,576 tokens）

**推理努力映射：**
| OpenAI reasoning_effort | Gemini 3 thinking_level | Gemini 2.5 thinking_budget |
|---|---|---|
| minimal | low | 1,024 |
| low | low | 1,024 |
| medium | high | 8,192 |
| high | high | 24,576 |

### 2. 缓存内容

支持使用预缓存的内容来提高响应速度：

```json
{
  "extra_body": {
    "google": {
      "cached_content": "cachedContents/0000aaaa1111bbbb2222cccc3333dddd4444eeee"
    }
  }
}
```

### 3. 多模态支持

支持文本、图像、音频和视频的多模态处理：

**图像输入格式：**
- Base64 编码的图像数据
- 支持多种图像格式（JPEG、PNG、WebP 等）

## 基础设施层实现要点

### 1. HTTP 客户端设计

```python
class GeminiHttpClient(BaseHttpClient):
    def __init__(self, api_key: str):
        super().__init__(
            base_url="https://generativelanguage.googleapis.com/v1beta/openai",
            api_key=api_key
        )
    
    async def post(self, endpoint: str, data: dict) -> dict:
        # 处理 Gemini 特定的 extra_body 参数
        # 处理多模态内容
        pass
    
    async def stream_post(self, endpoint: str, data: dict):
        # 实现流式响应处理
        pass
```

### 2. 消息转换器

```python
class GeminiMessageConverter:
    @staticmethod
    def to_gemini_format(messages: List[Dict]) -> List[Dict]:
        # 转换内部消息格式为 Gemini 格式
        # 处理多模态内容
        pass
    
    @staticmethod
    def from_gemini_response(response: Dict) -> Dict:
        # 转换 Gemini 响应为内部格式
        # 提取思考过程（如果有）
        pass
    
    @staticmethod
    def process_multimodal_content(content: List[Dict]) -> List[Dict]:
        # 处理多模态内容转换
        pass
```

### 3. 配置管理

```python
class GeminiConfig:
    api_key: str
    base_url: str = "https://generativelanguage.googleapis.com/v1beta/openai"
    model: str = "gemini-2.0-flash"
    temperature: float = 0.7
    max_tokens: int = 1000
    timeout: int = 30
    
    # Gemini 特有配置
    thinking_budget: str = "low"
    include_thoughts: bool = False
    cached_content: Optional[str] = None
```

## 性能优化

1. **缓存内容：** 使用 `cached_content` 提高响应速度
2. **思考预算：** 根据任务复杂度调整思考预算
3. **流式处理：** 实时响应处理
4. **连接池：** 复用 HTTP 连接

## 安全考虑

1. **API 密钥保护：** 安全存储和传输
2. **内容过滤：** Gemini 内置内容安全过滤
3. **输入验证：** 验证多模态输入格式
4. **错误脱敏：** 避免泄露敏感信息

## 监控指标

1. **请求延迟：** API 响应时间
2. **思考时间：** 推理过程耗时
3. **缓存命中率：** 缓存内容使用效果
4. **多模态处理时间：** 图像/视频处理耗时

## 测试策略

1. **单元测试：** 测试各个组件功能
2. **多模态测试：** 测试图像、文本组合输入
3. **思考功能测试：** 测试思考配置和过程
4. **性能测试：** 测试缓存和流式性能

## 与 OpenAI 的差异

1. **基础 URL 不同：** Gemini 使用特定的 Google 端点
2. **思考功能：** Gemini 独有的思考配置
3. **缓存机制：** Gemini 支持内容缓存
4. **多模态能力：** Gemini 原生支持多模态

## 迁移计划

1. **创建 Gemini HTTP 客户端**
2. **实现多模态消息转换器**
3. **添加思考配置支持**
4. **实现缓存内容功能**
5. **更新核心层客户端**
6. **添加测试和文档**