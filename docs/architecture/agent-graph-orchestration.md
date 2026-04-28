# Agent-Graph 统一协调架构设计

## 概述

本文档描述了 Agent Loop 和 Graph Workflow 之间的统一协调架构设计。该架构旨在提供一种清晰、灵活的方式，让 Agent 和 Graph 模块能够相互协作，支持多种执行模式。

## 背景

### 删除 Dynamic Thread 体系

在之前的架构中，存在 `DynamicThreadManager` 和 `start_dynamic_child` 机制，用于在未定义图结构的情况下关联两个工作流。经过分析发现：

1. **功能重复**：与 `TriggeredSubworkflowManager` 高度重复
2. **概念模糊**：设计目的不明确，实际使用场景缺失
3. **维护成本**：增加了不必要的代码复杂度

因此，我们删除了 Dynamic Thread 体系，并设计了本统一协调架构。

## 当前架构

### 模块关系

```
┌─────────────────────────────────────────────────────────────────┐
│                        Graph Module                              │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ ThreadEntity → ThreadExecutor → NodeHandlers            │    │
│  │                                     ↓                   │    │
│  │                           agent-loop-handler.ts         │    │
│  └─────────────────────────────────────────────────────────┘    │
│                              ↓                                   │
│                    AgentLoopCoordinator.execute()                │
└─────────────────────────────────────────────────────────────────┘
                               ↓
┌─────────────────────────────────────────────────────────────────┐
│                        Agent Module                              │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ AgentLoopEntity → AgentLoopExecutor → LLM/Tool calls    │    │
│  │                                                              │
│  │ AgentLoopCoordinator: 创建、执行、暂停、恢复              │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

### 当前协作方式

| 方向 | 机制 | 说明 |
|------|------|------|
| Graph → Agent | `AGENT_LOOP` 节点类型 | Graph 通过 `agent-loop-handler.ts` 调用 Agent 模块 |
| Agent → Graph | 暂不支持 | 需要通过本架构设计实现 |

### 状态共享机制

- **ConversationManager**：统一管理消息历史
- **parentThreadId/nodeId**：建立父子关系，支持级联操作
- **VariableStateManager**：变量作用域管理

## 统一协调架构

### 核心设计：WorkflowOrchestrator

引入 `WorkflowOrchestrator` 作为顶层协调器，统一管理 Agent Loop 和 Graph Workflow 的生命周期。

```
┌─────────────────────────────────────────────────────────────────┐
│                    WorkflowOrchestrator                          │
│                                                                  │
│  职责：                                                          │
│  - 统一管理 Agent Loop 和 Graph Workflow 的生命周期             │
│  - 提供工作流间通信机制                                          │
│  - 支持多种执行模式                                              │
│                                                                  │
│  执行模式：                                                      │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ 1. graph-first: Graph → Agent (当前模式)                  │   │
│  │ 2. agent-first: Agent → Graph (新增模式)                  │   │
│  │ 3. parallel: Agent ∥ Graph (并行模式)                     │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### 执行模式详解

#### 1. Graph-First 模式（当前支持）

Graph 作为主编排器，Agent Loop 作为节点执行器。

```
用户请求 → Graph Workflow → AGENT_LOOP 节点 → AgentLoopCoordinator
                                    ↓
                              其他节点类型
```

**适用场景**：
- 预定义的工作流程
- 需要复杂的状态管理
- 多步骤编排任务

**实现方式**：
- 通过 `AGENT_LOOP` 节点类型
- `agent-loop-handler.ts` 调用 `AgentLoopCoordinator`

#### 2. Agent-First 模式（新增）

Agent Loop 作为主控制器，通过 Tool 调用创建/执行 Graph Workflow。

```
用户请求 → Agent Loop → Tool: execute_workflow → Graph Workflow
                  ↓
            其他 Tool 调用
```

**适用场景**：
- Agent 根据用户意图动态选择执行哪个工作流
- 需要灵活决策的复杂任务
- 文档处理、数据分析等动态编排场景

**实现方式**：
- 创建内置工具 `execute_workflow`
- Agent 通过 LLM Tool 调用机制执行工作流

#### 3. Parallel 模式（未来扩展）

Agent 和 Graph 独立运行，通过消息队列/事件总线通信。

```
用户请求 → ┌→ Agent Loop (独立运行)
           └→ Graph Workflow (独立运行)
                    ↓
              Event Bus / Message Queue
```

**适用场景**：
- 需要真正并行执行的任务
- 长时间运行的后台任务
- 事件驱动的响应式系统

## 实现方案

### Phase 1: 内置工具系统

#### 1.1 扩展 ToolType

