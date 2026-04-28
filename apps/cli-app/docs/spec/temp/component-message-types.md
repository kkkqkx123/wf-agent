# 组件消息类型规范

## 一、概述

本文档定义 CLI-App 中所有组件的消息类型规范，提供统一的消息分类体系，支持可配置的输出模式控制。

---

## 二、消息类型层级

### 2.1 层级结构

```
Message (基础消息)
├── SystemMessage (系统消息)
│   ├── StartupMessage
│   ├── ShutdownMessage
│   ├── ConfigChangeMessage
│   └── ErrorMessage
│
├── AgentMessage (Agent 消息)
│   ├── AgentLifecycleMessage
│   │   ├── AgentStartMessage
│   │   ├── AgentPauseMessage
│   │   ├── AgentResumeMessage
│   │   └── AgentEndMessage
│   ├── AgentIterationMessage
│   ├── AgentStreamMessage
│   └── AgentErrorMessage
│
├── ToolMessage (工具消息)
│   ├── ToolCallStartMessage
│   ├── ToolCallEndMessage
│   ├── ToolResultMessage
│   └── ToolErrorMessage
│
├── HumanRelayMessage (Human Relay 消息)
│   ├── HumanRelayRequestMessage
│   ├── HumanRelayResponseMessage
│   ├── HumanRelayTimeoutMessage
│   └── HumanRelayCancelMessage
│
├── WorkflowMessage (工作流消息)
│   ├── WorkflowLifecycleMessage
│   │   ├── WorkflowStartMessage
│   │   └── WorkflowEndMessage
│   ├── WorkflowNodeMessage
│   │   ├── NodeStartMessage
│   │   ├── NodeEndMessage
│   │   └── NodeErrorMessage
│   └── WorkflowCheckpointMessage
│
├── ThreadMessage (线程消息)
│   ├── ThreadStartMessage
│   ├── ThreadStatusMessage
│   ├── ThreadPauseMessage
│   ├── ThreadResumeMessage
│   └── ThreadEndMessage
│
├── CheckpointMessage (检查点消息)
│   ├── CheckpointCreateMessage
│   ├── CheckpointRestoreMessage
│   └── CheckpointDeleteMessage
│
└── EventMessage (事件消息)
    ├── EventTriggerMessage
    ├── EventProcessMessage
    └── EventCompleteMessage
```

---

## 三、消息类型定义

### 3.1 基础消息接口

