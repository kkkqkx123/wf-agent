# LLM流式生成死循环检测集成方案

## 一、当前项目LLM架构分析

### 1.1 核心组件结构

```
sdk/core/llm/
├── wrapper.ts              # LLM统一入口，协调Profile管理和客户端创建
├── message-stream.ts       # 消息流处理器，事件驱动的流式响应处理
├── base-client.ts          # LLM客户端基类，处理HTTP请求和流式解析
├── clients/                # 具体LLM提供商客户端实现
│   ├── openai-client.ts
│   ├── anthropic-client.ts
│   ├── gemini-client.ts
│   └── ...
├── formatters/             # 响应格式转换器
│   ├── openai-chat.ts      # 支持reasoningContent字段
│   ├── anthropic.ts        # 支持thinking内容
│   ├── gemini-native.ts    # 支持thoughts内容
│   └── ...
└── profile-manager.ts      # Profile管理器
```

### 1.2 流式生成数据流

```
LLMWrapper.generateStream()
    ↓
Client.generateStream() (base-client.ts)
    ↓
SseTransport.executeStream() (HTTP SSE流)
    ↓
Formatter.parseStreamLine() (解析每行数据)
    ↓
Formatter.parseStreamChunk() (解析JSON块，提取reasoningContent)
    ↓
LLMResult chunk (包含content和reasoningContent字段)
    ↓
MessageStream.pushText() (推送文本增量)
    ↓
MessageStream事件触发 (text, streamEvent等)
```

### 1.3 关键观察点

**推理内容来源**：

- OpenAI: `chunk.reasoningContent` 来自 `delta.reasoning_content`
- Anthropic: `chunk.reasoningContent` 来自 thinking block
- Gemini: `chunk.reasoningContent` 来自 thought parts

**当前问题**：

- `wrapper.ts` 第143行只推送了 `chunk.content`，**没有推送 `chunk.reasoningContent`**
- `MessageStream` 只维护 `currentTextSnapshot`，没有独立的 `reasoningMessage` 累积
- 缺少针对推理内容的专门检测机制

## 二、集成方案设计

### 2.1 设计原则