```typescript
// packages/types/src/tool/state.ts
export type ToolType =
  | "STATELESS"
  | "STATEFUL"
  | "REST"
  | "MCP"
  | "BUILTIN";  // 新增
```

#### 1.2 定义 BuiltinToolConfig

```typescript
// packages/types/src/tool/tool-config.ts
export interface BuiltinToolConfig {
  /** 内置工具名称 */
  name: string;
  /** 执行函数 */
  execute: (
    parameters: Record<string, any>,
    context: BuiltinToolExecutionContext
  ) => Promise<any>;
}

export interface BuiltinToolExecutionContext {
  /** 当前线程 ID */
  threadId?: string;
  /** 父线程实体 */
  parentThreadEntity?: ThreadEntity;
  /** 线程注册表 */
  threadRegistry?: ThreadRegistry;
  /** 事件管理器 */
  eventManager?: EventManager;
  /** 线程构建器 */
  threadBuilder?: ThreadBuilder;
  /** 任务队列管理器 */
  taskQueueManager?: TaskQueueManager;
}
```

#### 1.3 实现 BuiltinExecutor

```typescript
// packages/tool-executors/src/builtin/BuiltinExecutor.ts
export class BuiltinExecutor extends BaseExecutor {
  protected async doExecute(
    tool: Tool,
    parameters: Record<string, any>,
    threadId?: string,
    context?: BuiltinToolExecutionContext,
  ): Promise<any> {
    const config = tool.config as BuiltinToolConfig;
    return config.execute(parameters, context);
  }
}
```

### Phase 2: 工作流执行工具

#### 2.1 创建 execute_workflow 工具

```typescript
// sdk/core/builtins/tools/workflow-tools.ts
import { z } from "zod";
import type { Tool, BuiltinToolConfig, BuiltinToolExecutionContext } from "@modular-agent/types";

/**
 * 执行工作流工具的参数 Schema
 */
const ExecuteWorkflowParametersSchema = z.object({
  workflowId: z.string().describe("要执行的工作流 ID"),
  input: z.record(z.any()).optional().describe("工作流输入参数"),
  waitForCompletion: z.boolean().default(true).describe("是否等待完成"),
  timeout: z.number().optional().describe("超时时间（毫秒）"),
});

/**
 * execute_workflow 内置工具
 */
export const executeWorkflowTool: Tool = {
  id: "builtin_execute_workflow",
  name: "execute_workflow",
  type: "BUILTIN",
  description: "Execute a graph workflow dynamically. Use this tool to run a predefined workflow with the given input parameters.",
  parameters: ExecuteWorkflowParametersSchema,
  config: {
    name: "execute_workflow",
    execute: async (params, context: BuiltinToolExecutionContext) => {
      const { workflowId, input = {}, waitForCompletion = true, timeout } = params;

      // 获取 TriggeredSubworkflowManager
      const triggeredSubworkflowManager = getContainer().get(Identifiers.TriggeredSubworkflowManager);

      // 构建执行请求
      const task: TriggeredSubgraphTask = {
        subgraphId: workflowId,
        input,
        mainThreadEntity: context.parentThreadEntity!,
        triggerId: `builtin-${Date.now()}`,
        config: {
          waitForCompletion,
          timeout,
        },
      };

      // 执行工作流
      const result = await triggeredSubworkflowManager.executeTriggeredSubgraph(task);

      // 处理结果
      if ("threadEntity" in result) {
        // 同步执行完成
        return {
          success: true,
          status: "completed",
          output: result.threadEntity.getOutput(),
          executionTime: result.executionTime,
        };
      } else {
        // 异步执行提交
        return {
          success: true,
          status: "submitted",
          threadId: result.threadId,
          taskId: result.taskId,
        };
      }
    },
  } satisfies BuiltinToolConfig,
};
```

#### 2.2 注册内置工具

```typescript
// sdk/core/services/builtin-tool-registry.ts
export class BuiltinToolRegistry {
  private tools: Map<string, Tool> = new Map();

  constructor() {
    // 注册内置工具
    this.register(executeWorkflowTool);
    // 未来可添加更多内置工具
    // this.register(queryWorkflowStatusTool);
    // this.register(cancelWorkflowTool);
  }

  register(tool: Tool): void {
    this.tools.set(tool.name, tool);
  }

  get(name: string): Tool | undefined {
    return this.tools.get(name);
  }

  getAll(): Tool[] {
    return Array.from(this.tools.values());
  }
}
```

### Phase 3: ToolService 集成

#### 3.1 扩展 ToolService