```typescript
/**
 * 消息优先级
 */
enum MessagePriority {
  CRITICAL = 0,   // 关键消息，必须显示
  HIGH = 1,       // 高优先级
  NORMAL = 2,     // 普通优先级
  LOW = 3,        // 低优先级
  DEBUG = 4,      // 调试信息
}

/**
 * 消息输出目标
 */
enum MessageOutputTarget {
  TUI = 'tui',           // TUI 显示
  FILE = 'file',         // 文件输出
  LOG = 'log',           // 日志记录
  ALL = 'all',           // 所有目标
  NONE = 'none',         // 不输出
}

/**
 * 基础消息接口
 */
interface BaseMessage {
  /** 消息唯一标识 */
  readonly id: string;
  
  /** 消息类型 */
  readonly type: MessageType;
  
  /** 消息类别 */
  readonly category: MessageCategory;
  
  /** 时间戳 */
  readonly timestamp: number;
  
  /** 消息级别 */
  readonly level: 'debug' | 'info' | 'warn' | 'error' | 'critical';
  
  /** 优先级 */
  readonly priority: MessagePriority;
  
  /** 默认输出目标 */
  readonly defaultTarget: MessageOutputTarget;
  
  /** 所属会话 ID */
  readonly sessionId?: string;
  
  /** 来源组件 */
  readonly source: string;
  
  /** 消息数据 */
  readonly data: unknown;
  
  /** 元数据 */
  readonly meta?: Record<string, unknown>;
}

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
 * 消息类型枚举
 */
enum MessageType {
  // 系统消息 (1000-1099)
  SYSTEM_STARTUP = 'system.startup',
  SYSTEM_SHUTDOWN = 'system.shutdown',
  SYSTEM_CONFIG_CHANGE = 'system.config_change',
  SYSTEM_ERROR = 'system.error',
  
  // Agent 消息 (2000-2099)
  AGENT_START = 'agent.start',
  AGENT_PAUSE = 'agent.pause',
  AGENT_RESUME = 'agent.resume',
  AGENT_END = 'agent.end',
  AGENT_ITERATION = 'agent.iteration',
  AGENT_STREAM = 'agent.stream',
  AGENT_ERROR = 'agent.error',
  
  // 工具消息 (3000-3099)
  TOOL_CALL_START = 'tool.call_start',
  TOOL_CALL_END = 'tool.call_end',
  TOOL_RESULT = 'tool.result',
  TOOL_ERROR = 'tool.error',
  
  // Human Relay 消息 (4000-4099)
  HUMAN_RELAY_REQUEST = 'human_relay.request',
  HUMAN_RELAY_RESPONSE = 'human_relay.response',
  HUMAN_RELAY_TIMEOUT = 'human_relay.timeout',
  HUMAN_RELAY_CANCEL = 'human_relay.cancel',
  
  // 工作流消息 (5000-5099)
  WORKFLOW_START = 'workflow.start',
  WORKFLOW_END = 'workflow.end',
  WORKFLOW_NODE_START = 'workflow.node_start',
  WORKFLOW_NODE_END = 'workflow.node_end',
  WORKFLOW_NODE_ERROR = 'workflow.node_error',
  WORKFLOW_CHECKPOINT = 'workflow.checkpoint',
  
  // 线程消息 (6000-6099)
  THREAD_START = 'thread.start',
  THREAD_STATUS = 'thread.status',
  THREAD_PAUSE = 'thread.pause',
  THREAD_RESUME = 'thread.resume',
  THREAD_END = 'thread.end',
  
  // 检查点消息 (7000-7099)
  CHECKPOINT_CREATE = 'checkpoint.create',
  CHECKPOINT_RESTORE = 'checkpoint.restore',
  CHECKPOINT_DELETE = 'checkpoint.delete',
  
  // 事件消息 (8000-8099)
  EVENT_TRIGGER = 'event.trigger',
  EVENT_PROCESS = 'event.process',
  EVENT_COMPLETE = 'event.complete',
}
```

---

## 四、详细消息定义

### 4.1 系统消息

#### StartupMessage

```typescript
interface StartupMessage extends BaseMessage {
  type: MessageType.SYSTEM_STARTUP;
  category: MessageCategory.SYSTEM;
  level: 'info';
  priority: MessagePriority.NORMAL;
  defaultTarget: MessageOutputTarget.ALL;
  data: {
    version: string;
    configPath: string;
    workingDirectory: string;
    environment: 'development' | 'production' | 'test';
    features: string[];
  };
}
```

#### ErrorMessage

```typescript
interface ErrorMessage extends BaseMessage {
  type: MessageType.SYSTEM_ERROR;
  category: MessageCategory.SYSTEM;
  level: 'error' | 'critical';
  priority: MessagePriority.CRITICAL;
  defaultTarget: MessageOutputTarget.ALL;
  data: {
    error: {
      name: string;
      message: string;
      stack?: string;
    };
    context?: Record<string, unknown>;
    recoverable: boolean;
  };
}
```

### 4.2 Agent 消息

#### AgentStartMessage

```typescript
interface AgentStartMessage extends BaseMessage {
  type: MessageType.AGENT_START;
  category: MessageCategory.AGENT;
  level: 'info';
  priority: MessagePriority.HIGH;
  defaultTarget: MessageOutputTarget.ALL;
  data: {
    agentId: string;
    profileId: string;
    config: {
      maxIterations: number;
      tools: string[];
      systemPrompt?: string;
    };
    context?: {
      workflowId?: string;
      threadId?: string;
    };
  };
}
```

#### AgentIterationMessage

```typescript
interface AgentIterationMessage extends BaseMessage {
  type: MessageType.AGENT_ITERATION;
  category: MessageCategory.AGENT;
  level: 'info';
  priority: MessagePriority.NORMAL;
  defaultTarget: MessageOutputTarget.TUI;
  data: {
    iteration: number;
    maxIterations: number;
    toolCallCount: number;
    messageCount: number;
    duration: number;  // 本次迭代耗时
    status: 'running' | 'waiting' | 'error';
  };
}
```

