# CLI-App 消息输出系统 PRD

## 1. 概述

### 1.1 设计目标

消息输出系统负责**统一决策**各类内容的输出目标，协调 TUI 和文件 IO 两个输出通道：

- **TUI 职责**：流式 LLM 消息、工具调用摘要、简单用户输入、可用操作提示
- **文件 IO 职责**：Human Relay 完整提示词、工具调用详情、执行日志、聚合呈现

### 1.2 核心原则

1. **单一职责** - 消息系统只负责"决定输出到哪里"，不处理具体渲染或文件写入
2. **TUI 轻量** - TUI 只处理轻量级、高频、需要实时反馈的内容
3. **文件分担** - 大段内容、复杂交互、历史记录通过文件 IO 处理
4. **统一路由** - 所有输出通过消息系统统一路由，避免分散决策

### 1.3 与文件 IO 系统的关系

```
┌─────────────────────────────────────────────────────────────────┐
│                      消息输出系统                                │
│                   (Message Output System)                        │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────┐  │
│  │   消息路由器     │───▶│   输出决策器     │───▶│  TUI 通道   │  │
│  │  MessageRouter  │    │ OutputDecider   │    │  (轻量内容)  │  │
│  └─────────────────┘    └─────────────────┘    └─────────────┘  │
│           │                                              │       │
│           │    ┌─────────────────────────────────────────┘       │
│           │    │                                                 │
│           ▼    ▼                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    文件 IO 系统                           │   │
│  │              (File IO System - file-io-prd.md)            │   │
│  │  ┌─────────────────┐    ┌─────────────────────────────┐   │   │
│  │  │  Functional IO  │    │      Display IO             │   │   │
│  │  │  (程序间交换)    │    │    (人类阅读)                │   │   │
│  │  │                 │    │                             │   │   │
│  │  │ • human-relay   │    │ • output.md (聚合呈现)       │   │   │
│  │  │   -output.txt   │    │ • execution-log.md          │   │   │
│  │  │ • human-relay   │    │ • sub-instances/            │   │   │
│  │  │   -input.txt    │    │                             │   │   │
│  │  └─────────────────┘    └─────────────────────────────┘   │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. 消息类型体系

### 2.1 消息分类（8大类别）

基于 component-message-types.md 的完整分类：

```
Message
├── SystemMessage          # 系统级消息
│   ├── StartupMessage
│   ├── ShutdownMessage
│   ├── ConfigChangeMessage
│   └── ErrorMessage
│
├── AgentMessage           # Agent 相关消息
│   ├── AgentStartMessage
│   ├── AgentPauseMessage
│   ├── AgentResumeMessage
│   ├── AgentEndMessage
│   ├── AgentIterationMessage
│   ├── AgentStreamMessage
│   └── AgentErrorMessage
│
├── ToolMessage            # 工具调用消息
│   ├── ToolCallStartMessage
│   ├── ToolCallEndMessage
│   ├── ToolResultMessage
│   └── ToolErrorMessage
│
├── HumanRelayMessage      # Human Relay 消息
│   ├── HumanRelayRequestMessage
│   ├── HumanRelayResponseMessage
│   ├── HumanRelayTimeoutMessage
│   └── HumanRelayCancelMessage
│
├── WorkflowMessage        # 工作流消息
│   ├── WorkflowStartMessage
│   ├── WorkflowEndMessage
│   ├── WorkflowNodeStartMessage
│   ├── WorkflowNodeEndMessage
│   ├── WorkflowNodeErrorMessage
│   └── WorkflowCheckpointMessage
│
├── ThreadMessage          # 线程消息
│   ├── ThreadStartMessage
│   ├── ThreadStatusMessage
│   ├── ThreadPauseMessage
│   ├── ThreadResumeMessage
│   └── ThreadEndMessage
│
├── CheckpointMessage      # 检查点消息
│   ├── CheckpointCreateMessage
│   ├── CheckpointRestoreMessage
│   └── CheckpointDeleteMessage
│
└── EventMessage           # 事件消息
    ├── EventTriggerMessage
    ├── EventProcessMessage
    └── EventCompleteMessage
