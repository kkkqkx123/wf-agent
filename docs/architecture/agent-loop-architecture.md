# Agent Loop 架构分析与实施方案

## 一、背景

当前SDK以图工作流为唯一核心实现，所有功能都依赖Thread/Workflow概念。参考Lim Code的架构，需要引入独立的Agent循环能力，以支持：
- 简单任务的直接执行（无需预定义工作流）
- VSCode-app等应用的基础Agent能力
- 主协调引擎按需调用子工作流的模式

## 二、当前架构分析

### 2.1 SDK架构现状

```
sdk/
├── api/                    # API层
│   ├── commands/          # 命令（都依赖Thread）
│   ├── queries/           # 查询
│   └── resources/         # 资源管理API
├── core/                   # 核心层
│   ├── execution/         # 执行引擎（完全基于图）
│   │   ├── coordinators/  # 协调器
│   │   ├── handlers/      # 节点处理器
│   │   └── managers/      # 管理器
│   └── services/          # 服务
└── index.ts               # 入口
```

### 2.2 核心问题

| 问题 | 说明 |
|------|------|
| 高耦合 | 所有执行逻辑都依赖Thread/Workflow/Graph |
| 高门槛 | 使用任何功能都必须先创建工作流和线程 |
| 无独立Agent | 无法像Lim Code那样直接执行Agent循环 |
| 复用困难 | 核心逻辑与图编排强绑定，难以单独使用 |

## 三、目标架构

### 3.1 双模式架构设计

```
┌─────────────────────────────────────────────────────────────┐
│                     目标架构                                 │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────────────────┐    ┌─────────────────────────────┐ │
│  │   Agent SDK (新增)   │    │   Graph SDK (现有)          │ │
│  │   ---------------   │    │   ----------------          │ │
│  │   AgentLoopService  │◄───┤   AGENT_LOOP节点            │ │
│  │   LLMService        │    │   Thread/Workflow           │ │
│  │   ToolService       │    │   图编排                     │ │
│  │                     │    │                             │ │
│  │   不依赖Thread       │    │   可调用Agent SDK           │ │
│  │   直接执行Agent循环  │    │   作为主协调器              │ │
│  └─────────────────────┘    └─────────────────────────────┘ │
│           │                            │                    │
│           └────────────┬───────────────┘                    │
│                        ▼                                    │
│              ┌──────────────────┐                          │
│              │  Common Utils    │                          │
│              │  - LLM客户端抽象  │                          │
│              │  - 工具执行基础   │                          │
│              │  - 消息类型定义   │                          │
│              └──────────────────┘                          │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 架构层次

| 层次 | 职责 | 当前状态 | 目标状态 |
|------|------|----------|----------|
| **Common Utils** | 基础抽象和工具 | 只有通用工具函数 | 增加LLM/Tool/Message基础抽象 |
| **Agent SDK** | 独立Agent能力 | 无 | 新增，提供AgentLoopService |
| **Graph SDK** | 图工作流编排 | 完整实现 | 复用Agent SDK作为节点 |
| **Apps** | 应用实现 | CLI-APP | VSCode-app等可直接使用Agent SDK |

## 五、SDK拆分方案分析

### 5.1 方案对比

| 方案 | 说明 | 优点 | 缺点 |
|------|------|------|------|
| **A. 不拆分** | 在现有sdk中增加AgentLoopService | 简单，无破坏性变更 | SDK臃肿，概念混杂 |
| **B. 完全拆分** | agent-sdk + graph-sdk 两个包 | 职责清晰，可独立使用 | 维护成本高，有重复代码 |
| **C. 内部分层** (推荐) | 一个sdk包，内部划分agent/graph模块 | 平衡，渐进式演进 | 需要良好的模块划分 |

### 5.2 推荐方案：C. 内部分层

保持单一SDK包，但内部明确分层：

```
sdk/
├── agent/                  # 新增：Agent基础能力
│   ├── agent-loop-service.ts
│   ├── llm-service.ts
│   └── tool-service.ts
├── graph/                  # 现有执行层改名
│   ├── execution/
│   ├── coordinators/
│   └── handlers/
├── api/                    # API层（保持不变）
│   ├── commands/
│   └── resources/
└── index.ts
```

**理由**：
1. **渐进式演进**：无需一次性重构整个SDK
2. **向后兼容**：现有代码无需修改
3. **维护简单**：一个包，但内部职责清晰
4. **灵活组合**：Agent和Graph可以相互调用

### 5.3 依赖关系

```
┌──────────────────────────────────────┐
│           API Layer                  │
│  (commands, queries, resources)      │
└──────────────┬───────────────────────┘
               │
       ┌───────┴───────┐
       ▼               ▼
