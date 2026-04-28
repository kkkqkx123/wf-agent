# Token 统计机制分析文档

## 概述

本文档详细分析了当前项目的 Token 统计实现，并与 Lim Code 和参考实现进行对比，识别存在的问题和改进方向。

**分析日期**: 2026-03-09  
**涉及模块**: sdk/core/llm, sdk/graph/execution, packages/types

---

## 1. 当前项目架构

### 1.1 核心组件

```
┌─────────────────────────────────────────────────────────────┐
│                    Formatter Layer                           │
│  ┌──────────────┬──────────────┬──────────────┐            │
│  │ OpenAIChat   │ GeminiNative │  Anthropic   │ 各供应商适配 │
│  │ Formatter    │ Formatter    │  Formatter   │             │
│  └──────┬───────┴──────┬───────┴──────┬───────┘            │
└─────────┼──────────────┼──────────────┼────────────────────┘
          │              │              │
          └──────────────┼──────────────┘
                         │
          ┌──────────────▼──────────────┐
          │      BaseLLMClient          │
          │  - generate()               │
          │  - generateStream()         │
          └──────────────┬──────────────┘
                         │
          ┌──────────────▼──────────────┐
          │       LLMWrapper            │
          │  - 统一调用接口              │
          │  - MessageStream 集成        │
          └──────────────┬──────────────┘
                         │
          ┌──────────────▼──────────────┐
          │      LLMExecutor            │
          │  - 无状态执行器              │
          └──────────────┬──────────────┘
                         │
          ┌──────────────▼──────────────┐
          │   LLMExecutionCoordinator   │
          │  - Token统计协调             │
          └──────────────┬──────────────┘
                         │
          ┌──────────────▼──────────────┐
          │   ConversationManager       │
          │  - 消息管理                  │
          │  - Token累积                 │
          └──────────────┬──────────────┘
                         │
          ┌──────────────▼──────────────┐
          │    TokenUsageTracker        │
          │  - 核心统计追踪              │
          │  - 历史记录                  │
          │  - 回退支持                  │
          └─────────────────────────────┘
```

### 1.2 数据流

```
API Response
    │
    ▼
┌─────────────────────────────────────┐
│ Formatter.parseResponse()           │
│ - 提取 usage 数据                    │
│ - 构建 LLMResult                     │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│ BaseLLMClient.generateStream()      │
│ - 累积 usage chunks                  │
│ - 传递给 MessageStream               │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│ LLMExecutionCoordinator             │
│ - conversationState.updateTokenUsage()│
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│ TokenUsageTracker                   │
│ - updateApiUsage()                   │
│ - accumulateStreamUsage()            │
│ - finalizeCurrentRequest()           │
└─────────────────────────────────────┘
```

---

## 2. 与 Lim Code 对比

### 2.1 架构差异

| 方面 | 当前项目 | Lim Code | 评价 |
|------|----------|----------|------|
| **消息格式** | LLMMessage (通用) | Content (Gemini原生) | Lim Code更贴近API |
| **Token存储** | TokenUsageTracker (内存) | 消息.usageMetadata | Lim Code支持消息级追溯 |
| **统计分离** | ❌ 不分离 | ✅ thoughts/candidates分离 | Lim Code支持思考模型分析 |
| **供应商适配** | Formatter策略 | ChannelAdapter策略 | 两者类似 |

### 2.2 核心数据结构对比

**当前项目 - LLMUsage**:
```typescript
export interface LLMUsage {
  promptTokens: number;      // 输入token
  completionTokens: number;  // 完成token（包含reasoning）
  totalTokens: number;       // 总数
  // 可选字段...
}
```

**Lim Code - UsageMetadata**:
```typescript
export interface UsageMetadata {
  promptTokenCount?: number;      // 输入token
  candidatesTokenCount?: number;  // 输出token（不含thinking）
  thoughtsTokenCount?: number;    // thinking token
  totalTokenCount?: number;       // 总数
}
```

### 2.3 关键差距

#### 差距 1: 思考内容统计分离

**Lim Code 做法**:
```typescript
// OpenAI Formatter
const completionTokens = usage.completion_tokens || 0;
const reasoningTokens = usage.completion_tokens_details?.reasoning_tokens || 0;
const candidatesTokenCount = completionTokens - reasoningTokens;

content.usageMetadata = {
  promptTokenCount: usage.prompt_tokens,
  candidatesTokenCount: candidatesTokenCount > 0 ? candidatesTokenCount : undefined,
  thoughtsTokenCount: reasoningTokens > 0 ? reasoningTokens : undefined
};
```