```

### 2.2 输出目标决策矩阵

| 消息类型 | TUI | 文件 IO | 说明 |
|---------|-----|---------|------|
| **AgentStreamMessage** | ✓ | ✗ | LLM 流式输出，仅 TUI |
| **AgentIterationMessage** | ✓ | ✓ (display) | 迭代摘要 TUI，详情写入 display |
| **ToolCallStartMessage** | ✓ | ✗ | 工具调用开始，TUI 显示摘要 |
| **ToolCallEndMessage** | ✓ | ✗ | 工具调用结束，TUI 显示耗时 |
| **ToolResultMessage** | ✗ | ✓ (display) | 工具结果详情，写入 display |
| **HumanRelayRequestMessage** | ✓ | ✓ (functional) | TUI 显示提示，完整内容写入 functional |
| **HumanRelayResponseMessage** | ✓ | ✗ | TUI 确认收到，不写入文件 |
| **WorkflowNodeStartMessage** | ✓ | ✓ (display) | TUI 显示节点开始 |
| **WorkflowNodeEndMessage** | ✓ | ✓ (display) | TUI 显示节点结束 |
| **CheckpointCreateMessage** | ✗ | ✓ (display) | 仅写入 display |
| **SystemErrorMessage** | ✓ | ✓ (display) | 错误信息双通道输出 |

### 2.3 核心消息接口

```typescript
/**
 * 消息类别
 */
enum MessageCategory {
  SYSTEM = 'system',
  AGENT = 'agent',
  TOOL = 'tool',
  HUMAN_RELAY = 'human_relay',
  WORKFLOW = 'workflow',
  THREAD = 'thread',
  CHECKPOINT = 'checkpoint',
  EVENT = 'event',
}

/**
 * 消息类型枚举（完整版）
 */
enum MessageType {
  // System (1000-1099)
  SYSTEM_STARTUP = 'system.startup',
  SYSTEM_SHUTDOWN = 'system.shutdown',
  SYSTEM_CONFIG_CHANGE = 'system.config_change',
  SYSTEM_ERROR = 'system.error',
  
  // Agent (2000-2099)
  AGENT_START = 'agent.start',
  AGENT_PAUSE = 'agent.pause',
  AGENT_RESUME = 'agent.resume',
  AGENT_END = 'agent.end',
  AGENT_ITERATION = 'agent.iteration',
  AGENT_STREAM = 'agent.stream',
  AGENT_ERROR = 'agent.error',
  
  // Tool (3000-3099)
  TOOL_CALL_START = 'tool.call_start',
  TOOL_CALL_END = 'tool.call_end',
  TOOL_RESULT = 'tool.result',
  TOOL_ERROR = 'tool.error',
  
  // Human Relay (4000-4099)
  HUMAN_RELAY_REQUEST = 'human_relay.request',
  HUMAN_RELAY_RESPONSE = 'human_relay.response',
  HUMAN_RELAY_TIMEOUT = 'human_relay.timeout',
  HUMAN_RELAY_CANCEL = 'human_relay.cancel',
  
  // Workflow (5000-5099)
  WORKFLOW_START = 'workflow.start',
  WORKFLOW_END = 'workflow.end',
  WORKFLOW_NODE_START = 'workflow.node_start',
  WORKFLOW_NODE_END = 'workflow.node_end',
  WORKFLOW_NODE_ERROR = 'workflow.node_error',
  WORKFLOW_CHECKPOINT = 'workflow.checkpoint',
  
  // Thread (6000-6099)
  THREAD_START = 'thread.start',
  THREAD_STATUS = 'thread.status',
  THREAD_PAUSE = 'thread.pause',
  THREAD_RESUME = 'thread.resume',
  THREAD_END = 'thread.end',
  
