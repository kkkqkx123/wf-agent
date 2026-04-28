# MessageStream 事件实现方案（基于 Anthropic SDK）

## Anthropic SDK 实现分析

### 1. MessageStreamMessageEvent (对应 `message` 事件)

**Anthropic SDK 实现**:
- 位置: `ref/anthropic-sdk-v0.71.2/src/lib/MessageStream.ts:474-478`
- 触发时机: 当收到 `message_stop` 事件时
- 实现方式:
  ```typescript
  case 'message_stop': {
    this._addMessageParam(messageSnapshot);
    this._addMessage(messageSnapshot, true);  // 触发 message 事件
    break;
  }
  ```
- `_addMessage` 方法 (第172-177行):
  ```typescript
  protected _addMessage(message: Message, emit = true) {
    this.receivedMessages.push(message);
    if (emit) {
      this._emit('message', message);  // 触发 message 事件
    }
  }
  ```

**我们的实现**:
- 当前: 在 `message_stop` 时只将消息添加到 `receivedMessages`，未触发事件
- 需要补充: 在添加消息后触发 `MESSAGE` 事件

---

### 2. MessageStreamAbortEvent (对应 `abort` 事件)

**Anthropic SDK 实现**:
- 位置: `ref/anthropic-sdk-v0.71.2/src/lib/MessageStream.ts:350-369`
- 触发时机: 在错误处理中检测到中止错误时
- 实现方式:
  ```typescript
  #handleError = (error: unknown) => {
    this.#errored = true;
    if (isAbortError(error)) {
      error = new APIUserAbortError();
    }
    if (error instanceof APIUserAbortError) {
      this.#aborted = true;
      return this._emit('abort', error);  // 触发 abort 事件
    }
    // ... 其他错误处理
  };
  ```
- 检测中止的时机:
  - 第200-202行: 在流处理完成后检查 `stream.controller.signal?.aborted`
  - 第522-524行: 在 `fromReadableStream` 中检查中止信号

**我们的实现**:
- 当前: `abort()` 方法只调用 `controller.abort()`，未触发事件
- 需要补充: 
  1. 在 `abort()` 方法中触发 `ABORT` 事件
  2. 在流处理完成后检查中止状态

---

### 3. MessageStreamToolCallEvent

**Anthropic SDK 实现**:
- Anthropic SDK **没有**单独的 `toolCall` 事件
- 使用 `contentBlock` 事件 (第479-482行):
  ```typescript
  case 'content_block_stop': {
    this._emit('contentBlock', messageSnapshot.content.at(-1)!);
    break;
  }
  ```
- 这个事件会传递整个内容块，包括工具调用块

**我们的实现**:
- 当前: 已有 `CONTENT_BLOCK_STOP` 事件
- 建议: 
  - **方案A**: 移除 `TOOL_CALL` 事件，使用 `CONTENT_BLOCK_STOP` 事件
  - **方案B**: 保留 `TOOL_CALL` 事件，在 `content_block_stop` 时，如果是工具调用块，额外触发 `TOOL_CALL` 事件

**推荐方案A**，因为:
1. 与 Anthropic SDK 保持一致
2. `CONTENT_BLOCK_STOP` 事件已经提供了足够的信息
3. 减少事件类型，简化 API

---

## 实现方案

### 1. 实现 MessageStreamMessageEvent

在 `packages/common-utils/src/llm/message-stream.ts` 的 `accumulateMessage` 方法中修改 `message_stop` 处理:

```typescript
case 'message_stop':
  const message = this.currentMessageSnapshot;
  if (message) {
    this.receivedMessages.push(message);
    this.currentMessageSnapshot = null;
    this.currentTextSnapshot = '';
    
    // 触发消息事件
    this.emit(MessageStreamEventType.MESSAGE, {
      type: MessageStreamEventType.MESSAGE,
      message
    } as MessageStreamMessageEvent);
  }
  return message;
```

### 2. 实现 MessageStreamAbortEvent

#### 2.1 修改 `abort()` 方法

```typescript
abort(): void {
  this.controller.abort();
  
  // 触发中止事件
  this.emit(MessageStreamEventType.ABORT, {
    type: MessageStreamEventType.ABORT,
    reason: 'Stream aborted by user'
  } as MessageStreamAbortEvent);
}
```

#### 2.2 在流处理完成后检查中止状态

需要在调用 `accumulateMessage` 的地方添加中止检查。这通常在客户端实现中完成。

### 3. 处理 MessageStreamToolCallEvent

**推荐**: 移除 `TOOL_CALL` 事件，因为 `CONTENT_BLOCK_STOP` 事件已经提供了相同的功能。

如果需要保留，可以在 `content_block_stop` 处理中添加:

```typescript
case 'content_block_stop':
  // 现有代码...
  
  // 触发内容块停止事件
  if (this.currentMessageSnapshot && Array.isArray(this.currentMessageSnapshot.content)) {
    const lastBlock = this.currentMessageSnapshot.content[this.currentMessageSnapshot.content.length - 1];
    
    this.emit(MessageStreamEventType.CONTENT_BLOCK_STOP, {
      type: MessageStreamEventType.CONTENT_BLOCK_STOP,
      index: this.currentMessageSnapshot.content.length - 1
    } as MessageStreamContentBlockStopEvent);
    
    // 如果是工具调用块，额外触发工具调用事件
    if (lastBlock && (lastBlock.type === 'tool_use' || lastBlock.type === 'server_tool_use')) {
      this.emit(MessageStreamEventType.TOOL_CALL, {
        type: MessageStreamEventType.TOOL_CALL,
        toolCall: lastBlock,
        snapshot: this.currentMessageSnapshot
      } as MessageStreamToolCallEvent);
    }
  }
  break;
```

---

## 实施步骤

1. ✅ 分析 Anthropic SDK 实现
2. ⏳ 实现 `MessageStreamMessageEvent`
3. ⏳ 实现 `MessageStreamAbortEvent`
4. ⏳ 决定 `MessageStreamToolCallEvent` 的处理方式
5. ⏳ 更新测试用例
6. ⏳ 更新文档