**当前项目问题**:
```typescript
// OpenAI Formatter 提取了reasoningTokens但未使用
const reasoningTokens = data.usage?.completion_tokens_details?.reasoning_tokens;

// 但LLMResult中没有对应字段存储
usage: data.usage ? {
  promptTokens: data.usage.prompt_tokens,
  completionTokens: data.usage.completion_tokens,  // ❌ 包含reasoning，未分离
  totalTokens: data.usage.total_tokens
} : undefined,
reasoningTokens  // ← 返回了，但类型未定义此字段
```

#### 差距 2: 消息级Token存储

**Lim Code**: 每个assistant消息都携带`usageMetadata`
```typescript
interface Content {
  role: 'user' | 'model';
  parts: ContentPart[];
  usageMetadata?: UsageMetadata;  // ← 消息级统计
}
```

**当前项目**: Token统计只在`TokenUsageTracker`中累积
```typescript
interface LLMMessage {
  role: MessageRole;
  content: MessageContent;
  // ❌ 没有usageMetadata字段
}
```

#### 差距 3: Gemini thoughtsTokenCount 处理

**Lim Code**:
```typescript
// Gemini Formatter 保留thoughtsTokenCount
content.usageMetadata = {
  promptTokenCount: response.usageMetadata.promptTokenCount,
  candidatesTokenCount: response.usageMetadata.candidatesTokenCount,
  thoughtsTokenCount: response.usageMetadata.thoughtsTokenCount  // ← 保留
};
```

**当前项目**:
```typescript
// Gemini Formatter 忽略了thoughtsTokenCount
usage: data.usageMetadata ? {
  promptTokens: data.usageMetadata.promptTokenCount || 0,
  completionTokens: data.usageMetadata.candidatesTokenCount || 0,
  totalTokens: data.usageMetadata.totalTokenCount || 0
  // ❌ 缺少 thoughtsTokenCount
} : undefined
```

---

## 3. 与参考实现对比

### 3.1 架构对比

| 组件 | 参考实现 | 当前项目 | 差距 |
|------|----------|----------|------|
| **流式Token管理** | `StreamingTokenManager` | `TokenUsageTracker` | 功能类似，参考实现更完善 |
| **输入Token估算** | `InputTokenEstimator` | 简单`estimateTokens` | ❌ 当前缺少详细估算 |
| **Fallback机制** | 完整tiktoken回退 | 简单估算 | ❌ 当前fallback不完善 |
| **事件驱动** | `EventBus` 发布/订阅 | 直接函数调用 | 架构风格不同 |
| **持久化** | 存储在消息中 | 内存存储 | 当前中断后丢失 |

### 3.2 流式统计对比

#### 参考实现 - 累积逻辑
```typescript
// StreamingTokenManager.ts
addApiUsage(inputTokens, outputTokens, cost) {
  this.tokens.input += inputTokens;
  this.tokens.output += outputTokens;
  this.tokens.totalCost += cost;
}
```

#### 当前项目 - 覆盖逻辑
```typescript
// base-client.ts
if (result.chunk.usage) {
  accumulatedUsage = { ...result.chunk.usage };  // ❌ 覆盖而非累积
}
```

### 3.3 Fallback机制对比

#### 参考实现 - 分级Fallback
```typescript
// 情况1: 完整回退 - API完全未返回token数据
const needsFullFallback = !this.hasApiUsageData &&
  this.tokens.input === 0 &&
  this.tokens.output === 0;

// 情况2: 部分回退 - 有output但缺少input
const needsPartialFallback = this.tokens.output > 0 &&
  this.tokens.input === 0 &&
  !this.receivedMessageStartUsage;
```

#### 当前项目 - 简单Fallback
```typescript
// token-utils.ts
export function getTokenUsage(usage: LLMUsage | null, messages: LLMMessage[]): number {
  // 优先使用 API 统计
  if (usage) {
    return usage.totalTokens;
  }
  // 使用本地估算
  return estimateTokens(messages);
}
```

---

## 4. 问题总结

### 4.1 高优先级问题

#### 问题 1: 流式统计累积逻辑错误

**文件**: `sdk/core/llm/base-client.ts:108-120`

**问题描述**:
```typescript
async *generateStream(request: LLMRequest): AsyncIterable<LLMResult> {
  let accumulatedUsage = null;
  
  for await (const line of stream) {
    const result = this.formatter.parseStreamLine(line, config);
    
    if (result.valid && result.chunk) {
      if (result.chunk.usage) {
        accumulatedUsage = { ...result.chunk.usage };  // ❌ 直接覆盖
      }
      // ...
    }
  }
}
```

**影响**: 流式响应中，每个chunk的usage直接覆盖前一个，导致统计错误。