  // Checkpoint (7000-7099)
  CHECKPOINT_CREATE = 'checkpoint.create',
  CHECKPOINT_RESTORE = 'checkpoint.restore',
  CHECKPOINT_DELETE = 'checkpoint.delete',
  
  // Event (8000-8099)
  EVENT_TRIGGER = 'event.trigger',
  EVENT_PROCESS = 'event.process',
  EVENT_COMPLETE = 'event.complete',
}

/**
 * 输出目标
 */
enum OutputTarget {
  TUI = 'tui',           // TUI 显示
  FILE_FUNCTIONAL = 'file_functional',  // 功能性文件 IO
  FILE_DISPLAY = 'file_display',        // 展示性文件 IO
  LOG = 'log',           // 日志
  NONE = 'none',         // 不输出
}

/**
 * 基础消息接口
 */
interface BaseMessage {
  readonly id: string;
  readonly type: MessageType;
  readonly category: MessageCategory;
  readonly timestamp: number;
  readonly level: 'debug' | 'info' | 'warn' | 'error' | 'critical';
  readonly sessionId: string;
  readonly source: string;
  readonly data: unknown;
}
```

---

## 3. 输出决策器

### 3.1 决策器职责

```typescript
/**
 * 输出决策器
 * 决定消息应该输出到哪些目标
 */
interface OutputDecider {
  /**
   * 决定消息输出目标
   * @returns 输出目标数组，可能包含多个目标
   */
  decide(message: BaseMessage): OutputTarget[];
  
  /**
   * 配置决策规则
   */
  configure(rules: DecisionRule[]): void;
}

/**
 * 决策规则
 */
interface DecisionRule {
  /** 匹配条件 */
  condition: {
    types?: MessageType[];
    categories?: MessageCategory[];
    levels?: string[];
    custom?: (message: BaseMessage) => boolean;
  };
  /** 输出目标 */
  targets: OutputTarget[];
  /** 优先级（数字越小优先级越高） */
  priority: number;
}
```

### 3.2 默认决策规则

```typescript
const DEFAULT_DECISION_RULES: DecisionRule[] = [
  // Rule 1: LLM 流式输出仅 TUI
  {
    condition: { types: [MessageType.AGENT_STREAM] },
    targets: [OutputTarget.TUI],
    priority: 100,
  },
  
  // Rule 2: Human Relay 请求 -> TUI + Functional File
  {
    condition: { types: [MessageType.HUMAN_RELAY_REQUEST] },
    targets: [OutputTarget.TUI, OutputTarget.FILE_FUNCTIONAL],
    priority: 100,
  },
  
  // Rule 3: 工具结果 -> Display File（详情）
  {
    condition: { types: [MessageType.TOOL_RESULT] },
    targets: [OutputTarget.FILE_DISPLAY],
    priority: 100,
  },
  
  // Rule 4: 工具调用开始/结束 -> TUI（摘要）
  {
    condition: { 
      types: [
        MessageType.TOOL_CALL_START, 
        MessageType.TOOL_CALL_END 
      ] 
    },
    targets: [OutputTarget.TUI],
    priority: 100,
  },
  
  // Rule 5: 检查点消息 -> Display File
  {
    condition: { category: [MessageCategory.CHECKPOINT] },
    targets: [OutputTarget.FILE_DISPLAY],
    priority: 100,
  },
  
  // Rule 6: 错误消息 -> TUI + Display File
  {
    condition: { levels: ['error', 'critical'] },
    targets: [OutputTarget.TUI, OutputTarget.FILE_DISPLAY],
    priority: 50,  // 高优先级，优先匹配
  },
  
  // Rule 7: 默认规则 -> TUI
  {
    condition: {},
    targets: [OutputTarget.TUI],
    priority: 999,
  },
];
```

---

## 4. 消息处理器

### 4.1 处理器接口

```typescript
/**
 * 消息处理器
 */
interface MessageHandler {
  readonly name: string;
  readonly target: OutputTarget;
  
