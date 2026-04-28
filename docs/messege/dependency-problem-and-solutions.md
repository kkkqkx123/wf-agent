# 依赖关系问题分析与解决方案

## 问题背景

在之前的分析中，我们建议让 `LLMWrapper.generateStream()` 返回 `MessageStream` 并触发 `ABORT` 事件。但是，这个设计可能会导致依赖关系问题。

## 当前依赖关系

```
types (基础类型定义)
  ↑
  |
  +-- common-utils (通用工具库)
        ↑
        |
        +-- sdk (SDK核心模块)
              ↑
              |
              +-- apps (应用层)
```

**依赖说明**:
- `common-utils` 依赖 `types`
- `sdk` 依赖 `types` 和 `common-utils`
- `apps` 依赖 `sdk`

## 潜在的依赖问题

### 问题1: MessageStream 触发 SDK EventManager 事件

**场景**: 如果让 `MessageStream` 在 abort 时触发 SDK EventManager 的事件

**问题代码**:
```typescript
// packages/common-utils/src/llm/message-stream.ts
import { EventManager } from '@modular-agent/sdk'; // ❌ 循环依赖！

export class MessageStream {
  abort(): void {
    this.controller.abort();
    
    // 触发中止事件
    this.emit(MessageStreamEventType.ABORT, {
      type: MessageStreamEventType.ABORT,
      reason: 'Stream aborted by user'
    } as MessageStreamAbortEvent);
    
    // ❌ 问题：尝试触发 SDK EventManager 的事件
    if (this.sdkEventManager) {
      this.sdkEventManager.emit({
        type: EventType.LLM_STREAM_ABORTED,
        // ...
      });
    }
  }
}
```

**依赖循环**:
```
sdk -> common-utils -> sdk (循环依赖!)
```

**为什么这是问题**:
1. `common-utils` 应该是通用的、无业务逻辑的工具库
2. `common-utils` 不应该依赖 `sdk`
3. 这会导致构建失败和模块化设计破坏

### 问题2: MessageStream 需要 SDK 特定的类型

**场景**: 如果 `MessageStream` 需要使用 SDK 特定的类型，如 `ThreadInterruptedException`

**问题代码**:
```typescript
// packages/common-utils/src/llm/message-stream.ts
import { ThreadInterruptedException } from '@modular-agent/types'; // ✅ OK
import { EventManager } from '@modular-agent/sdk'; // ❌ 循环依赖！

export class MessageStream {
  abort(reason?: ThreadInterruptedException): void {
    // ...
  }
}
```

**分析**:
- ✅ 使用 `types` 中的类型是安全的，因为 `common-utils` 已经依赖 `types`
- ❌ 使用 `sdk` 中的类会导致循环依赖

### 问题3: MessageStream 需要访问 SDK 的执行上下文

**场景**: 如果 `MessageStream` 需要访问 SDK 的 `ExecutionContext` 或 `ThreadRegistry`

**问题代码**:
```typescript
// packages/common-utils/src/llm/message-stream.ts
import { ExecutionContext } from '@modular-agent/sdk'; // ❌ 循环依赖！

export class MessageStream {
  constructor(private executionContext?: ExecutionContext) {
    // ...
  }
}
```

**依赖循环**:
```
sdk -> common-utils -> sdk (循环依赖!)
```

## 解决方案

### 方案1: 保持 MessageStream 独立，使用回调函数（推荐）

**核心思想**: `MessageStream` 保持独立，通过回调函数让 SDK 层处理业务逻辑

**实现**:

```typescript
// packages/common-utils/src/llm/message-stream.ts
export interface MessageStreamOptions {
  /** Abort 回调函数 */
  onAbort?: (reason?: string) => void;
  /** 错误回调函数 */
  onError?: (error: Error) => void;
  /** 消息回调函数 */
  onMessage?: (message: LLMMessage) => void;
  /** 工具调用回调函数 */
  onToolCall?: (toolCall: any, snapshot: LLMMessage) => void;
}

export class MessageStream implements AsyncIterable<InternalStreamEvent> {
  private options?: MessageStreamOptions;
  
  constructor(options?: MessageStreamOptions) {
    this.options = options;
  }
  
  abort(): void {
    if (this.aborted || this.ended) {
      return;
    }
    
    this.controller.abort();
    
    // 触发中止事件
    this.emit(MessageStreamEventType.ABORT, {
      type: MessageStreamEventType.ABORT,
      reason: 'Stream aborted by user'
    } as MessageStreamAbortEvent);
    
    // ✅ 调用回调函数，不依赖 SDK
    if (this.options?.onAbort) {
      this.options.onAbort('Stream aborted by user');
    }
  }
}
```

**SDK 层使用**:

