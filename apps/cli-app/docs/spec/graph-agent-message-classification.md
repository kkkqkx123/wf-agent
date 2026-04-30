# wf-agent 消息分类与多实例管理规范

## 1. 概述

### 1.1 架构关系

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           CLI-App 消息输出系统                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                         Graph (工作流引擎)                            │   │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────┐  │   │
│  │  │ WorkflowExec    │  │ WorkflowExec    │  │   WorkflowExec      │  │   │
│  │  │  (session-001)  │  │  (session-002)  │  │    (session-003)    │  │   │
│  │  │                 │  │                 │  │                     │  │   │
│  │  │ ┌─────────────┐ │  │ ┌─────────────┐ │  │ ┌─────────────────┐ │  │   │
│  │  │ │ Agent Node  │ │  │ │  LLM Node   │ │  │ │  Subgraph Node  │ │  │   │
│  │  │ │             │─┼──┼▶│             │ │  │ │                 │ │  │   │
│  │  │ │ ┌─────────┐ │ │  │ │             │ │  │ │ ┌─────────────┐ │ │  │   │
│  │  │ │ │ Agent   │ │ │  │ │             │ │  │ │ │ Sub-Exec  │ │ │  │   │
│  │  │ │ │ Loop    │◀┼─┼──┼─┤             │ │  │ │ │   实例      │─┼─┼──┤   │
│  │  │ │ │(loop-1) │ │ │  │ │             │ │  │ │ │(session-004)│ │ │  │   │
│  │  │ │ └─────────┘ │ │  │ └─────────────┘ │  │ │ └─────────────┘ │ │  │   │
│  │  │ └─────────────┘ │  │                 │  │ └─────────────────┘ │  │   │
│  │  └─────────────────┘  └─────────────────┘  └─────────────────────┘  │   │
│  │                                                                     │   │
│  │  • WorkflowExecution is the execution instance of Graph                                          │   │
│  │  • 支持 Fork/Join 并行执行                                             │   │
│  │  • 支持 Subgraph 嵌套调用                                              │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    ▲                                        │
│                                    │ 互调用                                  │
│                                    ▼                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                          Agent (Agent 引擎)                           │   │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────┐  │   │
│  │  │  Agent 实例      │  │  Agent 实例      │  │     Agent 实例       │  │   │
│  │  │   (loop-001)    │  │   (loop-002)    │  │     (loop-003)      │  │   │
│  │  │                 │  │                 │  │                     │  │   │
│  │  │ • 独立生命周期   │  │ • 可暂停/恢复    │  │ • 工具调用能力       │  │   │
│  │  │ • 迭代执行       │  │ • 检查点支持     │  │ • Human Relay       │  │   │
│  │  └─────────────────┘  └─────────────────┘  └─────────────────────┘  │   │
│  │                                                                     │   │
│  │  • Agent Loop 可被 Graph 的 Agent Node 调用                            │   │
│  │  • Agent 也可独立运行（不依赖 Graph）                                   │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 核心概念

| 概念 | 说明 | 消息前缀 |
|------|------|----------|
| **Graph** | 工作流定义，包含节点和边的静态结构 | - |
| **WorkflowExecution** | Graph 的执行实例，有独立状态和生命周期 | `execution.{id}` |
| **Agent** | Agent Loop 定义，包含配置和工具 | - |
| **Agent Loop** | Agent 的执行实例，有独立迭代状态 | `agent.{id}` |
| **Subgraph** | Graph 中的子工作流节点，创建子 WorkflowExecution | `subgraph.{id}` |

### 1.3 互调用关系

