# 消息类型迁移规范

## 1. 迁移概述

### 1.1 迁移目标

将 CLI-App 的消息类型定义迁移到 SDK 层，使其成为通用类型，供所有应用使用：

```
迁移前 (CLI-App 特有):
apps/cli-app/docs/spec/message-output-prd.md
apps/cli-app/docs/spec/wf-agent-message-classification.md

迁移后 (SDK 通用):
packages/types/src/messaging/     # 基础消息类型
sdk/api/shared/messaging/         # API 层消息接口
sdk/core/messaging/               # 核心消息总线实现
```

### 1.2 职责分离

| 层级 | 职责 | 示例 |
|------|------|------|
| **packages/types** | 纯类型定义 | 消息接口、枚举、数据结构 |
| **sdk/api** | API 接口 | 消息发送/订阅 API |
| **sdk/core** | 核心实现 | 消息总线、路由、处理器 |
| **cli-app** | 应用特定 | File IO 处理器、TUI 渲染 |

---

## 2. 现有事件类型分析

### 2.1 当前 SDK 事件体系

```
packages/types/src/events/
├── base.ts                 # EventType 枚举 (基础事件)
├── thread-events.ts        # Thread 相关事件
├── node-events.ts          # Node 相关事件
├── agent-events.ts         # Agent 相关事件
├── tool-events.ts          # Tool 相关事件
├── conversation-events.ts  # 对话相关事件
├── checkpoint-events.ts    # 检查点事件
├── subgraph-events.ts      # Subgraph 事件
├── interaction-events.ts   # 用户交互事件
├── system-events.ts        # 系统事件
└── skill-events.ts         # Skill 事件
```

### 2.2 现有事件类型示例

```typescript
// packages/types/src/events/base.ts
export type EventType =
  | "THREAD_STARTED"
  | "THREAD_COMPLETED"
  | "THREAD_FAILED"
  | "THREAD_PAUSED"
  | "THREAD_RESUMED"
  | "NODE_STARTED"
  | "NODE_COMPLETED"
  | "TOOL_CALL_STARTED"
  | "TOOL_CALL_COMPLETED"
  | "HUMAN_RELAY_REQUESTED"
  | "HUMAN_RELAY_RESPONDED"
  // ... 更多
```

### 2.3 问题分析

| 问题 | 说明 |
|------|------|
| 命名不一致 | 有些用过去式 (STARTED)，有些用现在式 |
| 缺少输出目标信息 | 事件不携带应该输出到哪里的信息 |
| 缺少实体层级 | 不区分 Thread/Agent/Subgraph 的层级关系 |
| 与消息系统脱节 | 事件和消息是两个独立的概念 |

---

## 3. 新消息类型体系

### 3.1 目录结构

```
packages/types/src/
├── events/                     # 保持现有事件（向后兼容）
│   └── ...
│
└── messaging/                  # 新增：消息类型体系
    ├── index.ts                # 统一导出
    ├── base.ts                 # 基础消息类型
    ├── entity.ts               # 实体标识
    ├── routing.ts              # 路由相关类型
    ├── categories/
    │   ├── index.ts
    │   ├── system.ts           # 系统消息
    │   ├── thread.ts           # Thread 消息
    │   ├── agent.ts            # Agent 消息
    │   ├── tool.ts             # Tool 消息
    │   ├── human-relay.ts      # Human Relay 消息
    │   └── subgraph.ts         # Subgraph 消息
    └── output.ts               # 输出目标类型
```

### 3.2 基础消息类型