#### AgentStreamMessage

```typescript
interface AgentStreamMessage extends BaseMessage {
  type: MessageType.AGENT_STREAM;
  category: MessageCategory.AGENT;
  level: 'info';
  priority: MessagePriority.NORMAL;
  defaultTarget: MessageOutputTarget.TUI;
  data: {
    chunk: string;
    isComplete: boolean;
    messageId: string;
  };
}
```

#### AgentEndMessage

```typescript
interface AgentEndMessage extends BaseMessage {
  type: MessageType.AGENT_END;
  category: MessageCategory.AGENT;
  level: 'info';
  priority: MessagePriority.HIGH;
  defaultTarget: MessageOutputTarget.ALL;
  data: {
    agentId: string;
    success: boolean;
    iterations: number;
    toolCallCount: number;
    duration: number;
    error?: {
      name: string;
      message: string;
    };
  };
}
```

### 4.3 工具消息

#### ToolCallStartMessage

```typescript
interface ToolCallStartMessage extends BaseMessage {
  type: MessageType.TOOL_CALL_START;
  category: MessageCategory.TOOL;
  level: 'info';
  priority: MessagePriority.NORMAL;
  defaultTarget: MessageOutputTarget.TUI;
  data: {
    toolCallId: string;
    toolName: string;
    arguments: Record<string, unknown>;
    /** 摘要信息（用于 TUI 显示） */
    summary: string;
  };
}
```

#### ToolCallEndMessage

```typescript
interface ToolCallEndMessage extends BaseMessage {
  type: MessageType.TOOL_CALL_END;
  category: MessageCategory.TOOL;
  level: 'info';
  priority: MessagePriority.NORMAL;
  defaultTarget: MessageOutputTarget.TUI;
  data: {
    toolCallId: string;
    toolName: string;
    success: boolean;
    duration: number;
    /** 摘要信息（用于 TUI 显示） */
    summary: string;
  };
}
```

#### ToolResultMessage

```typescript
interface ToolResultMessage extends BaseMessage {
  type: MessageType.TOOL_RESULT;
  category: MessageCategory.TOOL;
  level: 'info';
  priority: MessagePriority.LOW;
  defaultTarget: MessageOutputTarget.FILE;
  data: {
    toolCallId: string;
    toolName: string;
    result: unknown;
    /** 完整输出（写入文件） */
    fullOutput: string;
    /** 摘要（用于日志） */
    summary: string;
  };
}
```

#### ToolErrorMessage

```typescript
interface ToolErrorMessage extends BaseMessage {
  type: MessageType.TOOL_ERROR;
  category: MessageCategory.TOOL;
  level: 'error';
  priority: MessagePriority.HIGH;
  defaultTarget: MessageOutputTarget.ALL;
  data: {
    toolCallId: string;
    toolName: string;
    error: {
      name: string;
      message: string;
      stack?: string;
    };
    arguments: Record<string, unknown>;
  };
}
```

### 4.4 Human Relay 消息

#### HumanRelayRequestMessage

```typescript
interface HumanRelayRequestMessage extends BaseMessage {
  type: MessageType.HUMAN_RELAY_REQUEST;
  category: MessageCategory.HUMAN_RELAY;
  level: 'info';
  priority: MessagePriority.HIGH;
  defaultTarget: MessageOutputTarget.ALL;
  data: {
    requestId: string;
    prompt: string;
    context: {
      messages: Array<{
        role: 'user' | 'assistant' | 'system';
        content: string;
        timestamp: number;
      }>;
      metadata?: Record<string, unknown>;
    };
    timeout: number;
    /** 输出文件路径 */
    outputFile: string;
  };
}
```

#### HumanRelayResponseMessage

```typescript
interface HumanRelayResponseMessage extends BaseMessage {
  type: MessageType.HUMAN_RELAY_RESPONSE;
  category: MessageCategory.HUMAN_RELAY;
  level: 'info';
  priority: MessagePriority.HIGH;
  defaultTarget: MessageOutputTarget.ALL;
  data: {
    requestId: string;
    content: string;
    responseTime: number;
    inputFile: string;
  };
}
```

#### HumanRelayTimeoutMessage