```
场景 1: Graph 调用 Agent
─────────────────────────
WorkflowExecution (session-main)
  └── Agent Node
      └── Agent Loop (loop-001) [独立实例]
          └── 迭代执行
              └── Human Relay

场景 2: Agent 独立运行
─────────────────────────
Agent Loop (loop-standalone) [独立实例]
  └── 迭代执行
      └── Tool Call

场景 3: Graph 嵌套 Subgraph
─────────────────────────
WorkflowExecution (session-parent)
  └── Subgraph Node
      └── Sub-Execution (session-child) [独立实例]
          └── Agent Node
              └── Agent Loop (loop-nested) [独立实例]

场景 4: 并行执行 Fork/Join
─────────────────────────
WorkflowExecution (session-main)
  └── Fork Node
      ├── Branch 1 → WorkflowExecution (session-branch-1)
      └── Branch 2 → WorkflowExecution (session-branch-2)
  └── Join Node [等待所有分支]
```

---

## 2. 消息分类体系

### 2.1 消息标识结构

```typescript
/**
 * 消息标识符
 * 格式: {entityType}.{entityId}.{messageCategory}.{messageType}
 * 
 * 示例:
 * - execution.session-001.workflow.node_start
 * - agent.loop-001.iteration.start
 * - subgraph.session-sub-001.agent.stream
 */
interface MessageIdentity {
  /** 实体类型: execution | agent | subgraph */
  entityType: 'execution' | 'agent' | 'subgraph';
  
  /** 实体实例 ID */
  entityId: string;
  
  /** 消息类别 */
  category: MessageCategory;
  
  /** 消息类型 */
  type: MessageType;
  
  /** 父实体 ID（用于嵌套场景） */
  parentEntityId?: string;
  
  /** 根实体 ID（最顶层） */
  rootEntityId: string;
}

/**
 * 完整消息结构
 */
interface EntityMessage extends BaseMessage {
  /** 消息标识 */
  identity: MessageIdentity;
  
  /** 实体层级路径（用于聚合显示） */
  entityPath: string[];
  
  /** 并行组 ID（Fork/Join 场景） */
  parallelGroupId?: string;
  
  /** 执行顺序索引 */
  sequenceIndex: number;
}
```

### 2.2 WorkflowExecution Message Classification

```typescript
/**
 * WorkflowExecution message categories
 * Prefix: execution.{executionId}
 */
enum WorkflowExecutionMessageType {
  // Lifecycle (1000-1099)
  EXECUTION_START = 'execution.start',
  EXECUTION_PAUSE = 'execution.pause',
  EXECUTION_RESUME = 'execution.resume',
  EXECUTION_END = 'execution.end',
  EXECUTION_CANCEL = 'execution.cancel',
  
  // Node execution (1100-1199)
  NODE_START = 'node.start',
  NODE_END = 'node.end',
  NODE_ERROR = 'node.error',
  NODE_SKIP = 'node.skip',
  
  // Workflow status (1200-1299)
  WORKFLOW_START = 'workflow.start',
  WORKFLOW_END = 'workflow.end',
  WORKFLOW_CHECKPOINT = 'workflow.checkpoint',
  
  // Variable changes (1300-1399)
  VARIABLE_SET = 'variable.set',
  VARIABLE_GET = 'variable.get',
  
  // Parallel execution (1400-1499)
  FORK_START = 'fork.start',
  FORK_BRANCH_START = 'fork.branch_start',
  FORK_BRANCH_END = 'fork.branch_end',
  JOIN_WAIT = 'join.wait',
  JOIN_COMPLETE = 'join.complete',
  
  // Subgraph calls (1500-1599)
  SUBGRAPH_CALL = 'subgraph.call',
  SUBGRAPH_RETURN = 'subgraph.return',
  
  // Agent node calls (1600-1699)
  AGENT_NODE_CALL = 'agent_node.call',
  AGENT_NODE_RETURN = 'agent_node.return',
}

/**
 * WorkflowExecution message data
 */
interface WorkflowExecutionMessageData {
  executionId: string;
  workflowId: string;
  nodeId?: string;
  nodeType?: string;
  parentExecutionId?: string;
  parallelGroupId?: string;
}
```

### 2.3 Agent 消息分类