```typescript
// packages/types/src/messaging/base.ts

/**
 * 消息 ID 生成策略
 * 格式: {category}:{entityType}:{entityId}:{sequence}:{timestamp}
 * 示例: agent:loop:loop-001:42:1705312345678
 */
export type MessageId = string;

/**
 * 消息类别 - 8大类别
 */
export enum MessageCategory {
  SYSTEM = 'system',
  THREAD = 'thread',
  AGENT = 'agent',
  TOOL = 'tool',
  HUMAN_RELAY = 'human_relay',
  SUBGRAPH = 'subgraph',
  CHECKPOINT = 'checkpoint',
  EVENT = 'event',
}

/**
 * 基础消息接口
 * 所有消息必须实现此接口
 */
export interface BaseMessage {
  /** 消息唯一标识 */
  readonly id: MessageId;
  
  /** 消息类别 */
  readonly category: MessageCategory;
  
  /** 消息类型（各类别内定义） */
  readonly type: string;
  
  /** 时间戳 */
  readonly timestamp: number;
  
  /** 消息级别 */
  readonly level: 'debug' | 'info' | 'warn' | 'error' | 'critical';
  
  /** 实体标识 */
  readonly entity: EntityIdentity;
  
  /** 消息数据 */
  readonly data: unknown;
  
  /** 追踪信息 */
  readonly trace?: MessageTrace;
}

/**
 * 实体标识
 */
export interface EntityIdentity {
  /** 实体类型 */
  type: 'thread' | 'agent' | 'subgraph';
  
  /** 实体实例 ID */
  id: string;
  
  /** 父实体 ID（如果被调用） */
  parentId?: string;
  
  /** 根实体 ID（最顶层） */
  rootId: string;
  
  /** 嵌套深度 */
  depth: number;
  
  /** 并行组信息 */
  parallelGroup?: {
    groupId: string;
    branchIndex: number;
    totalBranches: number;
  };
}

/**
 * 消息追踪信息
 */
export interface MessageTrace {
  /** 调用链 */
  chain: string[];
  
  /** 序列号（用于排序） */
  sequence: number;
  
  /** 关联消息 ID */
  correlationId?: string;
}
```

### 3.3 各类别消息定义

```typescript
// packages/types/src/messaging/categories/agent.ts

/**
 * Agent 消息类型
 */
export enum AgentMessageType {
  // 生命周期
  START = 'agent.start',
  PAUSE = 'agent.pause',
  RESUME = 'agent.resume',
  END = 'agent.end',
  CANCEL = 'agent.cancel',
  
  // 迭代
  ITERATION_START = 'agent.iteration.start',
  ITERATION_END = 'agent.iteration.end',
  ITERATION_LIMIT = 'agent.iteration.limit',
  
  // LLM 交互
  LLM_REQUEST = 'agent.llm.request',
  LLM_STREAM = 'agent.llm.stream',
  LLM_RESPONSE = 'agent.llm.response',
  LLM_ERROR = 'agent.llm.error',
  
  // 工具
  TOOL_CALL_START = 'agent.tool.call_start',
  TOOL_CALL_END = 'agent.tool.call_end',
  TOOL_RESULT = 'agent.tool.result',
  TOOL_ERROR = 'agent.tool.error',
  
  // Human Relay
  HUMAN_RELAY_REQUEST = 'agent.human_relay.request',
  HUMAN_RELAY_RESPONSE = 'agent.human_relay.response',
  HUMAN_RELAY_TIMEOUT = 'agent.human_relay.timeout',
  HUMAN_RELAY_CANCEL = 'agent.human_relay.cancel',
  
  // 检查点
  CHECKPOINT_CREATE = 'agent.checkpoint.create',
  CHECKPOINT_RESTORE = 'agent.checkpoint.restore',
  
  // 消息
  MESSAGE_ADD = 'agent.message.add',
}

/**
 * Agent 消息数据接口
 */
export interface AgentStartData {
  loopId: string;
  agentId: string;
  config: {
    maxIterations: number;
    tools: string[];
    systemPrompt?: string;
  };
}

export interface AgentIterationData {
  iteration: number;
  maxIterations: number;
  toolCallCount: number;
  messageCount: number;
  status: 'running' | 'waiting' | 'error';
}

export interface AgentLLMStreamData {
  chunk: string;
  isComplete: boolean;
  messageId: string;
}

export interface AgentToolCallData {
  toolCallId: string;
  toolName: string;
  arguments: Record<string, unknown>;
  summary: string;
}

export interface AgentToolResultData {
  toolCallId: string;
  toolName: string;
  result: unknown;
  output: string;
  summary: string;
}

export interface AgentHumanRelayRequestData {
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
}

export interface AgentHumanRelayResponseData {
  requestId: string;
  content: string;
  responseTime: number;
}

/**
 * Agent 消息联合类型
 */
export type AgentMessage = 
  | BaseMessage & { type: AgentMessageType.START; data: AgentStartData }
  | BaseMessage & { type: AgentMessageType.ITERATION_START; data: AgentIterationData }
  | BaseMessage & { type: AgentMessageType.LLM_STREAM; data: AgentLLMStreamData }
  | BaseMessage & { type: AgentMessageType.TOOL_CALL_START; data: AgentToolCallData }
  | BaseMessage & { type: AgentMessageType.TOOL_RESULT; data: AgentToolResultData }
  | BaseMessage & { type: AgentMessageType.HUMAN_RELAY_REQUEST; data: AgentHumanRelayRequestData }
  | BaseMessage & { type: AgentMessageType.HUMAN_RELAY_RESPONSE; data: AgentHumanRelayResponseData }
  // ... 其他类型
  ;
```