┌──────────────┐ ┌──────────────┐
│  Agent Layer │ │  Graph Layer │
│  ----------- │ │  ----------- │
│  无外部依赖   │ │  依赖Agent   │
└──────────────┘ └──────────────┘
       │               │
       └───────┬───────┘
               ▼
┌──────────────────────────────────────┐
│        packages/types                │
│        packages/common-utils         │
└──────────────────────────────────────┘
```

## 六、实施路线图

### Phase 1: 基础类型准备（1-2周）

1. 在`packages/common-utils`中添加LLM基础类型
2. 定义`BaseLLMClient`接口
3. 确保Agent层和Graph层可以共享这些类型

### Phase 2: Agent层实现（2-3周）

1. 创建`sdk/agent/`目录
2. 实现`AgentLoopService`
3. 实现`AgentLLMService`（简化版LLM调用）
4. 实现`AgentToolService`（简化版工具执行）
5. 提供流式和非流式API

### Phase 3: Graph层集成（1-2周）

1. 重构`AGENT_LOOP`节点处理器
2. 使用`AgentLoopService`作为执行逻辑
3. 确保Graph层可以调用Agent层

### Phase 4: API层暴露（1周）

1. 在`sdk/api/`中暴露Agent相关API
2. 创建`AgentCommand`、`AgentQuery`
3. 更新SDK入口导出

### Phase 5: 文档和示例（1周）

1. 编写Agent SDK使用文档
2. 创建示例代码
3. 更新架构文档

## 七、关键设计决策

### 7.1 AgentLoopService 接口设计

```typescript
// sdk/agent/agent-loop-service.ts

export interface AgentLoopConfig {
  profileId: string;
  systemPrompt?: string;
  maxIterations?: number;
  tools?: string[];
  initialMessages?: LLMMessage[];
}

export interface AgentLoopResult {
  success: boolean;
  content?: string;
  iterations: number;
  toolCalls: AgentToolCallRecord[];
  error?: Error;
}

export class AgentLoopService {
  constructor(deps: AgentLoopDependencies);

  // 非流式执行
  async run(config: AgentLoopConfig): Promise<AgentLoopResult>;

  // 流式执行
  async *runStream(config: AgentLoopConfig): AsyncGenerator<AgentStreamEvent>;

  // 支持对话历史
  async continue(
    conversationId: string,
    userInput: string
  ): Promise<AgentLoopResult>;
}
```

### 7.2 与Graph层的集成点

```typescript
// sdk/graph/handlers/node-handlers/agent-loop-handler.ts

export async function agentLoopHandler(
  thread: Thread,
  node: Node,
  context: HandlerContext
): Promise<HandlerResult> {
  // 使用Agent层的Service
  const agentService = context.agentLoopService;

  const result = await agentService.run({
    profileId: config.profileId,
    systemPrompt: config.systemPrompt,
    maxIterations: config.maxIterations,
    tools: config.tools,
    initialMessages: buildMessagesFromThread(thread)
  });

  // 将结果写回Thread
  thread.variables.push({
    name: 'output',
    value: result.content
  });

  return { status: 'COMPLETED' };
}
```

## 八、风险和对策

| 风险 | 影响 | 对策 |
|------|------|------|
| 重构引入bug | 高 | 保持向后兼容，逐步替换 |
| 概念混淆 | 中 | 清晰的文档和命名规范 |
| 性能下降 | 低 | 保持轻量级设计，避免过度抽象 |
| 维护成本增加 | 中 | 内部分层而非完全拆分 |

## 九、总结

### 9.1 核心结论

1. **不拆分SDK包**，采用内部分层
2. **部分提取**LLM基础类型到common-utils
3. **新增Agent层**，提供独立的Agent循环能力
4. **Graph层依赖Agent层**，AGENT_LOOP节点使用AgentLoopService

### 9.2 预期收益

- ✅ 低门槛使用Agent能力（直接调用AgentLoopService）
- ✅ 支持VSCode-app等应用的基础Agent需求
- ✅ 图工作流可作为主协调器，按需调用子工作流
- ✅ 向后兼容，现有代码无需修改
- ✅ 架构清晰，职责分离

### 9.3 下一步行动

1. 评审此架构方案
2. 确定Phase 1的具体任务
3. 分配开发资源
4. 开始实施