```typescript
/**
 * Agent 消息类别
 * 前缀: agent.{loopId}
 */
enum AgentMessageType {
  // 生命周期 (2000-2099)
  AGENT_START = 'agent.start',
  AGENT_PAUSE = 'agent.pause',
  AGENT_RESUME = 'agent.resume',
  AGENT_END = 'agent.end',
  AGENT_CANCEL = 'agent.cancel',
  
  // 迭代执行 (2100-2199)
  ITERATION_START = 'iteration.start',
  ITERATION_END = 'iteration.end',
  ITERATION_LIMIT_HIT = 'iteration.limit_hit',
  
  // LLM 交互 (2200-2299)
  LLM_REQUEST = 'llm.request',
  LLM_STREAM = 'llm.stream',
  LLM_RESPONSE = 'llm.response',
  LLM_ERROR = 'llm.error',
  
  // 工具调用 (2300-2399)
  TOOL_CALL_START = 'tool_call.start',
  TOOL_CALL_END = 'tool_call.end',
  TOOL_RESULT = 'tool.result',
  TOOL_ERROR = 'tool.error',
  
  // Human Relay (2400-2499)
  HUMAN_RELAY_REQUEST = 'human_relay.request',
  HUMAN_RELAY_RESPONSE = 'human_relay.response',
  HUMAN_RELAY_TIMEOUT = 'human_relay.timeout',
  HUMAN_RELAY_CANCEL = 'human_relay.cancel',
  
  // 检查点 (2500-2599)
  CHECKPOINT_CREATE = 'checkpoint.create',
  CHECKPOINT_RESTORE = 'checkpoint.restore',
  
  // 消息历史 (2600-2699)
  MESSAGE_ADDED = 'message.added',
  MESSAGE_UPDATED = 'message.updated',
}

/**
 * Agent 消息数据
 */
interface AgentMessageData {
  loopId: string;
  agentId: string;
  threadId?: string;  // 如果被 Graph 调用
  nodeId?: string;    // 如果被 Graph 调用
  iteration?: number;
  maxIterations?: number;
}
```

### 2.4 Subgraph 消息分类

```typescript
/**
 * Subgraph 消息类别
 * 前缀: subgraph.{subthreadId}
 * 
 * Subgraph 本质上是 Thread，但带有父 Thread 上下文
 */
enum SubgraphMessageType {
  // 生命周期 (3000-3099)
  SUBGRAPH_START = 'subgraph.start',
  SUBGRAPH_END = 'subgraph.end',
  
  // 上下文传递 (3100-3199)
  CONTEXT_INHERIT = 'context.inherit',    // 继承父 Thread 上下文
  CONTEXT_RETURN = 'context.return',      // 返回结果到父 Thread
  
  // 状态同步 (3200-3299)
  STATE_SYNC = 'state.sync',              // 与父 Thread 状态同步
}

/**
 * Subgraph 消息数据
 */
interface SubgraphMessageData {
  subthreadId: string;
  parentThreadId: string;
  rootThreadId: string;
  graphId: string;
  depth: number;  // 嵌套深度
}
```

---

## 3. 多实例并行管理

### 3.1 实例隔离

```typescript
/**
 * 实例上下文
 * 每个执行实例（Thread/Agent Loop）都有独立的上下文
 */
interface InstanceContext {
  /** 实例 ID */
  instanceId: string;
  
  /** 实例类型 */
  type: 'thread' | 'agent' | 'subgraph';
  
  /** 父实例 ID */
  parentId?: string;
  
  /** 根实例 ID */
  rootId: string;
  
  /** 并行组信息 */
  parallelGroup?: {
    groupId: string;
    branchIndex: number;
    totalBranches: number;
  };
  
  /** 嵌套深度 */
  depth: number;
  
  /** 文件 IO 路径 */
  fileIOPaths: {
    functional: string;
    display: string;
  };
}

/**
 * 实例注册表
 */
interface InstanceRegistry {
  /** 注册实例 */
  register(context: InstanceContext): void;
  
  /** 获取实例 */
  get(instanceId: string): InstanceContext | undefined;
  
  /** 获取子实例 */
  getChildren(parentId: string): InstanceContext[];
  
  /** 获取根实例的所有后代 */
  getDescendants(rootId: string): InstanceContext[];
  
  /** 注销实例 */
  unregister(instanceId: string): void;
}
```