```typescript
// packages/types/src/messaging/categories/thread.ts

/**
 * Thread 消息类型
 */
export enum ThreadMessageType {
  // 生命周期
  START = 'thread.start',
  PAUSE = 'thread.pause',
  RESUME = 'thread.resume',
  END = 'thread.end',
  CANCEL = 'thread.cancel',
  
  // 节点
  NODE_START = 'thread.node.start',
  NODE_END = 'thread.node.end',
  NODE_ERROR = 'thread.node.error',
  NODE_SKIP = 'thread.node.skip',
  
  // 工作流
  WORKFLOW_START = 'thread.workflow.start',
  WORKFLOW_END = 'thread.workflow.end',
  WORKFLOW_CHECKPOINT = 'thread.workflow.checkpoint',
  
  // 变量
  VARIABLE_SET = 'thread.variable.set',
  VARIABLE_GET = 'thread.variable.get',
  
  // 并行
  FORK_START = 'thread.fork.start',
  FORK_BRANCH_START = 'thread.fork.branch_start',
  FORK_BRANCH_END = 'thread.fork.branch_end',
  JOIN_WAIT = 'thread.join.wait',
  JOIN_COMPLETE = 'thread.join.complete',
  
  // Agent 节点
  AGENT_CALL = 'thread.agent.call',
  AGENT_RETURN = 'thread.agent.return',
}

/**
 * Thread 消息数据接口
 */
export interface ThreadNodeData {
  threadId: string;
  graphId: string;
  nodeId: string;
  nodeType: string;
  status: 'running' | 'completed' | 'error' | 'skipped';
  duration?: number;
  input?: Record<string, unknown>;
  output?: Record<string, unknown>;
}

export interface ThreadAgentCallData {
  threadId: string;
  nodeId: string;
  targetLoopId: string;
  config: {
    profileId: string;
    maxIterations: number;
  };
}

// ... 其他数据接口
```

```typescript
// packages/types/src/messaging/categories/subgraph.ts

/**
 * Subgraph 消息类型
 */
export enum SubgraphMessageType {
  START = 'subgraph.start',
  END = 'subgraph.end',
  CONTEXT_INHERIT = 'subgraph.context.inherit',
  CONTEXT_RETURN = 'subgraph.context.return',
  STATE_SYNC = 'subgraph.state.sync',
}

/**
 * Subgraph 消息数据
 */
export interface SubgraphStartData {
  subthreadId: string;
  parentThreadId: string;
  rootThreadId: string;
  graphId: string;
  depth: number;
  inheritedVariables: Record<string, unknown>;
}

export interface SubgraphContextReturnData {
  subthreadId: string;
  parentThreadId: string;
  output: Record<string, unknown>;
  variables: Record<string, unknown>;
}
```

### 3.4 输出目标类型