  /**
   * 处理消息
   */
  handle(message: BaseMessage): Promise<void> | void;
  
  /**
   * 是否支持该消息
   */
  supports(message: BaseMessage): boolean;
  
  /**
   * 刷新/关闭
   */
  flush?(): Promise<void>;
  close?(): Promise<void>;
}
```

### 4.2 内置处理器

| 处理器 | 目标 | 职责 |
|--------|------|------|
| TUIHandler | TUI | 将消息渲染到 TUI 界面 |
| FunctionalFileHandler | FILE_FUNCTIONAL | 写入 human-relay-output.txt 等 |
| DisplayFileHandler | FILE_DISPLAY | 更新 output.md 聚合文件 |
| LogHandler | LOG | 写入应用日志 |

### 4.3 TUIHandler

```typescript
/**
 * TUI 处理器
 * 处理轻量级、需要实时反馈的消息
 */
class TUIHandler implements MessageHandler {
  readonly name = 'tui';
  readonly target = OutputTarget.TUI;
  
  // 支持的消息类型（白名单）
  private readonly supportedTypes = new Set<MessageType>([
    MessageType.AGENT_STREAM,
    MessageType.AGENT_ITERATION,
    MessageType.TOOL_CALL_START,
    MessageType.TOOL_CALL_END,
    MessageType.HUMAN_RELAY_REQUEST,
    MessageType.HUMAN_RELAY_RESPONSE,
    MessageType.WORKFLOW_NODE_START,
    MessageType.WORKFLOW_NODE_END,
    MessageType.SYSTEM_ERROR,
  ]);
  
  supports(message: BaseMessage): boolean {
    return this.supportedTypes.has(message.type);
  }
  
  handle(message: BaseMessage): void {
    switch (message.type) {
      case MessageType.AGENT_STREAM:
        this.renderStream(message.data as AgentStreamMessage['data']);
        break;
      case MessageType.TOOL_CALL_START:
        this.renderToolStart(message.data as ToolCallStartMessage['data']);
        break;
      case MessageType.HUMAN_RELAY_REQUEST:
        this.renderHumanRelayPrompt(message.data as HumanRelayRequestMessage['data']);
        break;
      // ... 其他类型
    }
  }
  
  private renderStream(data: AgentStreamMessage['data']): void {
    // 流式输出到 TUI
    tui.appendText(data.chunk);
  }
  
  private renderToolStart(data: ToolCallStartMessage['data']): void {
    // 显示工具调用摘要
    tui.showToolCall({
      name: data.toolName,
      summary: data.summary,
      status: 'running',
    });
  }
  
  private renderHumanRelayPrompt(data: HumanRelayRequestMessage['data']): void {
    // 显示简洁提示，引导用户查看文件
    tui.showNotification({
      type: 'human-relay',
      title: 'Human Relay Requested',
      message: `Please check: ${data.outputFile}`,
      action: `Input response to: ${data.inputFile}`,
    });
  }
}
```

### 4.4 FunctionalFileHandler

```typescript
/**
 * 功能性文件处理器
 * 与 file-io-prd.md 定义的 Functional IO 交互
 */
class FunctionalFileHandler implements MessageHandler {
  readonly name = 'file_functional';
  readonly target = OutputTarget.FILE_FUNCTIONAL;
  
  private fileIO: FileIOService;  // file-io-prd.md 定义的接口
  
  constructor(fileIO: FileIOService) {
    this.fileIO = fileIO;
  }
  
  supports(message: BaseMessage): boolean {
    // 只处理 Human Relay 请求
    return message.type === MessageType.HUMAN_RELAY_REQUEST;
  }
  