```typescript
// sdk/core/services/tool-service.ts
export class ToolService {
  private builtinExecutor: BuiltinExecutor;
  private builtinToolRegistry: BuiltinToolRegistry;

  constructor() {
    this.builtinExecutor = new BuiltinExecutor();
    this.builtinToolRegistry = new BuiltinToolRegistry();
  }

  async executeTool(
    tool: Tool,
    parameters: Record<string, any>,
    context: ToolExecutionContext,
  ): Promise<any> {
    switch (tool.type) {
      case "BUILTIN":
        return this.builtinExecutor.execute(tool, parameters, context.threadId, {
          threadId: context.threadId,
          parentThreadEntity: context.parentThreadEntity,
          threadRegistry: context.threadRegistry,
          eventManager: context.eventManager,
          threadBuilder: context.threadBuilder,
          taskQueueManager: context.taskQueueManager,
        });

      // 其他类型...
      default:
        // 现有逻辑
    }
  }

  /**
   * 获取所有可用工具（包括内置工具）
   */
  getAvailableTools(customTools: Tool[]): Tool[] {
    return [...customTools, ...this.builtinToolRegistry.getAll()];
  }
}
```

#### 3.2 Agent 配置集成

```typescript
// 使用示例
const agentConfig: AgentLoopConfig = {
  profileId: "my-agent",
  tools: [
    // 用户定义的工具
    ...userDefinedTools,
    // 内置工具会自动添加
  ],
  maxIterations: 10,
};

// ToolService 会自动合并内置工具
const availableTools = toolService.getAvailableTools(agentConfig.tools);
```

## 使用示例

### 示例 1: Agent 动态选择工作流

```typescript
// Agent 配置
const agent = await coordinator.execute({
  profileId: "orchestrator-agent",
  systemPrompt: `You are an orchestrator agent. Based on the user's request,
    decide which workflow to execute using the execute_workflow tool.

    Available workflows:
    - document-analysis: Analyze documents and extract insights
    - data-processing: Process and transform data
    - report-generation: Generate reports from data`,
  tools: [], // execute_workflow 会自动添加
  maxIterations: 5,
});

// Agent 会根据用户输入自动选择并执行相应的工作流
// 例如：用户说"分析这份文档"，Agent 会调用 execute_workflow({ workflowId: "document-analysis", ... })
```

### 示例 2: 嵌套工作流执行

```typescript
// 在一个工作流中，Agent 可以启动另一个工作流
const mainWorkflow = {
  id: "main-workflow",
  nodes: [
    { id: "start", type: "START" },
    {
      id: "agent",
      type: "AGENT_LOOP",
      config: {
        profileId: "coordinator",
        // Agent 可以通过 execute_workflow 工具启动子工作流
      }
    },
    { id: "end", type: "END" },
  ],
};
```

### 示例 3: 异步工作流执行

```typescript
// Agent 启动异步工作流并继续执行
const result = await coordinator.execute({
  profileId: "async-orchestrator",
  systemPrompt: "Start background tasks and continue processing",
  tools: [],
});

// Agent 调用 execute_workflow({ workflowId: "background-task", waitForCompletion: false })
// 返回 { status: "submitted", threadId: "xxx" }
// Agent 可以继续执行其他任务
```

## 架构优势

### 1. 概念清晰

- 不引入新的概念（如 Dynamic Thread）
- 复用现有的 `TriggeredSubworkflowManager` 机制
- Agent 通过 Tool 调用工作流，符合 LLM 工具调用范式

### 2. 模块独立

- Graph 模块保持独立，不依赖 Agent
- Agent 模块通过 Tool 接口调用 Graph
- 清晰的模块边界和职责划分

### 3. 灵活扩展

- 支持多种执行模式
- 易于添加新的内置工具
- 支持同步和异步执行

### 4. 状态管理

- 复用现有的状态管理机制
- 支持父子关系和级联操作
- 统一的事件系统

## 实现路线图

| 阶段 | 内容 | 优先级 |
|------|------|--------|
| Phase 1 | 内置工具基础设施（类型定义、BuiltinExecutor） | 高 |
| Phase 2 | execute_workflow 工具实现 | 高 |
| Phase 3 | ToolService 集成 | 高 |
| Phase 4 | 更多内置工具（query_status, cancel 等） | 中 |
| Phase 5 | Parallel 模式支持 | 低 |

## 相关文档

- [Agent Loop 架构](./agent-loop-architecture.md)
- [Dynamic Tools 分离分析](./dynamicTools-separation-analysis.md)
- [Trigger 模块功能清单](../sdk/core/trigger/功能清单.md)
- [Graph 模块功能清单](../sdk/graph/功能清单.md)

## 变更历史

- **2026-04-06**: 初始版本，设计 Agent-Graph 统一协调架构