```typescript
// packages/types/src/messaging/output.ts

/**
 * 输出目标
 */
export enum OutputTarget {
  /** TUI 显示 */
  TUI = 'tui',
  
  /** 功能性文件（程序间交换） */
  FILE_FUNCTIONAL = 'file_functional',
  
  /** 展示性文件（人类阅读） */
  FILE_DISPLAY = 'file_display',
  
  /** 日志 */
  LOG = 'log',
  
  /** 事件总线 */
  EVENT_BUS = 'event_bus',
  
  /** 不输出 */
  NONE = 'none',
}

/**
 * 输出决策
 */
export interface OutputDecision {
  /** 目标输出列表 */
  targets: OutputTarget[];
  
  /** 是否聚合到父实体 */
  aggregateToParent: boolean;
  
  /** 聚合级别 */
  aggregateLevel: 'none' | 'summary' | 'detail';
  
  /** 是否通知父实体 */
  notifyParent: boolean;
  
  /** 延迟（毫秒，用于防抖） */
  debounceMs?: number;
}

/**
 * 路由规则
 */
export interface RoutingRule {
  /** 规则名称 */
  name: string;
  
  /** 匹配条件 */
  match: {
    categories?: MessageCategory[];
    types?: string[];
    levels?: string[];
    entities?: string[];
    custom?: (message: BaseMessage) => boolean;
  };
  
  /** 输出决策 */
  decision: OutputDecision;
  
  /** 优先级（数字越小优先级越高） */
  priority: number;
}

/**
 * 默认路由规则
 */
export const DEFAULT_ROUTING_RULES: RoutingRule[] = [
  // Agent LLM 流式输出 -> 仅 TUI
  {
    name: 'agent-llm-stream',
    match: { 
      categories: [MessageCategory.AGENT],
      types: [AgentMessageType.LLM_STREAM] 
    },
    decision: {
      targets: [OutputTarget.TUI],
      aggregateToParent: false,
      aggregateLevel: 'none',
      notifyParent: false,
    },
    priority: 100,
  },
  
  // Agent Human Relay 请求 -> TUI + FILE_FUNCTIONAL + FILE_DISPLAY
  {
    name: 'agent-human-relay-request',
    match: { 
      categories: [MessageCategory.AGENT],
      types: [AgentMessageType.HUMAN_RELAY_REQUEST] 
    },
    decision: {
      targets: [OutputTarget.TUI, OutputTarget.FILE_FUNCTIONAL, OutputTarget.FILE_DISPLAY],
      aggregateToParent: true,
      aggregateLevel: 'summary',
      notifyParent: true,
    },
    priority: 100,
  },
  
  // Agent 工具结果 -> FILE_DISPLAY
  {
    name: 'agent-tool-result',
    match: { 
      categories: [MessageCategory.AGENT],
      types: [AgentMessageType.TOOL_RESULT] 
    },
    decision: {
      targets: [OutputTarget.FILE_DISPLAY],
      aggregateToParent: true,
      aggregateLevel: 'summary',
      notifyParent: false,
    },
    priority: 100,
  },
  
  // Thread 节点事件 -> TUI + FILE_DISPLAY
  {
    name: 'thread-node',
    match: { 
      categories: [MessageCategory.THREAD],
      types: [
        ThreadMessageType.NODE_START,
        ThreadMessageType.NODE_END,
      ] 
    },
    decision: {
      targets: [OutputTarget.TUI, OutputTarget.FILE_DISPLAY],
      aggregateToParent: false,
      aggregateLevel: 'none',
      notifyParent: false,
    },
    priority: 100,
  },
  
  // Subgraph 事件 -> FILE_DISPLAY + 聚合到父
  {
    name: 'subgraph-events',
    match: { categories: [MessageCategory.SUBGRAPH] },
    decision: {
      targets: [OutputTarget.FILE_DISPLAY],
      aggregateToParent: true,
      aggregateLevel: 'detail',
      notifyParent: true,
    },
    priority: 100,
  },
  
  // 默认规则 -> TUI
  {
    name: 'default',
    match: {},
    decision: {
      targets: [OutputTarget.TUI],
      aggregateToParent: false,
      aggregateLevel: 'none',
      notifyParent: false,
    },
    priority: 999,
  },
];
```

---

## 4. SDK API 层接口

### 4.1 消息总线 API

```typescript
// sdk/api/shared/messaging/message-bus-api.ts

import type { 
  BaseMessage, 
  MessageCategory,
  OutputTarget,
  RoutingRule,
} from '@wf-agent/types';

/**
 * 消息总线 API
 */
export interface MessageBusAPI {
  /**
   * 发布消息
   */
  publish(message: Omit<BaseMessage, 'id' | 'timestamp'>): void;
  
  /**
   * 订阅消息
   */
  subscribe(
    filter: MessageFilter,
    handler: MessageHandler
  ): MessageSubscription;
  
  /**
   * 配置路由规则
   */
  configureRouting(rules: RoutingRule[]): void;
  
  /**
   * 注册输出处理器
   */
  registerOutputHandler(handler: OutputHandler): void;
  
  /**
   * 获取消息历史
   */
  getHistory(filter?: MessageFilter): BaseMessage[];
}

/**
 * 消息过滤器
 */
export interface MessageFilter {
  categories?: MessageCategory[];
  types?: string[];
  entityIds?: string[];
  levels?: string[];
  since?: number;
  until?: number;
}

/**
 * 消息处理器
 */
export type MessageHandler = (message: BaseMessage) => void | Promise<void>;

/**
 * 消息订阅
 */
export interface MessageSubscription {
  unsubscribe(): void;
}

/**
 * 输出处理器
 */
export interface OutputHandler {
  readonly target: OutputTarget;
  readonly name: string;
  handle(message: BaseMessage): Promise<void>;
  supports(message: BaseMessage): boolean;
}
```