  async handle(message: BaseMessage): Promise<void> {
    if (message.type === MessageType.HUMAN_RELAY_REQUEST) {
      const data = message.data as HumanRelayRequestMessage['data'];
      
      // 写入 human-relay-output.txt（纯文本，符合 file-io-prd）
      await this.fileIO.writeHumanRelayOutput({
        sessionId: message.sessionId,
        content: data.prompt,
      });
      
      // 启动文件监控，等待 input
      this.fileIO.watchHumanRelayInput({
        sessionId: message.sessionId,
        onResponse: (content) => {
          // 触发 HumanRelayResponseMessage
          messageBus.publish({
            type: MessageType.HUMAN_RELAY_RESPONSE,
            data: { requestId: data.requestId, content },
          });
        },
      });
    }
  }
}
```

### 4.5 DisplayFileHandler

```typescript
/**
 * 展示性文件处理器
 * 更新 output.md 聚合文件
 */
class DisplayFileHandler implements MessageHandler {
  readonly name = 'file_display';
  readonly target = OutputTarget.FILE_DISPLAY;
  
  private fileIO: FileIOService;
  private buffer: Map<string, BaseMessage[]> = new Map();
  
  constructor(fileIO: FileIOService) {
    this.fileIO = fileIO;
  }
  
  supports(message: BaseMessage): boolean {
    // 支持需要记录到 display 的消息
    return [
      MessageType.TOOL_RESULT,
      MessageType.WORKFLOW_NODE_START,
      MessageType.WORKFLOW_NODE_END,
      MessageType.CHECKPOINT_CREATE,
      MessageType.AGENT_ITERATION,
    ].includes(message.type);
  }
  
  async handle(message: BaseMessage): Promise<void> {
    // 缓冲消息，批量更新
    const sessionMessages = this.buffer.get(message.sessionId) || [];
    sessionMessages.push(message);
    this.buffer.set(message.sessionId, sessionMessages);
    
    // 批量写入 output.md
    await this.flushSession(message.sessionId);
  }
  
  private async flushSession(sessionId: string): Promise<void> {
    const messages = this.buffer.get(sessionId) || [];
    if (messages.length === 0) return;
    
    // 转换为 output.md 格式
    const sections = messages.map(m => this.toDisplaySection(m));
    
    // 更新 output.md
    await this.fileIO.updateDisplayOutput({
      sessionId,
      sections,
    });
    
    // 清空缓冲
    this.buffer.delete(sessionId);
  }
  
  private toDisplaySection(message: BaseMessage): DisplaySection {
    // 转换消息为 output.md 的 section
    switch (message.type) {
      case MessageType.TOOL_RESULT:
        return {
          type: 'tool_result',
          title: `Tool: ${(message.data as ToolResultMessage['data']).toolName}`,
          content: (message.data as ToolResultMessage['data']).output,
        };
      // ... 其他类型
    }
  }
}
```

---

## 5. 消息总线

### 5.1 总线接口

```typescript
/**
 * 消息总线
 * 统一的消息发布/订阅系统
 */
interface MessageBus {
  /**
   * 发布消息
   */
  publish(message: BaseMessage): void;
  
  /**
   * 订阅消息
   */
  subscribe(
    filter: MessageFilter, 
    handler: (message: BaseMessage) => void
  ): Subscription;
  
  /**
   * 注册处理器
   */
  registerHandler(handler: MessageHandler): void;
  
  /**
   * 注销处理器
   */
  unregisterHandler(name: string): void;
}

/**
 * 消息过滤器
 */
interface MessageFilter {
  types?: MessageType[];
  categories?: MessageCategory[];
  sessionId?: string;
}

/**
 * 订阅句柄
 */
interface Subscription {
  unsubscribe(): void;
}
```

### 5.2 消息总线实现

```typescript
class MessageBusImpl implements MessageBus {
  private handlers: Map<string, MessageHandler> = new Map();
  private decider: OutputDecider;
  private subscribers: Array<{ filter: MessageFilter; handler: Function }> = [];
  
  constructor(decider: OutputDecider) {
    this.decider = decider;
  }
  