```typescript
// sdk/core/llm/wrapper.ts
export class LLMWrapper {
  async generateStream(request: LLMRequest): MessageStream {
    const profile = this.getProfile(request.profileId);
    const client = this.clientFactory.createClient(profile);
    const startTime = now();
    
    // ✅ 创建 MessageStream，传入回调函数
    const stream = new MessageStream({
      onAbort: (reason) => {
        // ✅ 在 SDK 层触发 EventManager 事件
        if (this.eventManager) {
          safeEmit(this.eventManager, {
            type: EventType.LLM_STREAM_ABORTED,
            timestamp: now(),
            workflowId: '',
            threadId: request.threadId || '',
            nodeId: request.nodeId || '',
            reason
          });
        }
      },
      onError: (error) => {
        // ✅ 在 SDK 层处理错误
        if (this.eventManager) {
          safeEmit(this.eventManager, {
            type: EventType.LLM_STREAM_ERROR,
            timestamp: now(),
            workflowId: '',
            threadId: request.threadId || '',
            nodeId: request.nodeId || '',
            error: error.message
          });
        }
      }
    });
    
    try {
      stream.setRequestId(generateId());
      
      // 执行流式调用
      for await (const chunk of client.generateStream(request)) {
        chunk.duration = diffTimestamp(startTime, now());
        
        if (chunk.finishReason) {
          stream.setFinalResult(chunk);
        }
      }
      
      return stream;
    } catch (error) {
      if (error instanceof Error && error.name === 'AbortError') {
        stream.abort(); // ✅ 会触发 onAbort 回调
      }
      throw this.handleError(error, profile);
    }
  }
}
```

**优点**:
- ✅ `MessageStream` 保持独立，不依赖 SDK
- ✅ SDK 可以通过回调函数处理业务逻辑
- ✅ 没有循环依赖
- ✅ 灵活性高，可以自定义回调逻辑

**缺点**:
- ⚠️ 需要修改 `MessageStream` 的构造函数
- ⚠️ 回调函数可能增加复杂度

### 方案2: 使用事件桥接器

**核心思想**: 创建一个事件桥接器，在 SDK 层监听 `MessageStream` 事件并转换为 SDK 事件

**实现**:

```typescript
// sdk/core/llm/message-stream-bridge.ts
import { MessageStream, MessageStreamEventType } from '@modular-agent/common-utils';
import type { EventManager } from '../../services/event-manager';
import { EventType } from '@modular-agent/types';
import { now } from '@modular-agent/common-utils';

/**
 * MessageStream 事件桥接器
 * 将 MessageStream 事件转换为 SDK EventManager 事件
 */
export class MessageStreamBridge {
  constructor(
    private messageStream: MessageStream,
    private eventManager: EventManager,
    private context: {
      threadId?: string;
      nodeId?: string;
      workflowId?: string;
    }
  ) {
    this.setupEventListeners();
  }
  
  private setupEventListeners(): void {
    // 监听 ABORT 事件
    this.messageStream.on(MessageStreamEventType.ABORT, (event) => {
      safeEmit(this.eventManager, {
        type: EventType.LLM_STREAM_ABORTED,
        timestamp: now(),
        workflowId: this.context.workflowId || '',
        threadId: this.context.threadId || '',
        nodeId: this.context.nodeId || '',
        reason: event.reason
      });
    });
    
    // 监听 ERROR 事件
    this.messageStream.on(MessageStreamEventType.ERROR, (event) => {
      safeEmit(this.eventManager, {
        type: EventType.LLM_STREAM_ERROR,
        timestamp: now(),
        workflowId: this.context.workflowId || '',
        threadId: this.context.threadId || '',
        nodeId: this.context.nodeId || '',
        error: event.error.message
      });
    });
    
    // 监听其他事件...
  }
  
  /**
   * 销毁桥接器，移除所有事件监听器
   */
  destroy(): void {
    this.messageStream.off(MessageStreamEventType.ABORT);
    this.messageStream.off(MessageStreamEventType.ERROR);
    // ... 移除其他监听器
  }
}
```

**SDK 层使用**:

