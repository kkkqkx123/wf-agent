# Gemini 客户端配置指南

本文档介绍如何在 Modular Agent Framework 中配置和使用不同的 Gemini 客户端。

## 概述

框架支持两种 Gemini 客户端实现：

1. **GeminiClient** - 使用 Gemini 原生 API
2. **GeminiOpenAIClient** - 使用 Gemini 的 OpenAI 兼容 API

## 配置方式

### 1. 全局配置

在 `configs/global.toml` 中设置：

```toml
[llm.gemini]
clientType = "native"  # 或 "openai-compatible"
apiKey = "${GEMINI_API_KEY}"
default_model = "gemini-2.5-flash"
```

### 2. 环境变量

在 `.env` 文件中设置：

```bash
AGENT_LLM_GEMINI_CLIENTTYPE=openai-compatible
AGENT_LLM_GEMINI_APIKEY=your_api_key_here
AGENT_LLM_GEMINI_DEFAULTMODEL=gemini-2.5-flash
```

### 3. 代码中使用

```typescript
import { LLMClientFactory } from './infrastructure/external/llm/clients/llm-client-factory';

// 创建客户端工厂
const factory = container.get('LLMClientFactory') as LLMClientFactory;

// 根据配置自动选择客户端类型
const geminiClient = factory.createClient('gemini');

// 或者使用 google 别名
const googleClient = factory.createClient('google');
```

## 客户端差异

### Gemini 原生 API 客户端

- **端点**: `https://generativelanguage.googleapis.com`
- **API 类型**: NATIVE
- **特点**: 
  - 支持 Gemini 特有功能（如思考预算、缓存内容）
  - 使用 Gemini 原生请求格式
  - 更好的功能支持

### Gemini OpenAI 兼容 API 客户端

- **端点**: `https://generativelanguage.googleapis.com/v1beta/openai`
- **API 类型**: OPENAI_COMPATIBLE
- **特点**:
  - 兼容 OpenAI 客户端库
  - 标准化的请求/响应格式
  - 更易于集成现有 OpenAI 代码

## 模型配置

### 支持的模型

- gemini-2.5-pro
- gemini-2.5-flash
- gemini-2.5-flash-lite
- gemini-2.0-flash-exp
- gemini-2.0-flash-thinking-exp
- gemini-1.5-pro
- gemini-1.5-flash
- gemini-1.5-flash-8b

### 模型特定配置

在 `configs/llms/provider/gemini/` 目录下为每个模型创建配置文件：

```toml
# gemini-2.5-flash.toml
[parameters]
temperature = 0.7
max_tokens = 8192
top_p = 0.95

[cost]
prompt_token_price = 0.000125
completion_token_price = 0.000375

[capabilities]
max_tokens = 8192
context_window = 1048576
supports_streaming = true
supports_tools = true
```

## 功能支持

### 共同功能

- 流式响应
- 工具调用
- 图像处理
- 音频处理（部分模型）
- 视频处理（部分模型）

### Gemini 特有功能

- 思考预算 (thinking_budget)
- 缓存内容 (cached_content)
- 推理努力程度 (reasoningEffort)

## 配置示例

### 使用原生 API

```toml
[llm.gemini]
clientType = "native"
apiKey = "${GEMINI_API_KEY}"
default_model = "gemini-2.5-pro"
```

### 使用 OpenAI 兼容 API

```toml
[llm.gemini]
clientType = "openai-compatible"
apiKey = "${GEMINI_API_KEY}"
default_model = "gemini-2.5-flash"
```

## 故障排除

### 常见问题

1. **API 密钥错误**
   - 确保 `GEMINI_API_KEY` 环境变量已设置
   - 检查 API 密钥是否有效

2. **客户端类型不匹配**
   - 确认 `clientType` 配置正确
   - 检查端点 URL 是否匹配

3. **模型不支持**
   - 检查模型名称是否在支持列表中
   - 确认模型是否可用

### 调试

启用调试日志：

```toml
[llm.gemini]
debug = true
log_requests = true
log_responses = true
```

## 迁移指南

### 从 OpenAI 迁移到 Gemini

1. 更新配置文件中的提供商设置
2. 替换模型名称
3. 调整参数（如 `topK` 替代 `topP`）
4. 测试功能兼容性

### 从原生 API 迁移到 OpenAI 兼容 API

1. 更改 `clientType` 为 "openai-compatible"
2. 更新端点 URL
3. 测试请求格式兼容性
4. 验证响应处理

## 最佳实践

1. **使用环境变量**存储敏感信息如 API 密钥
2. **为不同环境**创建不同的配置文件
3. **监控使用量和成本**，设置适当的限制
4. **实现降级策略**，在 API 不可用时切换到备用模型
5. **定期更新模型配置**，以支持新功能和模型

## 参考资料

- [Google Gemini API 文档](https://ai.google.dev/docs)
- [OpenAI API 文档](https://platform.openai.com/docs)
- [Modular Agent Framework 配置指南](../configs/README.md)