```typescript
interface HumanRelayTimeoutMessage extends BaseMessage {
  type: MessageType.HUMAN_RELAY_TIMEOUT;
  category: MessageCategory.HUMAN_RELAY;
  level: 'warn';
  priority: MessagePriority.HIGH;
  defaultTarget: MessageOutputTarget.ALL;
  data: {
    requestId: string;
    timeout: number;
    elapsed: number;
  };
}
```

### 4.5 工作流消息

#### WorkflowStartMessage

```typescript
interface WorkflowStartMessage extends BaseMessage {
  type: MessageType.WORKFLOW_START;
  category: MessageCategory.WORKFLOW;
  level: 'info';
  priority: MessagePriority.HIGH;
  defaultTarget: MessageOutputTarget.ALL;
  data: {
    workflowId: string;
    workflowName: string;
    version: string;
    input: Record<string, unknown>;
    threadId: string;
  };
}
```

#### WorkflowNodeMessage

```typescript
interface WorkflowNodeMessage extends BaseMessage {
  type: MessageType.WORKFLOW_NODE_START | MessageType.WORKFLOW_NODE_END;
  category: MessageCategory.WORKFLOW;
  level: 'info';
  priority: MessagePriority.NORMAL;
  defaultTarget: MessageOutputTarget.TUI;
  data: {
    workflowId: string;
    threadId: string;
    nodeId: string;
    nodeType: string;
    nodeName: string;
    status: 'running' | 'completed' | 'error' | 'skipped';
    duration?: number;
    input?: Record<string, unknown>;
    output?: Record<string, unknown>;
  };
}
```

#### WorkflowCheckpointMessage

```typescript
interface WorkflowCheckpointMessage extends BaseMessage {
  type: MessageType.WORKFLOW_CHECKPOINT;
  category: MessageCategory.WORKFLOW;
  level: 'info';
  priority: MessagePriority.NORMAL;
  defaultTarget: MessageOutputTarget.LOG;
  data: {
    workflowId: string;
    threadId: string;
    checkpointId: string;
    nodeId: string;
    state: Record<string, unknown>;
    timestamp: number;
  };
}
```

### 4.6 线程消息

#### ThreadStatusMessage

```typescript
interface ThreadStatusMessage extends BaseMessage {
  type: MessageType.THREAD_STATUS;
  category: MessageCategory.THREAD;
  level: 'info';
  priority: MessagePriority.NORMAL;
  defaultTarget: MessageOutputTarget.TUI;
  data: {
    threadId: string;
    workflowId: string;
    status: 'pending' | 'running' | 'paused' | 'completed' | 'error' | 'cancelled';
    progress: number;  // 0-100
    currentNode?: string;
    startedAt: number;
    updatedAt: number;
    estimatedEndAt?: number;
  };
}
```

### 4.7 检查点消息

#### CheckpointCreateMessage

```typescript
interface CheckpointCreateMessage extends BaseMessage {
  type: MessageType.CHECKPOINT_CREATE;
  category: MessageCategory.CHECKPOINT;
  level: 'info';
  priority: MessagePriority.LOW;
  defaultTarget: MessageOutputTarget.LOG;
  data: {
    checkpointId: string;
    entityType: 'agent' | 'workflow' | 'thread';
    entityId: string;
    nodeId?: string;
    state: Record<string, unknown>;
    metadata?: Record<string, unknown>;
  };
}
```

---

## 五、消息输出配置

### 5.1 配置结构

