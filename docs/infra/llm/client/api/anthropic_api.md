# Anthropic Claude API 文档

## 概述

Anthropic Claude API 提供了高性能、可信赖的 AI 模型访问接口，专精于语言、推理、分析和编程任务。Claude 模型以其安全性和对齐性而闻名。

## 核心 API 端点

### Messages API

**端点：** `POST https://api.anthropic.com/v1/messages`

**描述：** 生成对话式响应，支持多轮对话、工具使用、视觉输入等高级功能。

#### 请求参数

**必需参数：**
- `model` (string) - 要使用的 Claude 模型（如 "claude-sonnet-4-5", "claude-3-haiku"）
- `max_tokens` (integer) - 生成的最大 token 数
- `messages` (array) - 对话消息列表
  - `role` (string) - 消息角色（"user", "assistant"）
  - `content` (string|array) - 消息内容，可以是文本或多模态内容

**可选参数：**
- `system` (string) - 系统提示词
- `temperature` (number) - 随机性控制（0.0-1.0）
- `top_p` (number) - 核采样参数
- `top_k` (integer) - 采样候选数量
- `stop_sequences` (array) - 停止序列
- `stream` (boolean) - 是否启用流式响应
- `tools` (array) - 可用工具定义
- `tool_choice` (object) - 工具选择策略

#### 请求示例

**基础请求：**
```json
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 1024,
  "messages": [
    {
      "role": "user",
      "content": "Hello, Claude!"
    }
  ]
}
```

**带系统提示的请求：**
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
  ]
}
```

**多模态请求（图像 + 文本）：**
```json
{
  "model": "claude-sonnet-4-5",
  "max_tokens": 1024,
  "messages": [
    {
      "role": "user",
      "content": [
        {
          "type": "image",
          "source": {
            "type": "base64",
            "media_type": "image/jpeg",
            "data": "/9j/4AAQSkZJRg..."
          }
        },
        {
          "type": "text",
          "text": "What's in this image?"
        }
      ]
    }
  ]
}
```

**带工具使用的请求：**
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
  ]
}
```

#### 响应结构

**标准响应：**
```json
{
  "id": "msg_01XFDUDYJgAACzvnptvVbrqX",
  "type": "message",
  "role": "assistant",
  "content": [
    {
      "type": "text",
      "text": "Hello! How can I help you today?"
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

**工具使用响应：**
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

**流式响应：**
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

## 认证

**方式：** API Key

**头部：**
```
x-api-key: ANTHROPIC_API_KEY
anthropic-version: 2023-06-01
content-type: application/json
```

## Claude 特有功能

### 1. 工具使用 (Tool Use)

Claude 支持动态工具调用，可以执行外部函数：

**工具定义格式：**
```json
{
  "name": "tool_name",
  "description": "Tool description",
  "input_schema": {
    "type": "object",
    "properties": {
      "param1": {
        "type": "string",
        "description": "Parameter description"
      }
    },
    "required": ["param1"]
  }
}
```

**工具使用流程：**
1. 用户发送包含工具的请求
2. Claude 决定是否使用工具
3. 返回 `tool_use` 类型的响应
4. 执行工具并返回结果
5. Claude 基于工具结果生成最终响应

### 2. 视觉能力 (Vision)

Claude 支持图像分析：

**支持的图像格式：**
- JPEG
- PNG
- GIF
- WebP

**图像输入格式：**
```json
{
  "type": "image",
  "source": {
    "type": "base64",
    "media_type": "image/jpeg",
    "data": "base64_encoded_image_data"
  }
}
```

### 3. 长上下文

Claude 支持扩展的上下文窗口：
- Claude 3.5 Sonnet: 200K tokens
- Claude 3 Opus: 200K tokens
- Claude 3 Haiku: 200K tokens

### 4. 系统提示

系统提示用于设置 AI 的行为和角色：
```json
{
  "system": "You are a helpful assistant with expertise in technical documentation."
}
```

## 基础设施层实现要点

### 1. HTTP 客户端设计

```python
class AnthropicHttpClient(BaseHttpClient):
    def __init__(self, api_key: str):
        super().__init__(
            base_url="https://api.anthropic.com/v1",
            api_key=api_key
        )
        self.headers.update({
            "anthropic-version": "2023-06-01"
        })
    
    async def post(self, endpoint: str, data: dict) -> dict:
        # 处理 Claude 特定的请求格式
        # 处理工具使用和多模态内容
        pass
    
    async def stream_post(self, endpoint: str, data: dict):
        # 实现流式响应处理
        # 处理 Server-Sent Events 格式
        pass
