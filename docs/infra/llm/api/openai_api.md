# OpenAI API 文档

## 概述

OpenAI API 提供了多种 AI 模型的访问接口，主要用于自然语言处理任务。本文档重点关注 Chat Completions API，这是我们基础设施层需要实现的核心功能。

## 核心 API 端点

### Chat Completions API

**端点：** `POST /v1/chat/completions`

**描述：** 生成对话式响应，基于消息数组作为输入，支持多个并行生成选择。

#### 请求参数

**必需参数：**
- `model` (string) - 要使用的模型 ID（如 "gpt-4", "gpt-3.5-turbo"）
- `messages` (array) - 对话消息列表
  - `role` (string) - 消息角色（"system", "user", "assistant"）
  - `content` (string) - 消息内容

**可选参数：**
- `n` (integer) - 为每个输入生成的选择数量
- `store` (boolean) - 是否存储聊天完成记录
- `reasoning_effort` (string) - 推理努力程度（如 "none"）
- `stream` (boolean) - 是否启用流式响应
- `stream_options` (object) - 流式选项
- `service_tier` (string) - 处理层级（"auto", "default", "flex", "priority"）
- `seed` (integer) - 确定性响应的种子

#### 请求示例

```json
{
  "model": "gpt-4",
  "messages": [
    {
      "role": "system",
      "content": "You are a helpful assistant."
    },
    {
      "role": "user",
      "content": "Hello, how are you?"
    }
  ],
  "temperature": 0.7,
  "max_tokens": 150
}
```

#### 响应结构

**标准响应：**
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

**流式响应：**
```json
{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-4","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}

{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

{"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-4","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}
```

## 认证

**方式：** Bearer Token

**头部：**
```
Authorization: Bearer YOUR_API_KEY
Content-Type: application/json
```

## 错误处理

常见错误码：
- `400` - 请求参数错误
- `401` - 认证失败
- `429` - 请求频率限制
- `500` - 服务器内部错误

## 基础设施层实现要点

### 1. HTTP 客户端设计

```python
class BaseHttpClient:
    def __init__(self, base_url: str, api_key: str):
        self.base_url = base_url
        self.api_key = api_key
        self.headers = {
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json"
        }
    
    async def post(self, endpoint: str, data: dict) -> dict:
        # 实现 HTTP POST 请求
        pass
    
    async def stream_post(self, endpoint: str, data: dict):
        # 实现流式 HTTP POST 请求
        pass
```

### 2. 消息转换器

```python
class MessageConverter:
    @staticmethod
    def to_openai_format(messages: List[Dict]) -> List[Dict]:
        # 转换内部消息格式为 OpenAI 格式
        pass
    
    @staticmethod
    def from_openai_response(response: Dict) -> Dict:
        # 转换 OpenAI 响应为内部格式
        pass
```

### 3. 配置管理

```python
class OpenAIConfig:
    api_key: str
    base_url: str = "https://api.openai.com/v1"
    model: str = "gpt-3.5-turbo"
    temperature: float = 0.7
    max_tokens: int = 1000
    timeout: int = 30
```

## 性能优化

1. **连接池：** 复用 HTTP 连接
2. **重试机制：** 处理临时错误
3. **缓存：** 缓存常用响应
4. **流式处理：** 实时响应处理

## 安全考虑

1. **API 密钥保护：** 安全存储和传输
2. **输入验证：** 验证请求参数
3. **速率限制：** 遵守 API 限制
4. **错误脱敏：** 避免泄露敏感信息

## 监控指标

1. **请求延迟：** API 响应时间
2. **成功率：** 请求成功比例
3. **Token 使用量：** 消耗的 token 数量
4. **错误率：** 各类错误的发生频率

## 测试策略

1. **单元测试：** 测试各个组件功能
2. **集成测试：** 测试完整 API 调用流程
3. **Mock 测试：** 使用 Mock 数据测试
4. **性能测试：** 测试并发和延迟

## 迁移计划

1. **创建基础设施层 HTTP 客户端**
2. **实现消息转换器**
3. **更新核心层客户端以使用新基础设施**
4. **添加测试和文档**
5. **逐步移除 langchain 依赖**