```typescript
/**
 * 消息输出配置
 */
interface MessageOutputConfig {
  /** 全局默认配置 */
  defaults: {
    target: MessageOutputTarget;
    minLevel: 'debug' | 'info' | 'warn' | 'error';
    minPriority: MessagePriority;
  };
  
  /** 按类别配置 */
  categories?: Partial<Record<MessageCategory, CategoryConfig>>;
  
  /** 按类型配置 */
  types?: Partial<Record<MessageType, TypeConfig>>;
  
  /** 文件输出配置 */
  file?: FileOutputConfig;
  
  /** TUI 配置 */
  tui?: TUIOutputConfig;
  
  /** 日志配置 */
  log?: LogOutputConfig;
}

/**
 * 类别配置
 */
interface CategoryConfig {
  target?: MessageOutputTarget;
  minLevel?: 'debug' | 'info' | 'warn' | 'error';
  minPriority?: MessagePriority;
  enabled?: boolean;
}

/**
 * 类型配置
 */
interface TypeConfig extends CategoryConfig {
  /** 自定义处理函数 */
  handler?: string;
  /** 格式化模板 */
  template?: string;
}

/**
 * 文件输出配置
 */
interface FileOutputConfig {
  enabled: boolean;
  outputDir: string;
  files: {
    [key: string]: {
      path: string;
      messageTypes: MessageType[];
      rotation?: {
        enabled: boolean;
        maxSize: number;
        maxFiles: number;
      };
    };
  };
}

/**
 * TUI 输出配置
 */
interface TUIOutputConfig {
  enabled: boolean;
  maxHistory: number;
  updateInterval: number;
  categories: MessageCategory[];
  excludeTypes?: MessageType[];
}

/**
 * 日志输出配置
 */
interface LogOutputConfig {
  enabled: boolean;
  level: 'debug' | 'info' | 'warn' | 'error';
  format: 'json' | 'text';
}
```

### 5.2 默认配置

```typescript
const DEFAULT_MESSAGE_CONFIG: MessageOutputConfig = {
  defaults: {
    target: MessageOutputTarget.ALL,
    minLevel: 'info',
    minPriority: MessagePriority.NORMAL,
  },
  categories: {
    [MessageCategory.SYSTEM]: {
      target: MessageOutputTarget.ALL,
      minLevel: 'info',
    },
    [MessageCategory.AGENT]: {
      target: MessageOutputTarget.ALL,
      minLevel: 'info',
    },
    [MessageCategory.TOOL]: {
      target: MessageOutputTarget.TUI,
      minLevel: 'info',
    },
    [MessageCategory.HUMAN_RELAY]: {
      target: MessageOutputTarget.ALL,
      minLevel: 'info',
    },
    [MessageCategory.WORKFLOW]: {
      target: MessageOutputTarget.ALL,
      minLevel: 'info',
    },
    [MessageCategory.THREAD]: {
      target: MessageOutputTarget.TUI,
      minLevel: 'info',
    },
    [MessageCategory.CHECKPOINT]: {
      target: MessageOutputTarget.LOG,
      minLevel: 'debug',
    },
    [MessageCategory.EVENT]: {
      target: MessageOutputTarget.LOG,
      minLevel: 'info',
    },
  },
  types: {
    [MessageType.TOOL_RESULT]: {
      target: MessageOutputTarget.FILE,
      minLevel: 'info',
    },
    [MessageType.AGENT_STREAM]: {
      target: MessageOutputTarget.TUI,
      minLevel: 'info',
    },
  },
  file: {
    enabled: true,
    outputDir: './wf-agent/output',
    files: {
      agent: {
        path: 'agent-output.txt',
        messageTypes: [
          MessageType.AGENT_START,
          MessageType.AGENT_ITERATION,
          MessageType.AGENT_END,
          MessageType.AGENT_ERROR,
        ],
      },
      tool: {
        path: 'tool-calls.log',
        messageTypes: [
          MessageType.TOOL_CALL_START,
          MessageType.TOOL_CALL_END,
          MessageType.TOOL_RESULT,
          MessageType.TOOL_ERROR,
        ],
        rotation: {
          enabled: true,
          maxSize: 10 * 1024 * 1024,
          maxFiles: 5,
        },
      },
      humanRelay: {
        path: 'human-relay-output.txt',
        messageTypes: [
          MessageType.HUMAN_RELAY_REQUEST,
          MessageType.HUMAN_RELAY_RESPONSE,
        ],
      },
    },
  },
  tui: {
    enabled: true,
    maxHistory: 100,
    updateInterval: 100,
    categories: [
      MessageCategory.SYSTEM,
      MessageCategory.AGENT,
      MessageCategory.TOOL,
      MessageCategory.HUMAN_RELAY,
      MessageCategory.WORKFLOW,
      MessageCategory.THREAD,
    ],
    excludeTypes: [
      MessageType.TOOL_RESULT,
      MessageType.CHECKPOINT_CREATE,
    ],
  },
  log: {
    enabled: true,
    level: 'info',
    format: 'json',
  },
};
```