### 4.2 使用示例

```typescript
// 在 Graph 执行中发送消息
import { MessageBusAPI, MessageCategory, ThreadMessageType } from '@wf-agent/sdk';

class ThreadExecutionService {
  constructor(private messageBus: MessageBusAPI) {}
  
  async startNode(threadId: string, nodeId: string, nodeType: string) {
    // 发送节点开始消息
    this.messageBus.publish({
      category: MessageCategory.THREAD,
      type: ThreadMessageType.NODE_START,
      level: 'info',
      entity: {
        type: 'thread',
        id: threadId,
        rootId: threadId,
        depth: 0,
      },
      data: {
        threadId,
        nodeId,
        nodeType,
        status: 'running',
      },
    });
  }
}

// 在 CLI-App 中订阅消息
import { MessageBusAPI, MessageCategory, AgentMessageType } from '@wf-agent/sdk';

class CLIMessageHandler {
  constructor(private messageBus: MessageBusAPI) {
    // 订阅 Agent Human Relay 请求
    this.messageBus.subscribe(
      {
        categories: [MessageCategory.AGENT],
        types: [AgentMessageType.HUMAN_RELAY_REQUEST],
      },
      async (message) => {
        // 写入功能性文件
        await this.fileIO.writeHumanRelayOutput({
          sessionId: message.entity.id,
          content: message.data.prompt,
        });
        
        // 在 TUI 显示提示
        this.tui.showNotification({
          type: 'human-relay',
          message: `Human Relay requested: ${message.entity.id}`,
        });
      }
    );
  }
}
```

---

## 5. CLI-App 特有实现

### 5.1 File IO 处理器

```typescript
// apps/cli-app/src/messaging/handlers/file-io-handler.ts

import type { OutputHandler, BaseMessage, OutputTarget } from '@wf-agent/sdk';
import type { FileIOService } from '../file-io/file-io-service';

/**
 * CLI-App 文件 IO 处理器
 * 实现 SDK 的 OutputHandler 接口
 */
export class FileIOHandler implements OutputHandler {
  readonly target = OutputTarget.FILE_FUNCTIONAL;
  readonly name = 'cli-file-io';
  
  constructor(private fileIO: FileIOService) {}
  
  supports(message: BaseMessage): boolean {
    // 只处理 Human Relay 请求
    return message.type === 'agent.human_relay.request';
  }
  
  async handle(message: BaseMessage): Promise<void> {
    if (message.type === 'agent.human_relay.request') {
      const { prompt } = message.data as AgentHumanRelayRequestData;
      
      // 写入功能性文件
      await this.fileIO.writeHumanRelayOutput({
        sessionId: message.entity.id,
        content: prompt,
      });
      
      // 启动文件监控
      this.fileIO.watchHumanRelayInput({
        sessionId: message.entity.id,
        onResponse: (content) => {
          // 发送响应消息
          this.publishResponse(message.entity.id, content);
        },
      });
    }
  }
  
  private publishResponse(entityId: string, content: string): void {
    // 通过消息总线发送响应
    // ...
  }
}
```

### 5.2 TUI 处理器

```typescript
// apps/cli-app/src/messaging/handlers/tui-handler.ts

import type { OutputHandler, BaseMessage, OutputTarget } from '@wf-agent/sdk';
import type { TUIRenderer } from '../tui/tui-renderer';

/**
 * CLI-App TUI 处理器
 */
export class TUIHandler implements OutputHandler {
  readonly target = OutputTarget.TUI;
  readonly name = 'cli-tui';
  
  constructor(private tui: TUIRenderer) {}
  
  supports(message: BaseMessage): boolean {
    // 支持白名单内的消息类型
    const supportedTypes = new Set([
      'agent.llm.stream',
      'agent.tool.call_start',
      'agent.tool.call_end',
      'agent.human_relay.request',
      'thread.node.start',
      'thread.node.end',
    ]);
    return supportedTypes.has(message.type);
  }
  
  async handle(message: BaseMessage): Promise<void> {
    switch (message.type) {
      case 'agent.llm.stream':
        this.tui.appendStream(message.data.chunk);
        break;
      case 'agent.tool.call_start':
        this.tui.showToolCall({
          name: message.data.toolName,
          status: 'running',
        });
        break;
      case 'agent.human_relay.request':
        this.tui.showNotification({
          type: 'human-relay',
          message: `Check: ${message.data.outputFile}`,
        });
        break;
      // ... 其他类型
    }
  }
}
```