**建议修复**:
```typescript
if (result.chunk.usage) {
  accumulatedUsage = {
    promptTokens: result.chunk.usage.promptTokens ?? accumulatedUsage?.promptTokens ?? 0,
    completionTokens: result.chunk.usage.completionTokens ?? accumulatedUsage?.completionTokens ?? 0,
    totalTokens: result.chunk.usage.totalTokens ?? accumulatedUsage?.totalTokens ?? 0
  };
}
```

### 4.2 中优先级问题

#### 问题 2: 思考模型Token统计不完整

**影响模型**: OpenAI o1系列、DeepSeek R1、Gemini Thinking

**问题描述**: 
- OpenAI Formatter提取了`reasoningTokens`但未在类型中定义
- Gemini Formatter忽略了`thoughtsTokenCount`
- 无法区分thinking token和output token

### 4.3 低优先级问题

#### 问题 4: 缺少消息级Token存储

**影响**: 无法按消息追溯Token消耗，中断后统计丢失

**问题描述**:
- Token统计只在`TokenUsageTracker`内存中
- 没有与消息系统关联

#### 问题 5: 输入Token估算不完善

**影响**: API不返回usage时的fallback估算不够精确

**问题描述**:
- 只有简单的`estimateTokens(messages)`函数
- 没有区分system/history/tools的详细估算

---

## 5. 改进建议

### 5.1 最小改动方案

1. **修复流式统计累积逻辑**（必须）
   - 修改`base-client.ts`的累积逻辑
   - 累积而非覆盖

### 5.2 完整改进方案

如果需要完善的Token统计，建议按以下顺序实施：

**Phase 1: 核心修复**
1. 修复流式统计累积逻辑
2. 扩展LLMUsage类型（reasoningTokens）
3. 修改各Formatter正确提取和分离token

**Phase 2: 消息级统计**
1. LLMMessage添加usageMetadata字段
2. 修改消息构建逻辑存储usage
3. 支持按消息追溯token消耗

**Phase 3: Fallback完善**
1. 添加`InputTokenEstimator`类
2. 实现分级fallback机制
3. 集成StreamingTokenCounter作为fallback

---

## 6. 结论

### 当前状态评估

| 功能 | 状态 | 说明 |
|------|------|------|
| 基础Token统计 | ✅ 可用 | 基本功能正常 |
| 流式统计 | ⚠️ 有问题 | 累积逻辑有缺陷 |
| 思考模型统计 | ❌ 不支持 | 未分离thinking/output |
| 消息级存储 | ❌ 不支持 | 内存存储，中断丢失 |
| Fallback估算 | ⚠️ 简单 | 基础估算可用 |


---

## 附录

### A. 相关文件清单

**核心类型定义**:
- `packages/types/src/llm/usage.ts` - LLMUsage, TokenUsageStats
- `packages/types/src/message/message.ts` - LLMMessage
- `packages/types/src/llm/llm-result.ts` - LLMResult

**Formatter实现**:
- `sdk/core/llm/formatters/openai-chat.ts` - OpenAI Chat Formatter
- `sdk/core/llm/formatters/gemini-native.ts` - Gemini Native Formatter
- `sdk/core/llm/formatters/anthropic.ts` - Anthropic Formatter
- `sdk/core/llm/formatters/base.ts` - BaseFormatter

**Token统计**:
- `sdk/graph/execution/token-usage-tracker.ts` - TokenUsageTracker
- `sdk/graph/execution/utils/token-utils.ts` - Token工具函数
- `sdk/utils/token-encoder.ts` - TokenEncoder, StreamingTokenCounter

**客户端**:
- `sdk/core/llm/base-client.ts` - BaseLLMClient
- `sdk/core/llm/wrapper.ts` - LLMWrapper
- `sdk/graph/execution/executors/llm-executor.ts` - LLMExecutor

**协调器**:
- `sdk/graph/execution/coordinators/llm-execution-coordinator.ts` - 协调Token统计
- `sdk/graph/execution/managers/conversation-manager.ts` - 对话管理

### B. 参考实现文件

**Lim Code**:
- `ref/Lim-Code-1.0.93/backend/modules/conversation/tokenUtils.ts`
- `ref/Lim-Code-1.0.93/backend/modules/conversation/types.ts`
- `ref/Lim-Code-1.0.93/backend/modules/channel/formatters/openai.ts`
- `ref/Lim-Code-1.0.93/backend/modules/channel/formatters/gemini.ts`

### C. 术语对照

| 当前项目 | Lim Code | 参考实现 | 说明 |
|----------|----------|----------|------|
| promptTokens | promptTokenCount | tokensIn | 输入token |
| completionTokens | candidatesTokenCount | tokensOut | 输出token |
| reasoningTokens | thoughtsTokenCount | - | 思考token |
| totalTokens | totalTokenCount | - | 总token |
| TokenUsageTracker | ConversationManager | StreamingTokenManager | 统计管理器 |