---

## 六、消息处理器

### 6.1 处理器接口

```typescript
/**
 * 消息处理器接口
 */
interface MessageHandler {
  /**
   * 处理器名称
   */
  readonly name: string;
  
  /**
   * 支持的消息类型
   */
  readonly supportedTypes: MessageType[];
  
  /**
   * 处理消息
   */
  handle(message: BaseMessage): Promise<void> | void;
  
  /**
   * 刷新缓冲区
   */
  flush?(): Promise<void>;
  
  /**
   * 关闭处理器
   */
  close?(): Promise<void>;
}

/**
 * 消息路由器
 */
interface MessageRouter {
  /**
   * 注册处理器
   */
  register(handler: MessageHandler): void;
  
  /**
   * 注销处理器
   */
  unregister(name: string): void;
  
  /**
   * 路由消息
   */
  route(message: BaseMessage): Promise<void>;
  
  /**
   * 配置路由规则
   */
  configure(config: MessageOutputConfig): void;
}
```

### 6.2 内置处理器

| 处理器 | 说明 | 输出目标 |
|--------|------|----------|
| TUIHandler | TUI 显示处理器 | 终端 TUI |
| FileHandler | 文件输出处理器 | 输出文件 |
| LogHandler | 日志记录处理器 | 日志文件 |
| FilterHandler | 消息过滤处理器 | - |
| BufferHandler | 缓冲批处理器 | - |

---

## 七、使用示例

### 7.1 发送消息

```typescript
import { MessageFactory, MessageBus } from './message';

// 创建消息
const message = MessageFactory.createAgentStart({
  agentId: 'agent-001',
  profileId: 'default',
  config: {
    maxIterations: 10,
    tools: ['file_read', 'code_search'],
  },
});

// 发布消息
MessageBus.publish(message);
```

### 7.2 配置输出

```typescript
import { MessageRouter, DEFAULT_MESSAGE_CONFIG } from './message';

const router = new MessageRouter();

// 使用默认配置
router.configure(DEFAULT_MESSAGE_CONFIG);

// 自定义配置
router.configure({
  ...DEFAULT_MESSAGE_CONFIG,
  types: {
    [MessageType.TOOL_CALL_START]: {
      target: MessageOutputTarget.TUI,
    },
    [MessageType.TOOL_RESULT]: {
      target: MessageOutputTarget.FILE,
    },
  },
});
```

### 7.3 自定义处理器

```typescript
import { MessageHandler, BaseMessage, MessageType } from './message';

class CustomHandler implements MessageHandler {
  readonly name = 'custom';
  readonly supportedTypes = [MessageType.AGENT_END];
  
  handle(message: BaseMessage): void {
    if (message.type === MessageType.AGENT_END) {
      const data = message.data as AgentEndMessage['data'];
      console.log(`Agent ${data.agentId} completed in ${data.duration}ms`);
    }
  }
}

// 注册处理器
router.register(new CustomHandler());
```

---

## 八、消息 ID 生成

### 8.1 ID 格式

消息 ID 采用 `{prefix}-{timestamp}-{random}` 格式：

```typescript
function generateMessageId(prefix: string): string {
  const timestamp = Date.now().toString(36);
  const random = Math.random().toString(36).substring(2, 8);
  return `${prefix}-${timestamp}-${random}`;
}

// 示例
// msg-lxqz2n-abc123
// agent-lxqz2o-def456
// tool-lxqz2p-ghi789
```

### 8.2 前缀规范

| 消息类别 | 前缀 |
|----------|------|
| 系统 | sys |
| Agent | agent |
| 工具 | tool |
| Human Relay | hr |
| 工作流 | wf |
| 线程 | thread |
| 检查点 | cp |
| 事件 | evt |

---

## 九、版本兼容性

### 9.1 版本号

消息类型规范版本号格式：`{major}.{minor}.{patch}`

- **major**: 不兼容的结构性变更
- **minor**: 新增消息类型，向后兼容
- **patch**: 文档修正，无代码变更

### 9.2 当前版本

**Version: 1.0.0**

### 9.3 变更日志

| 版本 | 日期 | 变更内容 |
|------|------|----------|
| 1.0.0 | 2024-01-15 | 初始版本 |