  publish(message: BaseMessage): void {
    // 1. 决定输出目标
    const targets = this.decider.decide(message);
    
    // 2. 路由到对应处理器
    for (const target of targets) {
      const handler = this.findHandler(target, message);
      if (handler) {
        handler.handle(message).catch(err => {
          console.error(`Handler ${handler.name} failed:`, err);
        });
      }
    }
    
    // 3. 通知订阅者
    this.notifySubscribers(message);
  }
  
  private findHandler(target: OutputTarget, message: BaseMessage): MessageHandler | undefined {
    for (const handler of this.handlers.values()) {
      if (handler.target === target && handler.supports(message)) {
        return handler;
      }
    }
    return undefined;
  }
  
  private notifySubscribers(message: BaseMessage): void {
    for (const sub of this.subscribers) {
      if (this.matchesFilter(message, sub.filter)) {
        sub.handler(message);
      }
    }
  }
  
  private matchesFilter(message: BaseMessage, filter: MessageFilter): boolean {
    if (filter.types && !filter.types.includes(message.type)) return false;
    if (filter.categories && !filter.categories.includes(message.category)) return false;
    if (filter.sessionId && filter.sessionId !== message.sessionId) return false;
    return true;
  }
  
  registerHandler(handler: MessageHandler): void {
    this.handlers.set(handler.name, handler);
  }
  
  unregisterHandler(name: string): void {
    this.handlers.delete(name);
  }
  
  subscribe(filter: MessageFilter, handler: (message: BaseMessage) => void): Subscription {
    const sub = { filter, handler };
    this.subscribers.push(sub);
    return {
      unsubscribe: () => {
        const idx = this.subscribers.indexOf(sub);
        if (idx > -1) this.subscribers.splice(idx, 1);
      },
    };
  }
}
```

---

## 6. 典型场景流程

### 6.1 Human Relay 场景

```
Agent Loop
    │
    ▼ 触发 Human Relay
┌─────────────────┐
│ 创建消息        │
│ HumanRelayRequestMessage
│ - requestId     │
│ - prompt (完整内容)
│ - outputFile    │
│ - inputFile     │
└────────┬────────┘
         │
         ▼ 发布到 MessageBus
┌─────────────────┐
│ OutputDecider   │
│ 决策: TUI + FILE_FUNCTIONAL
└────────┬────────┘
         │
    ┌────┴────┐
    ▼         ▼
┌────────┐  ┌─────────────────┐
│TUIHandler│  │FunctionalFileHandler│
│        │  │                 │
│显示简洁 │  │ 1. 写入 human-  │
│提示信息 │  │    relay-output.txt │
│        │  │    (纯文本)      │
│"请查看  │  │ 2. 启动文件监控  │
│文件..." │  │    等待 input   │
└────────┘  └─────────────────┘
                     │
                     ▼ 用户保存文件
            ┌─────────────────┐
            │ 触发响应消息     │
            │ HumanRelayResponseMessage
            │ - requestId     │
            │ - content       │
            └─────────────────┘
```

### 6.2 工具调用场景

```
Tool Call
    │
    ▼
┌─────────────────┐
│ ToolCallStartMessage
│ - toolName      │
│ - summary       │
└────────┬────────┘
         │
         ▼ 决策: TUI
    ┌────────────┐
    │ TUIHandler │
    │ 显示摘要   │
    │ [→] file_read │
    └────────────┘
         │
         ▼ 工具执行完成
┌─────────────────┐
│ ToolCallEndMessage
│ - duration      │
└────────┬────────┘
         │
         ▼ 决策: TUI
    ┌────────────┐
    │ TUIHandler │
    │ 更新状态   │
    │ [✓] file_read (0.2s)│
    └────────────┘
         │
         ▼ 工具结果
┌─────────────────┐
│ ToolResultMessage
│ - fullOutput    │
└────────┬────────┘
         │
         ▼ 决策: FILE_DISPLAY
┌─────────────────┐
│ DisplayFileHandler
│ 更新 output.md  │
│ 添加工具结果详情 │
└─────────────────┘
```

---

## 7. 配置

### 7.1 配置文件

```toml
# cli-config.toml