```typescript
// sdk/core/llm/wrapper.ts
import { MessageStreamBridge } from './message-stream-bridge';

export class LLMWrapper {
  private eventManager?: EventManager;
  
  setEventManager(eventManager: EventManager): void {
    this.eventManager = eventManager;
  }
  
  async generateStream(request: LLMRequest): MessageStream {
    const profile = this.getProfile(request.profileId);
    const client = this.clientFactory.createClient(profile);
    const startTime = now();
    
    // 创建 MessageStream
    const stream = new MessageStream();
    
    // ✅ 创建事件桥接器
    if (this.eventManager) {
      new MessageStreamBridge(stream, this.eventManager, {
        threadId: request.threadId,
        nodeId: request.nodeId,
        workflowId: request.workflowId
      });
    }
    
    try {
      stream.setRequestId(generateId());
      
      // 执行流式调用
      for await (const chunk of client.generateStream(request)) {
        chunk.duration = diffTimestamp(startTime, now());
        
        if (chunk.finishReason) {
          stream.setFinalResult(chunk);
        }
      }
      
      return stream;
    } catch (error) {
      if (error instanceof Error && error.name === 'AbortError') {
        stream.abort(); // ✅ 会触发 ABORT 事件，桥接器会转换为 SDK 事件
      }
      throw this.handleError(error, profile);
    }
  }
}
```

**优点**:
- ✅ `MessageStream` 保持完全独立
- ✅ 事件转换逻辑集中在桥接器中
- ✅ 没有循环依赖
- ✅ 易于维护和扩展

**缺点**:
- ⚠️ 需要创建额外的桥接器类
- ⚠️ 需要管理桥接器的生命周期

### 方案3: 使用适配器模式

**核心思想**: 创建一个 SDK 特定的 MessageStream 适配器，包装原始的 MessageStream

**实现**:

```typescript
// sdk/core/llm/sdk-message-stream.ts
import { MessageStream, MessageStreamEventType } from '@modular-agent/common-utils';
import type { EventManager } from '../../services/event-manager';
import { EventType } from '@modular-agent/types';
import { now } from '@modular-agent/common-utils';

/**
 * SDK 特定的 MessageStream 适配器
 * 包装原始的 MessageStream，添加 SDK 特定的功能
 */
export class SDKMessageStream extends MessageStream {
  constructor(
    private eventManager?: EventManager,
    private context?: {
      threadId?: string;
      nodeId?: string;
      workflowId?: string;
    }
  ) {
    super();
    this.setupEventListeners();
  }
  
  private setupEventListeners(): void {
    // 监听 ABORT 事件
    this.on(MessageStreamEventType.ABORT, (event) => {
      if (this.eventManager) {
        safeEmit(this.eventManager, {
          type: EventType.LLM_STREAM_ABORTED,
          timestamp: now(),
          workflowId: this.context?.workflowId || '',
          threadId: this.context?.threadId || '',
          nodeId: this.context?.nodeId || '',
          reason: event.reason
        });
      }
    });
    
    // 监听其他事件...
  }
  
  /**
   * 重写 abort 方法，添加 SDK 特定的逻辑
   */
  abort(): void {
    super.abort();
    
    // ✅ 可以在这里添加 SDK 特定的逻辑
    // 例如：记录日志、更新状态等
  }
}
```

**SDK 层使用**:

```typescript
// sdk/core/llm/wrapper.ts
import { SDKMessageStream } from './sdk-message-stream';

export class LLMWrapper {
  private eventManager?: EventManager;
  
  setEventManager(eventManager: EventManager): void {
    this.eventManager = eventManager;
  }
  
  async generateStream(request: LLMRequest): SDKMessageStream {
    const profile = this.getProfile(request.profileId);
    const client = this.clientFactory.createClient(profile);
    const startTime = now();
    
    // ✅ 创建 SDK 特定的 MessageStream
    const stream = new SDKMessageStream(this.eventManager, {
      threadId: request.threadId,
      nodeId: request.nodeId,
      workflowId: request.workflowId
    });
    
    try {
      stream.setRequestId(generateId());
      
      // 执行流式调用
      for await (const chunk of client.generateStream(request)) {
        chunk.duration = diffTimestamp(startTime, now());
        
        if (chunk.finishReason) {
          stream.setFinalResult(chunk);
        }
      }
      
      return stream;
    } catch (error) {
      if (error instanceof Error && error.name === 'AbortError') {
        stream.abort(); // ✅ 会触发 SDK 特定的逻辑
      }
      throw this.handleError(error, profile);
    }
  }
}
```

**优点**:
- ✅ `MessageStream` 保持独立
- ✅ SDK 可以扩展 MessageStream 的功能
- ✅ 没有循环依赖
- ✅ 面向对象的设计，易于扩展

**缺点**:
- ⚠️ 需要创建适配器类
- ⚠️ 返回类型从 `MessageStream` 变为 `SDKMessageStream`

### 方案4: 保持现状，不触发 SDK 事件

**核心思想**: `MessageStream` 只触发自己的事件，不触发 SDK EventManager 的事件

**实现**:

