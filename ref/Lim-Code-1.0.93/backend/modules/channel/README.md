# Channel Module - 渠道调用模块

## 概述

渠道调用模块负责执行 LLM API 调用，支持多种 API 格式。

## 核心功能

- ✅ 从配置管理模块获取配置
- ✅ 根据配置类型选择格式转换器
- ✅ 构建 HTTP 请求并执行调用
- ✅ 解析响应并返回标准化数据
- ✅ 支持流式和非流式输出

## 模块结构

```
channel/
├── types.ts                    # 类型定义
├── ChannelManager.ts           # 核心管理器
├── register.ts                 # 模块注册
├── index.ts                    # 统一导出
├── README.md                   # 本文档
└── formatters/                 # 格式转换器
    ├── base.ts                # 基类
    ├── gemini.ts              # Gemini 实现 ✅
    ├── openai.ts              # OpenAI 占位 🔄
    ├── anthropic.ts           # Anthropic 占位 🔄
    └── index.ts               # 注册表
```

## 使用示例

### 流式控制机制

系统支持**两级流式控制**，简洁高效：

1. **配置基础默认**：`config.preferStream`（默认 `false`）
2. **配置级覆盖**：`config.options.stream`（可选）

```typescript
// 方式 1：只设置基础默认值
const configId1 = await configManager.createConfig({
    name: 'Gemini Flash',
    type: 'gemini',
    enabled: true,
    preferStream: false,  // ← 基础默认：非流式
    url: 'https://generativelanguage.googleapis.com/v1beta',
    apiKey: 'YOUR_API_KEY',
    model: 'gemini-2.5-flash',
    options: {
        temperature: 0.7,
        maxOutputTokens: 2048
    }
});

// 方式 2：使用 options.stream 覆盖
const configId2 = await configManager.createConfig({
    name: 'Gemini Flash Stream',
    type: 'gemini',
    enabled: true,
    preferStream: false,  // ← 基础默认：非流式
    url: 'https://generativelanguage.googleapis.com/v1beta',
    apiKey: 'YOUR_API_KEY',
    model: 'gemini-2.5-flash',
    options: {
        temperature: 0.7,
        maxOutputTokens: 2048,
        stream: true  // ← 配置级覆盖：强制流式（优先级更高）
    }
});
```

**为什么是两级？**
- `preferStream`：全局默认值，某些渠道可能只支持流式
- `options.stream`：针对该配置的具体设置，优先级更高

### 非流式生成

```typescript
import { ChannelManager } from 'limcode';

const channelManager = new ChannelManager(configManager);

// 使用配置决定所有生成行为
const response = await channelManager.generate({
    configId: 'gemini-main',
    history: conversationHistory
    // ← 所有生成参数（包括 systemInstruction、stream 等）都由配置决定
});

// 响应包含完整的 Content 格式
console.log(response.content.role);              // 'model'
console.log(response.content.parts);             // ContentPart[]
console.log(response.content.thoughtsTokenCount); // 思考 token 数（如果有）
console.log(response.content.candidatesTokenCount); // 输出 token 数

// 直接存储到 Conversation，无需任何转换
await conversationManager.addMessage({
    conversationId: 'chat-001',
    ...response.content
});
```

### 流式生成

**注意**：流式接口已完成，底层 HTTP 实现待完成。

```typescript
// 创建流式配置（使用 options.stream）
const streamConfigId = await configManager.createConfig({
    name: 'Gemini Flash Stream',
    type: 'gemini',
    enabled: true,
    preferStream: false,
    url: 'https://generativelanguage.googleapis.com/v1beta',
    apiKey: 'YOUR_API_KEY',
    model: 'gemini-2.5-flash',
    options: {
        temperature: 0.7,
        maxOutputTokens: 2048,
        stream: true  // ← 配置级：启用流式
    }
});

// 使用流式配置
const result = await channelManager.generate({
    configId: streamConfigId,
    history: conversationHistory
    // ← 所有参数由配置决定
});

// 自动根据配置返回流式或非流式
for await (const chunk of result as AsyncGenerator<StreamChunk>) {
    // 增量内容
    console.log(chunk.delta);  // ContentPart[]
    
    // 实时显示
    displayIncrementalContent(chunk.delta);
    
    if (chunk.done && chunk.content) {
        // 最后一个块包含完整的 Content
        await conversationManager.addMessage({
            conversationId: 'chat-001',
            ...chunk.content
        });
    }
}
```

**流式选择逻辑**：

```typescript
// 决策逻辑（两级配置）
const useStream = config.options?.stream ?? config.preferStream ?? false;
```

- ✅ **配置完全控制**：所有生成参数都在配置中设置
- ✅ **请求简洁**：只包含对话内容，不包含生成参数
- ✅ **职责分离**：配置层面决定技术细节，调用层面关注业务逻辑

## 支持的格式

| 格式 | 状态 | 描述 |
|------|------|------|
| Gemini | ✅ 完整实现 | 支持所有功能，包括思考签名、多媒体、流式输出 |
| OpenAI | ✅ 完整实现 | 支持 OpenAI 格式（兼容 DeepSeek 等），包括思考内容 |
| Anthropic | 🔄 占位 | 待实现 |

## API 调用流程

```
1. 接收 GenerateRequest
   ↓
2. 从配置管理获取配置
   ↓
3. 选择格式转换器
   ↓
4. 构建 HTTP 请求
   ↓
5. 执行 HTTP 调用
   ↓
6. 解析响应
   ↓
7. 返回 GenerateResponse
```

## 错误处理

所有错误都包装为 `ChannelError`，包含：
- `type`: 错误类型（CONFIG_ERROR, NETWORK_ERROR, API_ERROR 等）
- `message`: 错误描述
- `details`: 详细信息（可选）

```typescript
try {
    const response = await channelManager.generate(request);
} catch (error) {
    if (error instanceof ChannelError) {
        console.error(`${error.type}: ${error.message}`);
        console.error('详情:', error.details);
    }
}
```

## 扩展新格式

1. 创建格式转换器类继承 `BaseFormatter`
2. 实现所有抽象方法
3. 在 `formatters/index.ts` 中注册

```typescript
export class CustomFormatter extends BaseFormatter {
    buildRequest(request, config) { /* ... */ }
    parseResponse(response) { /* ... */ }
    parseStreamChunk(chunk) { /* ... */ }
    validateConfig(config) { /* ... */ }
    getSupportedType() { return 'custom'; }
}

// 注册
formatterRegistry.register(new CustomFormatter());
```

## 相关模块

- **Config Module**: 提供配置管理
- **Conversation Module**: 提供对话历史格式
- **Orchestrator Module**: 协调整体流程（待实现）

## 依赖关系

```
ChannelManager
    ↓ 依赖
ConfigManager (从 config 模块)
    ↓ 提供
ChannelConfig