[message_output]
# 默认输出级别
default_level = "info"

[message_output.decider]
# 自定义决策规则
# 规则按优先级排序，先匹配的先应用

[[message_output.decider.rules]]
name = "stream-only-tui"
types = ["agent.stream"]
targets = ["tui"]
priority = 100

[[message_output.decider.rules]]
name = "human-relay-to-file"
types = ["human_relay.request"]
targets = ["tui", "file_functional"]
priority = 100

[[message_output.decider.rules]]
name = "tool-result-to-display"
types = ["tool.result"]
targets = ["file_display"]
priority = 100

[message_output.tui]
# TUI 显示配置
enabled = true
max_history = 100
update_interval_ms = 50

[message_output.file]
# 文件输出配置
enabled = true
# 与 file-io-prd.md 的配置保持一致
base_dir = ".wf-agent"
```

---

## 8. 与 File IO 系统的集成

### 8.1 集成接口

```typescript
/**
 * File IO 服务接口
 * 由 file-io-prd.md 的实现提供
 */
interface FileIOService {
  /**
   * 写入 Human Relay 输出文件
   */
  writeHumanRelayOutput(params: {
    sessionId: string;
    content: string;
  }): Promise<void>;
  
  /**
   * 监控 Human Relay 输入文件
   */
  watchHumanRelayInput(params: {
    sessionId: string;
    onResponse: (content: string) => void;
    timeout?: number;
  }): void;
  
  /**
   * 更新 Display 输出文件
   */
  updateDisplayOutput(params: {
    sessionId: string;
    sections: DisplaySection[];
  }): Promise<void>;
  
  /**
   * 获取文件路径
   */
  getPaths(sessionId: string): {
    functional: {
      humanRelayOutput: string;
      humanRelayInput: string;
    };
    display: {
      output: string;
    };
  };
}
```

### 8.2 初始化流程

```typescript
// 初始化消息输出系统
function initializeMessageOutput(fileIO: FileIOService): MessageBus {
  // 1. 创建决策器
  const decider = new OutputDeciderImpl(DEFAULT_DECISION_RULES);
  
  // 2. 创建消息总线
  const bus = new MessageBusImpl(decider);
  
  // 3. 注册处理器
  bus.registerHandler(new TUIHandler());
  bus.registerHandler(new FunctionalFileHandler(fileIO));
  bus.registerHandler(new DisplayFileHandler(fileIO));
  bus.registerHandler(new LogHandler());
  
  return bus;
}
```

---

## 9. 实现建议

### 9.1 目录结构

```
src/
├── message/
│   ├── types.ts              # 消息类型定义
│   ├── bus.ts                # 消息总线实现
│   ├── decider.ts            # 输出决策器
│   ├── handlers/
│   │   ├── tui-handler.ts    # TUI 处理器
│   │   ├── functional-file-handler.ts
│   │   ├── display-file-handler.ts
│   │   └── log-handler.ts
│   └── config.ts             # 配置管理
├── index.ts                  # 导出和初始化
└── ...
```

### 9.2 关键实现点

1. **异步处理** - 消息处理是异步的，避免阻塞 Agent Loop
2. **错误隔离** - 单个处理器失败不影响其他处理器
3. **缓冲机制** - DisplayFileHandler 需要缓冲和批量写入
4. **会话隔离** - 所有消息必须携带 sessionId，确保正确路由
5. **与 File IO 解耦** - 通过接口依赖，不直接操作文件系统

---

## 10. 迁移路径

### 阶段一：基础框架
- 实现消息类型定义
- 实现消息总线和决策器
- 实现 TUIHandler

### 阶段二：文件集成
- 集成 File IO 系统
- 实现 FunctionalFileHandler
- 实现 DisplayFileHandler

### 阶段三：完善配置
- 实现配置系统
- 支持自定义决策规则
- 文档完善