```typescript
// sdk/core/llm/wrapper.ts
export class LLMWrapper {
  async generateStream(request: LLMRequest): MessageStream {
    const profile = this.getProfile(request.profileId);
    const client = this.clientFactory.createClient(profile);
    const startTime = now();
    
    // 创建 MessageStream
    const stream = new MessageStream();
    
    try {
      stream.setRequestId(generateId());
      
      // 执行流式调用
      for await (const chunk of client.generateStream(request)) {
        chunk.duration = diffTimestamp(startTime, now());
        
        if (chunk.finishReason) {
          stream.setFinalResult(chunk);
        }
      }
      
      return stream;
    } catch (error) {
      if (error instanceof Error && error.name === 'AbortError') {
        stream.abort(); // ✅ 只触发 MessageStream.ABORT 事件
      }
      throw this.handleError(error, profile);
    }
  }
}

// sdk/core/execution/executors/llm-executor.ts
export class LLMExecutor {
  async executeLLMCall(
    messages: LLMMessage[],
    requestData: LLMExecutionRequestData,
    options?: { abortSignal?: AbortSignal, threadId?: string, nodeId?: string }
  ): Promise<LLMExecutionResult> {
    const llmRequest = {
      profileId: requestData.profileId,
      messages: messages,
      tools: requestData.tools,
      parameters: requestData.parameters,
      stream: requestData.stream || false,
      signal: options?.abortSignal
    };

    let finalResult: LLMResult | null = null;

    try {
      if (llmRequest.stream) {
        // 流式调用
        const messageStream = await this.llmWrapper.generateStream(llmRequest);
        
        // ✅ 在 SDK 层监听 MessageStream 事件并转换为 SDK 事件
        messageStream.on(MessageStreamEventType.ABORT, (event) => {
          if (this.eventManager) {
            safeEmit(this.eventManager, {
              type: EventType.LLM_STREAM_ABORTED,
              timestamp: now(),
              workflowId: '',
              threadId: options?.threadId || '',
              nodeId: options?.nodeId || '',
              reason: event.reason
            });
          }
        });
        
        // 消费流
        for await (const chunk of messageStream) {
          if (chunk.finishReason) {
            finalResult = chunk;
          }
        }
      } else {
        finalResult = await this.llmWrapper.generate(llmRequest);
      }
      // ... 处理结果
    } catch (error) {
      // ... 错误处理
    }
  }
}
```

**优点**:
- ✅ `MessageStream` 保持完全独立
- ✅ 没有循环依赖
- ✅ 不需要修改 `MessageStream`
- ✅ SDK 层完全控制事件转换

**缺点**:
- ⚠️ 需要在每个使用 `MessageStream` 的地方都添加事件监听器
- ⚠️ 代码重复

## 推荐方案

### 短期推荐：方案4（保持现状）

**理由**:
1. ✅ 最简单，不需要修改 `MessageStream`
2. ✅ 没有循环依赖风险
3. ✅ SDK 层完全控制事件转换
4. ✅ 易于实现和测试

**实施步骤**:
1. 让 `LLMWrapper.generateStream()` 返回 `MessageStream`
2. 在 `LLMExecutor` 中监听 `MessageStream.ABORT` 事件
3. 在事件监听器中触发 SDK EventManager 的事件

### 长期推荐：方案2（事件桥接器）

**理由**:
1. ✅ `MessageStream` 保持完全独立
2. ✅ 事件转换逻辑集中管理
3. ✅ 易于维护和扩展
4. ✅ 符合单一职责原则

**实施步骤**:
1. 创建 `MessageStreamBridge` 类
2. 在 `LLMWrapper` 中使用桥接器
3. 管理桥接器的生命周期

## 依赖关系总结

### 当前依赖关系（正确）

```
types
  ↑
  |
  +-- common-utils
        ↑
        |
        +-- sdk
              ↑
              |
              +-- apps
```

### 错误的依赖关系（避免）

```
types
  ↑
  |
  +-- common-utils ←──┐
        ↑              |
        |              | ❌ 循环依赖
        +-- sdk ────────┘
              ↑
              |
              +-- apps
```

### 正确的依赖关系（推荐）

```
types
  ↑
  |
  +-- common-utils
        ↑
        |
        +-- sdk (使用回调函数或桥接器)
              ↑
              |
              +-- apps
```

## 结论

1. **不要让 `MessageStream` 直接依赖 SDK**
   - `common-utils` 应该保持通用性和独立性
   - 避免循环依赖

2. **使用回调函数或事件桥接器**
   - 让 SDK 层处理业务逻辑
   - `MessageStream` 只负责流式响应事件

3. **推荐方案4（短期）和方案2（长期）**
   - 短期：保持现状，在 SDK 层监听事件
   - 长期：使用事件桥接器，集中管理事件转换

4. **保持清晰的依赖层次**
   - types → common-utils → sdk → apps
   - 不要破坏这个依赖层次