基于 [streaming-dead-loop-detector.md](file:///d:/项目/agent/wf-agent/ref/streaming-dead-loop-detector.md) 的设计规范：

1. **仅检测reasoningMessage**：避免对正常文本内容误判
2. **检查点机制**：基于字符数设置500/1000/2000 tokens三个检查点
3. **三种检测类型**：
   - 类型1：段落内容重复（语义块周期检测）
   - 类型2：有序列表重复（行标准化+周期检测）
   - 类型3：短序列循环（正则匹配）
4. **通用周期检测**：类型1和类型2共用同一套周期检测逻辑

### 2.2 架构调整

#### 方案A：在MessageStream中集成（推荐）

**优势**：

- MessageStream已经是流式响应的核心处理器
- 天然拥有累积文本的能力
- 可以在检测到死循环时直接abort()
- 符合单一职责原则

**改造点**：

1. **扩展MessageStream类**

   ```typescript
   export class MessageStream {
     // 新增：推理内容累积
     private reasoningMessage: string = "";

     // 新增：死循环检测器
     private deadLoopDetector?: DeadLoopDetector;

     // 新增：检查点状态
     private checkpointStates: Map<number, boolean> = new Map();
   }
   ```

2. **修改pushText方法**

   ```typescript
   pushText(delta: string, isReasoning: boolean = false): void {
     if (isReasoning) {
       this.reasoningMessage += delta;
       this.checkDeadLoop(); // 检查死循环
     } else {
       this.currentTextSnapshot += delta;
     }
     // ... 原有逻辑
   }
   ```

3. **新增pushReasoning方法**

   ```typescript
   pushReasoning(delta: string): void {
     this.pushText(delta, true);
   }
   ```

4. **在wrapper.ts中推送推理内容**

   ```typescript
   // wrapper.ts 第143行附近
   if (chunk.content) {
     stream.pushText(chunk.content);
   }

   // 新增：推送推理内容
   if (chunk.reasoningContent) {
     stream.pushReasoning(chunk.reasoningContent);
   }
   ```

#### 方案B：独立检测器中间件

**优势**：

- 完全解耦，不影响现有MessageStream逻辑
- 可以灵活启用/禁用检测功能

**劣势**：

- 需要额外的数据传递机制
- 检测到死循环后需要通过signal中断，不如方案A直接

**实现方式**：
创建一个装饰器模式的包装类，拦截LLMResult流并进行检测。

### 2.3 推荐方案：方案A详细实现

## 三、具体实现步骤

### 步骤1：创建死循环检测器类

**文件位置**: `sdk/core/llm/dead-loop-detector.ts`

```typescript
/**
 * LLM流式生成死循环检测器
 *
 * 基于streaming-dead-loop-detector.md设计规范实现
 */

export interface DeadLoopDetectionResult {
  detected: boolean;
  type?: "short-sequence" | "paragraph-repeat" | "list-repeat";
  details?: string;
}

export interface DeadLoopDetectorConfig {
  /** 检查点阈值（字符数） */
  checkpoints?: number[];
  /** 短序列检测窗口大小（字符） */
  shortSequenceWindow?: number;
  /** 最小重复单元长度（字符） */
  minRepeatUnitLength?: number;
  /** 最小重复次数 */
  minRepeatCount?: number;
  /** 最小周期块/行数 */
  minPeriodElements?: number;
  /** 最大周期长度 */
  maxPeriodLength?: number;
}

export class DeadLoopDetector {
  private config: Required<DeadLoopDetectorConfig>;
  private checkedCheckpoints: Set<number> = new Set();

  constructor(config: DeadLoopDetectorConfig = {}) {
    this.config = {
      checkpoints: config.checkpoints || [500, 1000, 2000],
      shortSequenceWindow: config.shortSequenceWindow || 200,
      minRepeatUnitLength: config.minRepeatUnitLength || 2,
      minRepeatCount: config.minRepeatCount || 4,
      minPeriodElements: config.minPeriodElements || 6,
      maxPeriodLength: config.maxPeriodLength || 50,
    };
  }

  /**
   * 检测死循环
   * @param reasoningMessage 当前累积的推理内容
   * @returns 检测结果
   */
  detect(reasoningMessage: string): DeadLoopDetectionResult {
    const charCount = reasoningMessage.length;

    // 遍历检查点
    for (const checkpoint of this.config.checkpoints) {
      if (charCount >= checkpoint && !this.checkedCheckpoints.has(checkpoint)) {
        this.checkedCheckpoints.add(checkpoint);

        // 根据检查点执行不同的检测
        const result = this.detectAtCheckpoint(reasoningMessage, checkpoint);
        if (result.detected) {
          return result;
        }
      }
    }

    return { detected: false };
  }

  /**
   * 重置检测状态（新的API请求开始时调用）
   */
  reset(): void {
    this.checkedCheckpoints.clear();
  }

  /**
   * 在指定检查点执行检测
   */
  private detectAtCheckpoint(text: string, checkpoint: number): DeadLoopDetectionResult {
    // 获取检测范围的文本片段
    const previousCheckpoint = this.getPreviousCheckpoint(checkpoint);
    const startIndex = previousCheckpoint || 0;
    const segment = text.slice(startIndex);

    // 类型3：短序列循环检测（仅在第1检查点）
    if (checkpoint === this.config.checkpoints[0]) {
      const result = this.detectShortSequence(segment);
      if (result.detected) return result;
    }

    // 类型1和类型2：在第2和第3检查点执行
    if (checkpoint >= this.config.checkpoints[1]) {
      // 类型1：段落内容重复检测
      const paragraphResult = this.detectParagraphRepeat(segment);
      if (paragraphResult.detected) return paragraphResult;

      // 类型2：有序列表重复检测
      const listResult = this.detectListRepeat(segment);
      if (listResult.detected) return listResult;
    }

    return { detected: false };
  }

  /**
   * 类型3：短序列循环检测
   */
  private detectShortSequence(text: string): DeadLoopDetectionResult {
    // 取最近N个字符
    const windowSize = Math.min(this.config.shortSequenceWindow, text.length);
    const recentText = text.slice(-windowSize);

    // 正则匹配：至少2个字符的子串连续重复至少4次
    const pattern = new RegExp(
      `(.{${this.config.minRepeatUnitLength},})\\1{${this.config.minRepeatCount - 1},}`,
      "s",
    );

    const match = recentText.match(pattern);
    if (match) {
      return {
        detected: true,
        type: "short-sequence",
        details: `Detected short sequence loop: "${match[1]}" repeated`,
      };
    }

    return { detected: false };
  }

  /**
   * 类型1：段落内容重复检测
   */
  private detectParagraphRepeat(text: string): DeadLoopDetectionResult {
    // 步骤1：语义块分割
    const blocks = this.splitIntoSemanticBlocks(text);

    if (blocks.length < this.config.minPeriodElements) {
      return { detected: false };
    }

    // 步骤2：调用通用周期检测
    const periodResult = this.detectPeriod(blocks);
    if (periodResult.detected) {
      return {
        detected: true,
        type: "paragraph-repeat",
        details: `Detected paragraph repeat with period ${periodResult.period}`,
      };
    }

    return { detected: false };
  }

  /**
   * 类型2：有序列表重复检测
   */
  private detectListRepeat(text: string): DeadLoopDetectionResult {
    // 步骤1：按行分割
    const lines = text.split("\n");

    if (lines.length < this.config.minPeriodElements) {
      return { detected: false };
    }

    // 步骤2：行标准化（去除有序列表标号）
    const normalizedLines = lines.map(line => this.normalizeListItem(line));

    // 步骤3：调用通用周期检测
    const periodResult = this.detectPeriod(normalizedLines);
    if (periodResult.detected) {
      return {
        detected: true,
        type: "list-repeat",
        details: `Detected list repeat with period ${periodResult.period}`,
      };
    }

    return { detected: false };
  }

  /**
   * 通用周期检测逻辑（类型1和类型2共用）
   */
  private detectPeriod(elements: string[]): { detected: boolean; period?: number } {
    const maxPeriod = Math.min(this.config.maxPeriodLength, Math.floor(elements.length / 2));

    for (let p = 1; p <= maxPeriod; p++) {
      let consecutiveCount = 0;

      // 从末尾向前检查
      for (let i = elements.length - 1; i >= p; i--) {
        if (elements[i] === elements[i - p]) {
          consecutiveCount++;
        } else {
          break;
        }
      }

      if (consecutiveCount >= this.config.minPeriodElements) {
        return { detected: true, period: p };
      }
    }

    return { detected: false };
  }

  /**
   * 语义块分割
   */
  private splitIntoSemanticBlocks(text: string): string[] {
    // 以自然语言边界符分割：。.!！;；?？\n
    const separators = /[。.!！;；?？\n]+/;
    const blocks = text.split(separators).filter(block => block.trim().length > 0);
    return blocks;
  }

  /**
   * 有序列表行标准化
   */
  private normalizeListItem(line: string): string {
    // 匹配有序列表标号模式：1. 2. 10. 等
    const listPattern = /^\d+\.\s*/;
    return line.replace(listPattern, "");
  }

  /**
   * 获取上一个检查点
   */
  private getPreviousCheckpoint(current: number): number | null {
    const index = this.config.checkpoints.indexOf(current);
    if (index <= 0) return null;
    return this.config.checkpoints[index - 1];
  }
}
```

### 步骤2：扩展MessageStream类

**文件**: `sdk/core/llm/message-stream.ts`

```typescript
// 在类定义中添加导入
import { DeadLoopDetector, DeadLoopDetectionResult } from "./dead-loop-detector.js";

export class MessageStream implements AsyncIterable<InternalStreamEvent> {
  // ... 现有属性

  // 新增：推理内容累积
  private reasoningMessage: string = "";

  // 新增：死循环检测器
  private deadLoopDetector: DeadLoopDetector;

  // 新增：死循环检测回调
  private onDeadLoopDetected?: (result: DeadLoopDetectionResult) => void;

  constructor(onDeadLoopDetected?: (result: DeadLoopDetectionResult) => void) {
    // ... 现有初始化代码

    // 初始化管理器
    this.deadLoopDetector = new DeadLoopDetector();
    this.onDeadLoopDetected = onDeadLoopDetected;
  }

  /**
   * 推送推理内容增量
   * @param delta 推理内容增量
   */
  pushReasoning(delta: string): void {
    if (this.ended || this.errored || this.aborted) {
      return;
    }

    this.reasoningMessage += delta;

    // 检测死循环
    const detectionResult = this.deadLoopDetector.detect(this.reasoningMessage);
    if (detectionResult.detected) {
      logger.warn("Dead loop detected in reasoning content", {
        type: detectionResult.type,
        details: detectionResult.details,
        reasoningLength: this.reasoningMessage.length,
      });

      // 触发回调（如果设置了）
      if (this.onDeadLoopDetected) {
        this.onDeadLoopDetected(detectionResult);
      }

      // 自动中止流
      this.abort();
      return;
    }

    // 可选：触发推理内容事件
    this.emit("reasoningText", {
      type: "reasoningText",
      delta,
      snapshot: this.reasoningMessage,
    } as MessageStreamReasoningTextEvent);
  }

  /**
   * 重置死循环检测器（新的API请求开始时调用）
   */
  resetDeadLoopDetector(): void {
    this.reasoningMessage = "";
    this.deadLoopDetector.reset();
  }
}
```

### 步骤3：修改wrapper.ts推送推理内容

**文件**: `sdk/core/llm/wrapper.ts`

```typescript
async generateStream(request: LLMRequest): Promise<Result<MessageStream, LLMError>> {
  // ... 现有代码

  const result = await tryCatchAsyncWithSignal(async signal => {
    stream.setRequestId(generateId());

    // 重置死循环检测器
    stream.resetDeadLoopDetector();

    try {
      for await (const chunk of client.generateStream({ ...request, signal })) {
        const nowTime = now();

        // Update streaming statistics
        chunkCount++;
        if (firstChunkTime === undefined) {
          firstChunkTime = nowTime;
        }
        lastChunkTime = nowTime;

        chunk.duration = diffTimestamp(startTime, nowTime);

        // Push the text content to MessageStream.
        if (chunk.content) {
          stream.pushText(chunk.content);
        }

        // 新增：Push reasoning content to MessageStream
        if (chunk.reasoningContent) {
          stream.pushReasoning(chunk.reasoningContent);
        }

        if (chunk.finishReason) {
          stream.setFinalResult(chunk);
        }
      }

      // End the stream when it is completed normally.
      stream.end();
    } catch (error) {
      // If it is a termination error, the MessageStream needs to be terminated to correctly update its internal state.
      if (isAbortError(error)) {
        stream.abort();
      }
      throw error;
    }

    // ... 后续代码
  }, request.signal);

  // ... 返回结果
}
```

### 步骤4：添加新的事件类型（可选）

**文件**: `packages/types/src/events/message-stream.ts`

```typescript
// 新增事件类型
export type MessageStreamEventType =
  | "connect"
  | "streamEvent"
  | "text"
  | "inputJson"
  | "message"
  | "finalMessage"
  | "error"
  | "abort"
  | "end"
  | "reasoningText"; // 新增

// 新增事件接口
export interface MessageStreamReasoningTextEvent {
  type: "reasoningText";
  delta: string;
  snapshot: string;
}
```

### 步骤5：配置化支持

允许用户通过配置启用/禁用死循环检测：

```typescript
// 在LLMProfile或LLMRequest中添加配置
export interface LLMRequest {
  // ... 现有字段

  /** 死循环检测配置 */
  deadLoopDetection?: {
    enabled?: boolean;
    config?: DeadLoopDetectorConfig;
  };
}

// 在MessageStream构造函数中根据配置初始化
constructor(options?: {
  enableDeadLoopDetection?: boolean;
  deadLoopConfig?: DeadLoopDetectorConfig;
  onDeadLoopDetected?: (result: DeadLoopDetectionResult) => void;
}) {
  if (options?.enableDeadLoopDetection !== false) {
    this.deadLoopDetector = new DeadLoopDetector(options?.deadLoopConfig);
    this.onDeadLoopDetected = options?.onDeadLoopDetected;
  }
}
```

## 四、测试策略

### 4.1 单元测试

**文件**: `sdk/core/llm/__tests__/dead-loop-detector.test.ts`

```typescript
import { describe, it, expect } from "vitest";
import { DeadLoopDetector } from "../dead-loop-detector.js";

describe("DeadLoopDetector", () => {
  describe("类型3：短序列循环检测", () => {
    it("应该检测到短序列重复", () => {
      const detector = new DeadLoopDetector();
      const text = "我需要思考".repeat(10);
      const result = detector.detect(text);

      expect(result.detected).toBe(true);
      expect(result.type).toBe("short-sequence");
    });

    it("不应该误判正常文本", () => {
      const detector = new DeadLoopDetector();
      const text = "这是一个正常的句子，没有任何重复模式。";
      const result = detector.detect(text.repeat(3));

      expect(result.detected).toBe(false);
    });
  });

  describe("类型1：段落内容重复检测", () => {
    it("应该检测到段落重复", () => {
      const detector = new DeadLoopDetector();
      const text = "今天天气真好。我们出去玩吧！".repeat(10);
      const result = detector.detect(text);

      expect(result.detected).toBe(true);
      expect(result.type).toBe("paragraph-repeat");
    });
  });

  describe("类型2：有序列表重复检测", () => {
    it("应该检测到有序列表重复", () => {
      const detector = new DeadLoopDetector();
      const text = Array.from(
        { length: 12 },
        (_, i) => `${i + 1}. 分析需求\n${i + 2}. 设计方案`,
      ).join("\n");

      const result = detector.detect(text);

      expect(result.detected).toBe(true);
      expect(result.type).toBe("list-repeat");
    });
  });

  describe("检查点机制", () => {
    it("应该在达到检查点时才检测", () => {
      const detector = new DeadLoopDetector({
        checkpoints: [100, 200, 300],
      });

      // 未达到检查点
      const shortText = "重复".repeat(10);
      expect(detector.detect(shortText).detected).toBe(false);

      // 达到第一个检查点
      const longText = "重复".repeat(100);
      const result = detector.detect(longText);
      expect(result.detected).toBe(true);
    });
  });
});
```

### 4.2 集成测试

**文件**: `sdk/core/llm/__tests__/message-stream-dead-loop.int.test.ts`

```typescript
import { describe, it, expect, vi } from "vitest";
import { MessageStream } from "../message-stream.js";

describe("MessageStream with Dead Loop Detection", () => {
  it("应该在检测到死循环时自动abort", async () => {
    const onDeadLoopDetected = vi.fn();
    const stream = new MessageStream({
      enableDeadLoopDetection: true,
      onDeadLoopDetected,
    });

    // 模拟推送大量重复的推理内容
    const repetitiveContent = "我需要分析这个问题。".repeat(100);
    stream.pushReasoning(repetitiveContent);

    // 验证是否触发了死循环检测
    expect(onDeadLoopDetected).toHaveBeenCalled();
    expect(stream.isAborted()).toBe(true);
  });

  it("正常内容不应该触发死循环检测", () => {
    const onDeadLoopDetected = vi.fn();
    const stream = new MessageStream({
      enableDeadLoopDetection: true,
      onDeadLoopDetected,
    });

    const normalContent = "这是一段正常的推理内容，没有重复模式。";
    stream.pushReasoning(normalContent.repeat(50));

    expect(onDeadLoopDetected).not.toHaveBeenCalled();
    expect(stream.isAborted()).toBe(false);
  });
});
```

## 五、性能优化考虑

### 5.1 检测频率控制

- 仅在检查点触发检测，避免每个chunk都检测
- 默认检查点：500/1000/2000字符，平衡检测及时性和性能

### 5.2 检测范围限制

- 类型3：仅检测最近200字符
- 类型1和类型2：仅检测上一个检查点到当前检查点之间的片段
- 避免对整个历史文本重复检测

### 5.3 早期退出优化

```typescript
// 在detectPeriod中，如果元素数量不足，立即返回
if (elements.length < this.config.minPeriodElements) {
  return { detected: false };
}
```

### 5.4 可配置性

允许用户根据场景调整参数：

- 降低检查点阈值 → 更早检测但可能误判
- 提高minPeriodElements → 减少误判但可能漏检
- 禁用检测 → 对性能敏感的场景

## 六、错误处理和日志

### 6.1 日志记录

```typescript
logger.warn("Dead loop detected in reasoning content", {
  type: detectionResult.type,
  details: detectionResult.details,
  reasoningLength: this.reasoningMessage.length,
  requestId: this.requestId,
});
```

### 6.2 事件通知

除了自动abort，还可以：

- 触发自定义回调 `onDeadLoopDetected`
- 通过EventManager发出事件
- 记录到metrics系统用于监控

### 6.3 优雅降级

如果检测器抛出异常，不应影响正常流：

```typescript
try {
  const detectionResult = this.deadLoopDetector.detect(this.reasoningMessage);
  // ... 处理结果
} catch (error) {
  logger.error("Dead loop detector error, skipping detection", { error });
  // 继续正常流程，不中断
}
```

## 七、兼容性考虑

### 7.1 向后兼容

- 默认启用检测，但可以通过配置禁用
- 不改变现有API签名（使用可选参数）
- 不影响非推理模型的正常使用

### 7.2 多模型支持

检测器对以下模型的reasoningContent都有效：

- OpenAI: DeepSeek R1, o1系列
- Anthropic: Claude with extended thinking
- Gemini: Gemini thinking models

### 7.3 与现有中断机制集成

检测到死循环后调用`stream.abort()`，会：

- 触发abort事件
- 清理资源
- 与现有的InterruptionState机制兼容

## 八、配置使用示例

### 8.1 基本使用（默认启用）

```typescript
import { LLMWrapper } from "@wf-agent/sdk";

const wrapper = new LLMWrapper();

// 默认启用死循环检测
const result = await wrapper.generateStream({
  profileId: "openai-gpt4",
  messages: [{ role: "user", content: "Solve this problem..." }],
});
```

### 8.2 自定义检测参数

```typescript
// 调整检查点阈值和检测参数
const result = await wrapper.generateStream({
  profileId: "openai-gpt4",
  messages: [{ role: "user", content: "Analyze..." }],
  deadLoopDetection: {
    enabled: true,
    checkpoints: [300, 600, 1200], // 更早的检测点
    minRepeatCount: 3, // 更敏感的重复检测
    minPeriodElements: 4, // 更少的周期元素要求
  },
});
```

### 8.3 禁用死循环检测

```typescript
// 对性能敏感的场景可以禁用检测
const result = await wrapper.generateStream({
  profileId: "mock-llm",
  messages: [{ role: "user", content: "Test..." }],
  deadLoopDetection: {
    enabled: false,
  },
});
```

### 8.4 监听死循环事件

```typescript
const stream = new MessageStream({
  enableDeadLoopDetection: true,
  onDeadLoopDetected: result => {
    console.log("Dead loop detected:", result.type, result.details);
    // 可以执行自定义逻辑，如记录日志、发送告警等
  },
});

// 也可以通过事件系统监听推理内容
stream.on("reasoningText", data => {
  console.log("Reasoning delta:", data.delta);
  console.log("Current reasoning:", data.snapshot);
});
```

## 九、实施路线图

### Phase 1: 核心实现（1-2天）

1. 创建 `dead-loop-detector.ts`
2. 编写单元测试
3. 验证检测逻辑正确性

### Phase 2: MessageStream集成（1天）

1. 扩展MessageStream类
2. 添加pushReasoning方法
3. 集成检测器调用

### Phase 3: Wrapper层适配（0.5天）

1. 修改wrapper.ts推送reasoningContent
2. 添加reset逻辑

### Phase 4: 配置化和优化（1天）

1. 添加配置选项
2. 性能测试和优化
3. 完善日志和错误处理

### Phase 5: 集成测试和文档（1天）

1. 编写集成测试
2. 更新文档
3. Code review和修复问题

**总计**: 4-5.5天

## 十、风险评估

### 9.1 潜在风险

| 风险             | 影响           | 缓解措施                 |
| ---------------- | -------------- | ------------------------ |
| 误判正常重复内容 | 用户体验下降   | 调整阈值，增加白名单机制 |
| 性能开销         | 响应延迟增加   | 检查点机制，早期退出优化 |
| 检测遗漏         | 死循环未被发现 | 多个检查点，多种检测类型 |
| 与现有逻辑冲突   | 功能异常       | 充分测试，优雅降级       |

### 9.2 监控指标

建议添加以下metrics：

- 死循环检测触发次数
- 误判率（人工反馈）
- 检测耗时
- 平均在哪个检查点检测到

## 十一、总结

本方案基于项目现有的LLM架构，采用**在MessageStream中集成死循环检测器**的方式，具有以下优势：

1. **最小侵入性**：只需修改3个文件（detector、message-stream、wrapper）
2. **符合设计规范**：严格遵循streaming-dead-loop-detector.md的要求
3. **高性能**：检查点机制避免频繁检测
4. **可配置**：支持启用/禁用和参数调整
5. **易维护**：检测逻辑独立封装，便于优化和扩展

实施后，系统将能够自动检测并中断LLM流式生成中的死循环，提升用户体验和资源利用率。