```

### 2. 消息转换器

```python
class AnthropicMessageConverter:
    @staticmethod
    def to_claude_format(messages: List[Dict]) -> List[Dict]:
        # 转换内部消息格式为 Claude 格式
        # 处理多模态内容
        pass
    
    @staticmethod
    def from_claude_response(response: Dict) -> Dict:
        # 转换 Claude 响应为内部格式
        # 处理工具使用结果
        pass
    
    @staticmethod
    def process_tools(tools: List[Dict]) -> List[Dict]:
        # 处理工具定义格式转换
        pass
    
    @staticmethod
    def process_multimodal_content(content: List[Dict]) -> List[Dict]:
        # 处理多模态内容转换
        pass
```

### 3. 工具使用管理器

```python
class ToolUseManager:
    def __init__(self, tools: Dict[str, callable]):
        self.tools = tools
    
    async def execute_tool(self, tool_name: str, tool_input: Dict) -> Dict:
        # 执行工具调用
        pass
    
    def format_tool_result(self, tool_use_id: str, result: Dict) -> Dict:
        # 格式化工具结果
        pass
```

### 4. 配置管理

```python
class AnthropicConfig:
    api_key: str
    base_url: str = "https://api.anthropic.com/v1"
    model: str = "claude-sonnet-4-5"
    max_tokens: int = 1024
    temperature: float = 0.7
    timeout: int = 30
    
    # Claude 特有配置
    system_prompt: Optional[str] = None
    tools: Optional[List[Dict]] = None
    tool_choice: Optional[Dict] = None
    top_p: Optional[float] = None
    top_k: Optional[int] = None
```

## 性能优化

1. **流式处理：** 实时响应处理
2. **工具缓存：** 缓存工具定义和结果
3. **连接池：** 复用 HTTP 连接
4. **批处理：** 批量处理工具调用

## 安全考虑

1. **API 密钥保护：** 安全存储和传输
2. **内容过滤：** Claude 内置内容安全过滤
3. **工具验证：** 验证工具输入和输出
4. **错误脱敏：** 避免泄露敏感信息

## 监控指标

1. **请求延迟：** API 响应时间
2. **工具使用率：** 工具调用频率
3. **多模态处理时间：** 图像处理耗时
4. **Token 使用量：** 输入输出 token 消耗

## 测试策略

1. **单元测试：** 测试各个组件功能
2. **工具测试：** 测试工具使用流程
3. **多模态测试：** 测试图像处理能力
4. **流式测试：** 测试流式响应处理

## 与其他 API 的差异

1. **消息格式：** 使用 `content` 数组而非单一字符串
2. **工具使用：** 内置工具使用支持
3. **系统提示：** 独立的系统提示字段
4. **响应格式：** 结构化的内容块格式

## 错误处理

常见错误码：
- `400` - 请求参数错误
- `401` - 认证失败
- `429` - 请求频率限制
- `500` - 服务器内部错误
- `529` - 服务过载

## 迁移计划

1. **创建 Anthropic HTTP 客户端**
2. **实现消息转换器**
3. **添加工具使用支持**
4. **实现多模态处理**
5. **更新核心层客户端**
6. **添加测试和文档**