### 3.2 消息路由策略

```typescript
/**
 * 消息路由决策
 */
interface RoutingDecision {
  /** 目标输出 */
  targets: OutputTarget[];
  
  /** 是否聚合到父实例 */
  aggregateToParent: boolean;
  
  /** 聚合级别 */
  aggregateLevel: 'none' | 'summary' | 'detail';
  
  /** 是否触发父实例状态更新 */
  notifyParent: boolean;
}

/**
 * 路由规则
 */
const ROUTING_RULES: Record<string, RoutingDecision> = {
  // Thread 消息默认路由
  'thread.*': {
    targets: [OutputTarget.TUI, OutputTarget.FILE_DISPLAY],
    aggregateToParent: false,
    aggregateLevel: 'none',
    notifyParent: false,
  },
  
  // Agent 消息默认路由
  'agent.*': {
    targets: [OutputTarget.TUI, OutputTarget.FILE_DISPLAY],
    aggregateToParent: true,  // 聚合到父 Thread
    aggregateLevel: 'summary',
    notifyParent: true,
  },
  
  // Agent Human Relay 路由
  'agent.*.human_relay.*': {
    targets: [OutputTarget.TUI, OutputTarget.FILE_FUNCTIONAL, OutputTarget.FILE_DISPLAY],
    aggregateToParent: true,
    aggregateLevel: 'summary',
    notifyParent: true,
  },
  
  // Subgraph 消息路由
  'subgraph.*': {
    targets: [OutputTarget.FILE_DISPLAY],  // Subgraph 不直接显示在 TUI
    aggregateToParent: true,  // 聚合到父 Thread
    aggregateLevel: 'detail',
    notifyParent: true,
  },
  
  // Fork 分支消息路由
  '*.fork.branch_*': {
    targets: [OutputTarget.FILE_DISPLAY],
    aggregateToParent: true,
    aggregateLevel: 'summary',
    notifyParent: false,  // Join 完成后再通知
  },
};
```

### 3.3 聚合呈现策略

```typescript
/**
 * 聚合配置
 */
interface AggregationConfig {
  /** 根实例 ID */
  rootInstanceId: string;
  
  /** 聚合文件路径 */
  outputFile: string;
  
  /** 包含的子实例 */
  includedInstances: string[];
  
  /** 聚合策略 */
  strategy: 'flat' | 'hierarchical' | 'timeline';
}

/**
 * 聚合策略实现
 */
enum AggregationStrategy {
  /**
   * 扁平聚合
   * 所有实例消息按时间顺序排列
   */
  FLAT = 'flat',
  
  /**
   * 层级聚合
   * 按实例层级树形展示
   */
  HIERARCHICAL = 'hierarchical',
  
  /**
   * 时间线聚合
   * 按时间线展示，显示并行关系
   */
  TIMELINE = 'timeline',
}
```

---

## 4. 典型场景消息流

### 4.1 场景 1: Graph → Agent → Human Relay