---

## 6. 迁移步骤

### 6.1 第一阶段：类型定义迁移

1. 在 `packages/types/src/messaging/` 创建新类型
2. 保持 `packages/types/src/events/` 不变（向后兼容）
3. 在 `packages/types/src/index.ts` 导出 messaging

### 6.2 第二阶段：SDK 实现

1. 在 `sdk/core/messaging/` 实现消息总线
2. 在 `sdk/api/shared/messaging/` 提供 API 接口
3. 将现有事件系统逐步迁移到消息系统

### 6.3 第三阶段：CLI-App 适配

1. 更新 CLI-App 使用 SDK 消息 API
2. 实现 File IO 和 TUI 处理器
3. 移除 CLI-App 内原有的消息类型定义

### 6.4 第四阶段：废弃旧事件

1. 标记旧事件系统为 deprecated
2. 提供迁移指南
3. 在后续版本中移除

---

## 7. 与现有事件系统的关系

### 7.1 兼容性策略

```
┌─────────────────────────────────────────────────────────────────┐
│                     SDK 事件系统 (现有)                          │
│  packages/types/src/events/                                     │
│  - 基于 EventEmitter                                            │
│  - 用于内部组件通信                                             │
│  - 逐步迁移到消息系统                                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼ 迁移
┌─────────────────────────────────────────────────────────────────┐
│                     SDK 消息系统 (新增)                          │
│  packages/types/src/messaging/                                  │
│  sdk/core/messaging/                                            │
│  - 统一的消息类型定义                                           │
│  - 支持路由和输出决策                                           │
│  - 供所有应用使用                                               │
└─────────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
        ┌─────────┐     ┌─────────┐     ┌─────────┐
        │ CLI-App │     │ Web-App │     │ Other   │
        │ File IO │     │ UI      │     │ Apps    │
        │ Handler │     │ Handler │     │ ...     │
        └─────────┘     └─────────┘     └─────────┘
```

### 7.2 事件到消息的映射

| 现有事件 | 新消息类型 | 说明 |
|---------|-----------|------|
| `THREAD_STARTED` | `thread.start` | 直接映射 |
| `NODE_STARTED` | `thread.node.start` | 归类到 Thread |
| `AGENT_LOOP_STARTED` | `agent.start` | 新增 Agent 类别 |
| `TOOL_CALL_STARTED` | `agent.tool.call_start` | 归类到 Agent |
| `HUMAN_RELAY_REQUESTED` | `agent.human_relay.request` | 规范化命名 |
| `SUBGRAPH_STARTED` | `subgraph.start` | 新增 Subgraph 类别 |

---

## 8. 总结

### 8.1 迁移收益

1. **通用性** - 消息类型成为 SDK 标准，供所有应用使用
2. **一致性** - 统一的消息命名和结构
3. **可扩展性** - 新应用可以直接使用，无需重新定义
4. **可维护性** - 类型定义集中管理

### 8.2 CLI-App 特有职责

1. **File IO 处理器** - 实现功能性/展示性文件写入
2. **TUI 处理器** - 实现终端界面渲染
3. **配置** - 定义 CLI 特定的路由规则

### 8.3 文件位置总结

| 内容 | 位置 | 说明 |
|------|------|------|
| 消息类型定义 | `packages/types/src/messaging/` | SDK 通用 |
| 消息总线 API | `sdk/api/shared/messaging/` | SDK API 层 |
| 消息总线实现 | `sdk/core/messaging/` | SDK 核心 |
| File IO 处理器 | `apps/cli-app/src/messaging/` | CLI-App 特有 |
| TUI 处理器 | `apps/cli-app/src/messaging/` | CLI-App 特有 |
| File IO 规范 | `apps/cli-app/docs/spec/file-io-prd.md` | CLI-App 特有 |
