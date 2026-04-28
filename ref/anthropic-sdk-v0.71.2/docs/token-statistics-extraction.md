# Anthropic SDK 流式响应中的 Token 统计信息提取机制分析

## 概述

本文档详细分析了 Anthropic SDK 如何从流式响应中提取和累积 token 统计信息。SDK 通过一个复杂的事件处理机制来实时收集和更新 token 使用情况。

## 核心机制

### 1. 流式事件类型

SDK 处理包含 token 使用信息的多种流式事件：
- `message_start`: 包含初始使用统计信息，包括 `input_tokens` 和初始的 `output_tokens`
- `message_delta`: 包含使用统计信息的增量更新，特别是 `output_tokens`
- `message_stop`: 标记消息流结束

### 2. Token 累积过程

在 `MessageStream.ts` 和 `BetaMessageStream.ts` 中，`#accumulateMessage()` 方法负责累积 token 统计信息：

```typescript
case 'message_delta':
  snapshot.stop_reason = event.delta.stop_reason;
  snapshot.stop_sequence = event.delta.stop_sequence;
  snapshot.usage.output_tokens = event.usage.output_tokens;

  // 如果事件中存在，则更新其他使用字段
  if (event.usage.input_tokens != null) {
    snapshot.usage.input_tokens = event.usage.input_tokens;
  }

  if (event.usage.cache_creation_input_tokens != null) {
    snapshot.usage.cache_creation_input_tokens = event.usage.cache_creation_input_tokens;
  }

  if (event.usage.cache_read_input_tokens != null) {
    snapshot.usage.cache_read_input_tokens = event.usage.cache_read_input_tokens;
  }

  if (event.usage.server_tool_use != null) {
    snapshot.usage.server_tool_use = event.usage.server_tool_use;
  }

  return snapshot;
```

### 3. 使用接口结构

SDK 定义了两个主要接口用于 token 使用：

- **Usage 接口**: 包含完整的 token 统计信息，包括：
  - `input_tokens`: 输入 token 数量
  - `output_tokens`: 输出 token 数量
  - `cache_creation_input_tokens`: 用于缓存创建的 token
  - `cache_read_input_tokens`: 从缓存读取的 token
  - `server_tool_use`: 服务器工具使用统计

- **MessageDeltaUsage 接口**: 包含流式传输期间的增量更新

### 4. 实时更新

随着流式响应的进行：
1. `message_start` 事件提供初始 token 计数（尤其是输入 token）
2. `message_delta` 事件提供输出 token 计数的持续更新
3. 最终累积值表示整个请求的完整 token 使用情况

### 5. 事件处理

SDK 的 `core/streaming.ts` 中的 `_iterSSEMessages` 函数处理原始 SSE 流，解析单个事件并将它们传递给流处理器。每个事件按顺序处理，token 统计信息被增量更新。

### 6. 最终结果

当流以 `message_stop` 事件完成时，SDK 已经累积了所有 token 统计信息到最终的 `Message` 对象中，可以通过 `message.usage.input_tokens`、`message.usage.output_tokens` 等方式访问。

## 总结

这种设计使 SDK 能够即使在长时间运行的流式操作期间也提供准确的 token 统计信息，统计信息在响应过程中不断更新。SDK 通过事件驱动的方式处理流式数据，确保在任何时间点都能获得最新的 token 使用情况。