```
时间线 ───────────────────────────────────────────────────────────────▶

Thread (session-main)
  │
  ├──► NODE_START [agent-node-1]
  │
  ├──► AGENT_NODE_CALL
  │      entity: thread.session-main
  │      data: { targetLoopId: loop-001 }
  │
  │      Agent Loop (loop-001) 启动
  │        │
  │        ├──► AGENT_START
  │        │      entity: agent.loop-001
  │        │      parent: thread.session-main
  │        │
  │        ├──► ITERATION_START [1/10]
  │        │
  │        ├──► LLM_REQUEST
  │        │
  │        ├──► LLM_STREAM [chunk-1]
  │        │      target: TUI (流式显示)
  │        │
  │        ├──► LLM_STREAM [chunk-2]
  │        │      target: TUI
  │        │
  │        ├──► TOOL_CALL_START [file_read]
  │        │      target: TUI (摘要)
  │        │
  │        ├──► TOOL_CALL_END [file_read]
  │        │      target: TUI
  │        │
  │        ├──► TOOL_RESULT
  │        │      target: FILE_DISPLAY (详情)
  │        │      aggregateTo: thread.session-main
  │        │
  │        └──► HUMAN_RELAY_REQUEST
  │               entity: agent.loop-001
  │               targets: TUI + FILE_FUNCTIONAL + FILE_DISPLAY
  │               ├──► TUI: 显示简洁提示
  │               ├──► FILE_FUNCTIONAL: 写入 human-relay-output.txt
  │               └──► FILE_DISPLAY: 更新 output.md (聚合视图)
  │
  ├──► [等待 Human Relay 响应...]
  │
  │      用户保存 human-relay-input.txt
  │        │
  │        └──► HUMAN_RELAY_RESPONSE
  │               entity: agent.loop-001
  │               targets: TUI
  │
  ├──► AGENT_NODE_RETURN
  │      data: { result: '...' }
  │
  └──► NODE_END [agent-node-1]
```

### 4.2 场景 2: Fork/Join 并行执行

```
时间线 ───────────────────────────────────────────────────────────────▶

Thread (session-main)
  │
  ├──► FORK_START [parallel-group-1]
  │      branches: 2
  │
  ├──┬──► FORK_BRANCH_START [branch-0]
  │  │     newThread: session-branch-0
  │  │
  │  │     Thread (session-branch-0)
  │  │       ├──► NODE_START [llm-node]
  │  │       ├──► LLM_STREAM [...]
  │  │       └──► NODE_END [llm-node]
  │  │            target: FILE_DISPLAY
  │  │            aggregateTo: session-main
  │  │
  │  └──► FORK_BRANCH_END [branch-0]
  │
  ├──┬──► FORK_BRANCH_START [branch-1]
  │  │     newThread: session-branch-1
  │  │
  │  │     Thread (session-branch-1)
  │  │       ├──► NODE_START [agent-node]
  │  │       ├──► AGENT_NODE_CALL → Agent Loop (loop-002)
  │  │       │                      ├──► AGENT_START
  │  │       │                      ├──► ITERATION_START
  │  │       │                      └──► HUMAN_RELAY_REQUEST
  │  │       │                             targets: FILE_FUNCTIONAL
  │  │       │                             aggregateTo: session-main
  │  │       └──► NODE_END [agent-node]
  │  │            target: FILE_DISPLAY
  │  │            aggregateTo: session-main
  │  │
  │  └──► FORK_BRANCH_END [branch-1]
  │
  ├──► JOIN_WAIT
  │      waiting: [session-branch-0, session-branch-1]
  │
  ├──► [等待所有分支完成...]
  │
  └──► JOIN_COMPLETE
         results: [result-0, result-1]
         target: TUI + FILE_DISPLAY
```

### 4.3 场景 3: 嵌套 Subgraph

```
Thread (session-parent) [depth: 0]
  │
  ├──► SUBGRAPH_CALL
  │      subgraphGraphId: sub-workflow-1
  │      newThread: session-child
  │
  │      Thread (session-child) [depth: 1]
  │        ├──► SUBGRAPH_START
  │        │      parent: session-parent
  │        │      root: session-parent
  │        │
  │        ├──► CONTEXT_INHERIT
  │        │      variables: { ... }
  │        │
  │        ├──► NODE_START [agent-node]
  │        ├──► AGENT_NODE_CALL → Agent Loop (loop-nested)
  │        │                      ├──► AGENT_START
  │        │                      │      parent: session-child
  │        │                      │      root: session-parent
  │        │                      │      depth: 2
  │        │                      ├──► HUMAN_RELAY_REQUEST
  │        │                      │      fileIO: .wf-agent/function/session-child/...
  │        │                      └──► AGENT_END
  │        │                             aggregateTo: session-child
  │        ├──► NODE_END [agent-node]
  │        │      aggregateTo: session-parent
  │        │
  │        ├──► CONTEXT_RETURN
  │        │      output: { ... }
  │        │
  │        └──► SUBGRAPH_END
  │               aggregateTo: session-parent
  │
  └──► SUBGRAPH_RETURN
         result: { ... }
```

---

## 5. 文件 IO 目录结构

### 5.1 多实例目录组织

```
.wf-agent/
├── function/                              # 功能性文件
│   ├── session-main/                      # Thread 实例
│   │   ├── human-relay-input.txt
│   │   └── human-relay-output.txt
│   │
│   ├── session-main-fork-0/               # Fork 分支 0
│   │   └── (文件)
│   │
│   ├── session-main-fork-1/               # Fork 分支 1
│   │   └── (文件)
│   │
│   ├── session-child/                     # Subgraph 子实例
│   │   └── (文件)
│   │
│   └── agent-loop-001/                    # Agent Loop 实例
│       ├── human-relay-input.txt
│       └── human-relay-output.txt
│
└── display/                               # 展示性文件
    ├── session-main/
    │   ├── output.md                      # 聚合呈现
    │   ├── execution-log.md
    │   └── sub-instances/                 # 子实例链接
    │       ├── session-main-fork-0 -> ../../session-main-fork-0/
    │       ├── session-main-fork-1 -> ../../session-main-fork-1/
    │       └── session-child -> ../../session-child/
    │
    ├── session-main-fork-0/
    │   └── output.md
    │
    ├── session-main-fork-1/
    │   └── output.md
    │
    ├── session-child/
    │   ├── output.md
    │   └── sub-instances/
    │       └── agent-loop-001 -> ../../agent-loop-001/
    │
    └── agent-loop-001/
        └── output.md
```

### 5.2 聚合 output.md 结构

```markdown
---
instanceId: thread-session-main
type: thread
graphId: workflow-v1
parentId: null
rootId: thread-session-main
depth: 0
startedAt: 1705312345678
status: running
---

# Thread Execution: session-main

======

## 基本信息

- **实例类型**: Thread
- **工作流**: workflow-v1
- **状态**: 运行中
- **开始时间**: 2024-01-15 10:30:00
- **当前节点**: agent-node-1

══════════════════════════════

## 执行日志

### [10:30:05] Node: start
- 状态: ✓ 完成
- 耗时: 12ms

### [10:30:08] Node: agent-node-1
- 状态: ⏳ 运行中
- 类型: Agent Loop
- 子实例: [agent-loop-001](../agent-loop-001/output.md)

══════════════════════════════

## 子实例

### Fork 分支

| 分支 | 状态 | 链接 |
|------|------|------|
| Branch 0 | ✓ 完成 | [session-main-fork-0](./sub-instances/session-main-fork-0/output.md) |
| Branch 1 | ⏳ 运行中 | [session-main-fork-1](./sub-instances/session-main-fork-1/output.md) |

### Subgraph

- [session-child](./sub-instances/session-child/output.md) - 状态: 运行中

### Agent Loop

- [agent-loop-001](./sub-instances/agent-loop-001/output.md) - 状态: 等待 Human Relay

══════════════════════════════

## Human Relay 进行中

**Agent Loop**: agent-loop-001
**节点**: agent-node-1

**操作步骤**:
1. 查看提示词：`.wf-agent/function/agent-loop-001/human-relay-output.txt`
2. 复制提示词到网页端 LLM
3. 将 LLM 响应粘贴到：`.wf-agent/function/agent-loop-001/human-relay-input.txt`
4. 保存文件，系统将自动继续

======
```

---

## 6. 消息输出决策表

### 6.1 Thread 消息

| 消息类型 | TUI | FILE_FUNCTIONAL | FILE_DISPLAY | 聚合到父 |
|---------|-----|-----------------|--------------|---------|
| THREAD_START | ✓ | ✗ | ✓ | ✗ |
| THREAD_END | ✓ | ✗ | ✓ | ✗ |
| NODE_START | ✓ | ✗ | ✓ | ✗ |
| NODE_END | ✓ | ✗ | ✓ | ✗ |
| FORK_START | ✓ | ✗ | ✓ | ✗ |
| FORK_BRANCH_START | ✗ | ✗ | ✓ | ✓ |
| JOIN_WAIT | ✓ | ✗ | ✓ | ✗ |
| JOIN_COMPLETE | ✓ | ✗ | ✓ | ✗ |
| SUBGRAPH_CALL | ✓ | ✗ | ✓ | ✗ |
| SUBGRAPH_RETURN | ✓ | ✗ | ✓ | ✗ |
| AGENT_NODE_CALL | ✓ | ✗ | ✓ | ✗ |
| AGENT_NODE_RETURN | ✓ | ✗ | ✓ | ✗ |

### 6.2 Agent 消息

| 消息类型 | TUI | FILE_FUNCTIONAL | FILE_DISPLAY | 聚合到父 Thread |
|---------|-----|-----------------|--------------|----------------|
| AGENT_START | ✓ | ✗ | ✓ | ✓ |
| AGENT_END | ✓ | ✗ | ✓ | ✓ |
| ITERATION_START | ✓ | ✗ | ✓ | ✓ |
| ITERATION_END | ✓ | ✗ | ✓ | ✓ |
| LLM_STREAM | ✓ | ✗ | ✗ | ✗ |
| LLM_RESPONSE | ✗ | ✗ | ✓ | ✓ |
| TOOL_CALL_START | ✓ | ✗ | ✗ | ✗ |
| TOOL_CALL_END | ✓ | ✗ | ✗ | ✗ |
| TOOL_RESULT | ✗ | ✗ | ✓ | ✓ |
| **HUMAN_RELAY_REQUEST** | ✓ | **✓** | ✓ | ✓ |
| **HUMAN_RELAY_RESPONSE** | ✓ | ✗ | ✗ | ✗ |
| CHECKPOINT_CREATE | ✗ | ✗ | ✓ | ✓ |

### 6.3 Subgraph 消息

| 消息类型 | TUI | FILE_FUNCTIONAL | FILE_DISPLAY | 聚合到父 Thread |
|---------|-----|-----------------|--------------|----------------|
| SUBGRAPH_START | ✗ | ✗ | ✓ | ✓ |
| SUBGRAPH_END | ✗ | ✗ | ✓ | ✓ |
| CONTEXT_INHERIT | ✗ | ✗ | ✓ | ✓ |
| CONTEXT_RETURN | ✗ | ✗ | ✓ | ✓ |

---

## 7. 实现建议

### 7.1 核心模块

```
src/
├── message/
│   ├── types/
│   │   ├── entity-message.ts       # 实体消息基础类型
│   │   ├── thread-messages.ts      # Thread 消息定义
│   │   ├── agent-messages.ts       # Agent 消息定义
│   │   └── subgraph-messages.ts    # Subgraph 消息定义
│   ├── routing/
│   │   ├── entity-router.ts        # 实体消息路由
│   │   ├── aggregation-manager.ts  # 聚合管理
│   │   └── routing-rules.ts        # 路由规则
│   ├── registry/
│   │   └── instance-registry.ts    # 实例注册表
│   └── handlers/
│       ├── thread-handler.ts
│       ├── agent-handler.ts
│       └── subgraph-handler.ts
```

### 7.2 关键实现点

1. **实体路径追踪** - 每个消息必须携带完整的 entityPath
2. **并行组管理** - Fork/Join 场景需要跟踪并行组状态
3. **嵌套深度限制** - 防止无限递归嵌套
4. **文件 IO 隔离** - 每个实例独立的文件目录
5. **聚合一致性** - 确保父实例聚合视图实时更新
