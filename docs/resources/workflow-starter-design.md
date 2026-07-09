# Workflow Starter — 组件化注册方案设计

## 1. 背景分析：当前 Workflow 定义逻辑

### 1.1 定义模式

当前一个完整的工作流应用需要**离散地准备 5 类资源**，分别注册到各自的 Registry：

| 资源类型 | 接口/配置位置 | 注册目标 |
|---------|-------------|---------|
| Workflow 定义 | `configs/workflows/*.toml` | `WorkflowRegistry` |
| Node 模板 | `configs/node-templates/*.toml` | `NodeTemplateRegistry` |
| Trigger 模板 | `configs/trigger-templates/*.toml` | `TriggerTemplateRegistry` |
| Agent Loop 配置 | `configs/agent-loops/*.toml` | `AgentLoopRegistry` |
| Hook 模板 | `configs/hook-templates/*.toml` | `HookTemplateRegistry` |

注册流程：`config-processor` 扫描 `index.json` → 解析 TOML → 写入各 Registry。

**实际部署场景** 中，"代码审查 + 修改"这样一个完整能力，需要同时准备 workflow、agent_loop、trigger_template、node_template、prompt_template 多个文件，并在多个 index.json 中注册路径。这种分散式的管理在单个原型验证阶段尚可接受，但在以下场景成为问题：

1. 需要将一个完整的工作流能力**打包分发**给多个项目使用
2. 需要让非核心开发者仅通过**少量配置**就能接入一个复杂工作流
3. 需要在运行时根据环境**动态调整**行为（如结束条件、LLM 型号）

### 1.2 Loop + Agent 端条件模式的核心需求

代码审查 + 修改工作流的本质是一个**二阶段 Agent 协作循环**：

```
START (注入初始提示词 default)
  → LOOP_START
    → LLM Node (根据 default + 历史，确定本轮任务)
      → Executor AGENT_LOOP (执行代码修改任务)
        → Reviewer AGENT_LOOP (审查修改结果)
          → LOOP_END (检查 complete 变量)
            → 满足 → END
            → 不满足 → 回到 LLM Node
```

当前框架提供了 LOOP_START/LOOP_END（`loop-configs.ts`）和 AGENT_LOOP（`agent-loop-configs.ts`）节点类型，但缺乏**将这三者编排为一个可复用单元**的机制。用户需要手动构建 node + edge 的完整图结构，并且在 LOOP_END 的 `breakCondition` 中硬编码表达式。

此外，现有 `attempt_completion` 仅能返回文本 `result`，无法操作工作流变量和 data 数组。审查者 Agent 需要一种方式，在一次工具调用中通过统一 `data` 对象完成变量设置和数组追加，而不依赖额外的 `set_variable` 工具。

---

## 2. Starter 模式设计

### 2.1 核心概念

**Starter（启动器）** 是一个自描述的组件包，它将一个完整工作流所需的所有资源打包为一个单元，对外暴露有限的可配置选项，通过 `assemble(config) → bundle` 方法组装出完整的 `WorkflowBundle`。

```
WorkflowStarter  (代码实现)
  │  assemble(config)
  ▼
WorkflowBundle  (纯数据)
  ├─ workflow: WorkflowTemplate           (必须)
  ├─ agentLoops: AgentLoopDefinition[]    (包含执行者 + 审查者两个 Agent)
  ├─ nodeTemplates: NodeTemplate[]        (模板节点)
  ├─ triggerTemplates: TriggerTemplate[]  (触发器模板)
  ├─ hookTemplates: HookTemplate[]        (钩子模板)
  └─ promptTemplates: PromptTemplate[]    (提示词模板)
```

### 2.2 WorkflowStarter 接口定义

```typescript
// packages/sdk-kit/src/starter/types.ts

/**
 * Starter 对外暴露的可配置选项描述
 */
export interface StarterConfigField {
  type: "string" | "number" | "boolean" | "expression" | "array" | "object";
  default?: unknown;
  description: string;
  required?: boolean;
  /** 如 type=expression, 可指定允许的 DSL 函数名 */
  allowedFunctions?: string[];
}

/**
 * Starter 的元描述
 */
export interface StarterMetadata {
  id: string;
  name: string;
  version: string;
  description: string;
  author?: string;
  tags?: string[];
  category?: string;
  /** 该 starter 依赖的外部资源（如 LLM profile 名称） */
  dependencies?: string[];
  /** 可配置的字段列表 */
  configurable?: Record<string, StarterConfigField>;
}

/**
 * Starter 组装出的完整资源包
 */
export interface WorkflowBundle {
  workflow: WorkflowTemplate;
  agentLoops?: AgentLoopDefinition[];
  nodeTemplates?: NodeTemplate[];
  triggerTemplates?: TriggerTemplate[];
  hookTemplates?: HookTemplate[];
  promptTemplates?: PromptTemplate[];
}

/**
 * Starter 生命周期接口
 */
export interface WorkflowStarter<C extends Record<string, unknown> = Record<string, unknown>> {
  readonly metadata: StarterMetadata;

  /** 核心方法：将 config 组装为完整的资源包 */
  assemble(config: C): WorkflowBundle;

  /** 生命周期钩子 */
  onBeforeAssemble?(config: C): MaybePromise<void>;
  onAfterInstall?(bundle: WorkflowBundle): MaybePromise<void>;
  onBeforeUninstall?(): MaybePromise<void>;
  onAfterUninstall?(): MaybePromise<void>;
}
```

### 2.3 StarterRegistry

```typescript
// packages/sdk-kit/src/starter/starter-registry.ts

export class StarterRegistry {
  private starters = new Map<string, WorkflowStarter>();

  register(starter: WorkflowStarter): void;
  unregister(id: string): void;
  get(id: string): WorkflowStarter | undefined;
  list(): WorkflowStarter[];

  async activate(id: string, config: Record<string, unknown>,
    registries: { workflowRegistry; triggerTemplateRegistry; nodeTemplateRegistry;
      agentLoopRegistry; hookTemplateRegistry; promptTemplateRegistry }
  ): Promise<WorkflowBundle>;

  async deactivate(id: string, registries: {...}): Promise<void>;
}
```

`activate` 内部流程：

```
activate(id, config, registries)
  1. starter = starters.get(id)
  2. starter.onBeforeAssemble?.(config)
  3. bundle = starter.assemble(config)
  4. registries.workflowRegistry.register(bundle.workflow)
  5. bundle.agentLoops?.forEach(al => registries.agentLoopRegistry.register(al))
  6-9. 相同模式注册 nodeTemplates, triggerTemplates, hookTemplates, promptTemplates
  10. starter.onAfterInstall?.(bundle)
```

---

## 3. 代码审查 + 修改 Starter 详细设计

### 3.1 工作流完整流程

下图为一次循环迭代的消息和数据流动：

```
┌──────────────────────────────────────────────────────────────────┐
│  Variable: default = [system_prompt, user_prompt_1, ...]        │
│  Variable: complete = false                                      │
│  Variable: judges = []                                           │
│  MessageContext: default-context = [workflow 入口初始消息数组]    │
└──────────────────────────────────────────────────────────────────┘

START (通过 messageContextRegistry 注入初始提示词到 "default" 上下文)
  │
  ▼
LOOP_START (进入循环，暴露 maxIterations)
  │
  ▼
LLM Node (任务分发器)
  │  config:
  │    profileId = "gpt-4o-mini"   (轻量模型，仅做任务分发)
  │    输入提示词 =
  │      - 从 "default" 上下文获取当前累积的对话历史
  │      - 从变量 "complete"、"judges" 获取审查结果
  │      - 最新一条 assistant 消息(如果有)作为本轮剩余工作的锚点
  │    输出: 一个明确的 task description 字符串
  │
  ▼
Executor AGENT_LOOP (执行 Agent)
  │  config:
  │    profileId = <用户配置>
  │    messageInputs:
  │      - externalName: "default"       # 从 workflow 的 "default" 上下文获取所有消息
  │        internalName: "system-context"
  │    dataInputs:
  │      - parentField: "currentTask"    # 从 workflow input.data 获取 LLM Node 输出的任务描述
  │        internalName: "task_description"
  │
  │  内部逻辑:
  │    1. collectInitialMessages(): 从 "system-context" 拿到全部消息
  │    2. 截取消息数组的**最后一条 assistant 消息**作为本轮输入 prompt
  │       (如果没有 assistant 消息，使用 default-context 的第一条 user 消息)
  │    3. 执行内部 Agent 循环:
  │       读取/修改文件 -> 运行验证 -> 工具调用
  │       某一刻 LLM 决定调用 attempt_completion(result, data?) 结束
  │    4. 将最终消息写回 "system-context"
  │    5. 通过 messageOutputs 将完整对话同步回 workflow 的 "default" 上下文
  │
  ▼
(通过 messageOutputs, executor 的对话同步回 "default" 上下文)

Reviewer AGENT_LOOP (审查 Agent)
  │  config:
  │    profileId = <用户配置>  (可以比执行者更强大的模型)
  │    messageInputs:
  │      - externalName: "default"
  │        internalName: "review-context"
  │    dataInputs:
  │      - parentField: "judges"
  │        internalName: "previous_judges"
  │
  │  内部逻辑:
  │    1. collectInitialMessages(): 从 "review-context" 拿到本轮执行者修改后的对话
  │    2. 注入审查系统提示词 (用户配置的 reviewer system prompt)
  │    3. 注入之前的 judges 数组作为参考
  │    4. Agent 审查代码修改
  │    5. 调用增强版 attempt_completion:
  │       attempt_completion({
  │         result: "Review summary...",
  │         variables: { complete: true/false },
  │         data: { judges: [{ id, file, score, comment }] }
  │       })
  │       此工具调用同时:
  │       - 设置 workflow 变量 complete (boolean)
  │       - 追加一条 judge 记录到 workflow 变量 judges 数组
  │       - 返回 result 文本
  │    6. 尝试从 attempt_completion 返回中截取最后一条 assistant 消息
  │       (包含 review summary)，通过 messageOutputs 追加到 "default" 上下文
  │
  ▼
LOOP_END
  │  config:
  │    loopId = "review-loop"
  │    breakCondition: "variables.complete === true || iterationCount >= maxIterations"
  │    loopStartNodeId: "task_planner"
  │
  ├─ breakCondition = true  → END
  │
  └─ breakCondition = false → 回到 LLM Node (下一轮)
                              LLM Node 读取更新后的 "default" 上下文
                              (包含 executor 本轮全部对话 + reviewer 审查总结)
                              以及更新后的 judges 数组，决定下一轮任务
```

### 3.2 涉及的工作流变量

| 变量名 | 类型 | 用途 | 设置者 |
|--------|------|------|--------|
| `complete` | boolean | 是否结束循环 | Reviewer 的 attempt_completion.variables.complete |
| `judges` | object[] | 历次审查记录 | Reviewer 的 attempt_completion.data.judges，自动 append |
| - | - | 每条 judge 结构: `{ iteration, file, score, comment }` | - |

### 3.3 涉及的 MessageContext

| Context ID | 用途 | 内容 |
|-----------|------|------|
| `default` | 整个工作流的对话上下文 | 初始提示词 + 每轮 executor 的完整对话 + 每轮 reviewer 的审查总结 |

`default` 上下文的累积过程：

```
初始: [system_prompt, user_initial_input]
第1轮 executor 完成 → 追加 executor 全部对话消息
第1轮 reviewer 完成 → 追加 reviewer 最后一条 assistant 消息
第2轮 executor 开始 → 从 default 拿到累积消息，截取最后一条 assistant 消息
...
```

### 3.4 Starter 配置项

```toml
# user-config.toml — 用户只需提供这些

starterId = "@standard/code-review-agent"

# === 核心配置 ===
maxIterations = 10

# === 任务分发 LLM 配置（轻量模型，确定本轮任务） ===
plannerProfileId = "gpt-4o-mini"
plannerSystemPrompt = """
You are a task planner for code review. Based on the conversation history
and previous review results, determine the next task to execute.
Output a clear, single-sentence task description for the executor.
"""

# === 执行者 Agent 配置 ===
executorProfileId = "gpt-4o"
executorSystemPrompt = """
You are a code review executor with full file access.
"""

# === 审查者 Agent 配置 ===
reviewerProfileId = "o3-mini"
reviewerSystemPrompt = """
You are a strict code reviewer. Review the changes made by the executor.
If the changes are satisfactory, call attempt_completion with complete=true.
Otherwise, provide detailed feedback and call attempt_completion with complete=false.
Each review must attach a judge record with score (1-10) and actionable comments.
"""

# === 可用工具 ===
availableTools = ["readFile", "writeFile", "editFile", "glob", "grep", "bash"]

# === 初始提示词（作为 workflow 入口消息，注入 "default" 上下文） ===
[[initialPrompts]]
role = "system"
content = "You are a code review assistant..."

[[initialPrompts]]
role = "user"
content = "Please review the code at {{targetPath}}..."
```

### 3.5 Starter 实现

```typescript
// packages/sdk-kit/src/starters/code-review-agent.starter.ts

interface CodeReviewConfig {
  maxIterations: number;
  plannerProfileId: string;
  plannerSystemPrompt: string;
  executorProfileId: string;
  executorSystemPrompt: string;
  reviewerProfileId: string;
  reviewerSystemPrompt: string;
  availableTools: string[];
  initialPrompts?: LLMMessage[];
}

export class CodeReviewAgentStarter implements WorkflowStarter<CodeReviewConfig> {
  readonly metadata: StarterMetadata = {
    id: "@standard/code-review-agent",
    name: "Code Review Agent",
    version: "1.0.0",
    description: "Dual-agent loop: executor + reviewer with end-condition check",
    configurable: {
      maxIterations: { type: "number", default: 10 },
      plannersProfileId: { type: "string", default: "gpt-4o-mini" },
      executorProfileId: { type: "string", default: "gpt-4o" },
      reviewerProfileId: { type: "string", default: "o3-mini" },
      // ... 其他字段
    },
  };

  assemble(config: CodeReviewConfig): WorkflowBundle {
    return {
      workflow: this.buildWorkflow(config),
      agentLoops: [
        this.buildExecutorAgent(config),
        this.buildReviewerAgent(config),
      ],
      promptTemplates: [
        this.buildPlannerPrompt(config),
      ],
    };
  }

  private buildWorkflow(config: CodeReviewConfig): WorkflowTemplate {
    return {
      id: "@standard/code-review-agent-workflow",
      name: "Code Review Agent Workflow",
      type: "STANDALONE",
      version: "1.0.0",
      // === Variables ===
      variables: [
        { name: "complete", type: "boolean", value: false,
          metadata: { description: "Loop exit flag, set by reviewer agent" }},
        { name: "judges", type: "array", value: [],
          metadata: { description: "Review judgment records, appended each iteration" }},
        { name: "iterationCount", type: "number", value: 0,
          metadata: { description: "Current iteration counter" }},
      ],
      // === Nodes ===
      nodes: [
        // START: registers initial prompts into "default" message context
        {
          id: "start",
          type: "START",
          config: {
            messageInputs: [{
              externalName: "workflow-input",   // from caller
              internalName: "default",
              defaultMessages: config.initialPrompts,
            }],
            variableInputs: [{
              externalName: "targetPath",
              internalName: "targetPath",
              required: true,
            }],
          },
        },
        // LOOP_START
        {
          id: "loop_start",
          type: "LOOP_START",
          config: {
            loopId: "review-loop",
            maxIterations: config.maxIterations,
            variableInputs: [
              { externalName: "complete", internalName: "complete" },
              { externalName: "judges", internalName: "judges" },
              { externalName: "targetPath", internalName: "targetPath" },
            ],
          },
        },
        // LLM Node: Task Planner — reads default context, outputs current task
        {
          id: "task_planner",
          type: "LLM",
          config: {
            profileId: config.plannerProfileId,
            prompt: config.plannerSystemPrompt,
            // Planner receives:
            //   - "default" message context (accumulated history)
            //   - judges array (previous reviews)
            //   - current complete variable
            inputs: {
              context: "$ref:messageContext.default",
              judges: "$var:judges",
              complete: "$var:complete",
            },
          },
        },
        // Executor AGENT_LOOP
        {
          id: "executor_agent",
          type: "AGENT_LOOP",
          config: {
            agentLoopId: "@standard/code-review-agent-executor",
            inlineConfig: {
              messageInputs: [{
                externalName: "default",
                internalName: "system-context",
              }],
              dataInputs: [{
                parentField: "currentTask",
                internalName: "task_description",
              }],
              messageOutputs: [{
                internalName: "system-context",
                externalName: "default",
              }],
            },
          },
        },
        // Reviewer AGENT_LOOP
        {
          id: "reviewer_agent",
          type: "AGENT_LOOP",
          config: {
            agentLoopId: "@standard/code-review-agent-reviewer",
            inlineConfig: {
              messageInputs: [{
                externalName: "default",
                internalName: "review-context",
              }],
              dataInputs: [
                { parentField: "judges", internalName: "previous_judges" },
              ],
              messageOutputs: [{
                internalName: "review-context",
                externalName: "default",
              }],
            },
          },
        },
        // LOOP_END
        {
          id: "loop_end",
          type: "LOOP_END",
          config: {
            loopId: "review-loop",
            breakCondition: {
              type: "expression",
              expression: "variables.complete === true",
            },
            loopStartNodeId: "task_planner",
          },
        },
        { id: "end", type: "END", config: {} },
      ],
      // === Edges ===
      edges: [
        { id: "e0", sourceNodeId: "__start__", targetNodeId: "start", type: "DEFAULT" },
        { id: "e1", sourceNodeId: "start",   targetNodeId: "loop_start", type: "DEFAULT" },
        { id: "e2", sourceNodeId: "loop_start", targetNodeId: "task_planner", type: "DEFAULT" },
        { id: "e3", sourceNodeId: "task_planner", targetNodeId: "executor_agent", type: "DEFAULT" },
        { id: "e4", sourceNodeId: "executor_agent", targetNodeId: "reviewer_agent", type: "DEFAULT" },
        { id: "e5", sourceNodeId: "reviewer_agent", targetNodeId: "loop_end", type: "DEFAULT" },
        { id: "e6", sourceNodeId: "loop_end", targetNodeId: "end", type: "DEFAULT" },
        { id: "e7", sourceNodeId: "loop_end", targetNodeId: "task_planner", type: "CONDITIONAL",
          condition: { type: "expression", expression: "!hasMoreIterations || !breakTriggered" } },
      ],
      config: {
        timeout: 600000,
        enableCheckpoint: true,
        maxRetries: 2,
      },
    };
  }

  private buildExecutorAgent(config: CodeReviewConfig): AgentLoopDefinition {
    return {
      id: "@standard/code-review-agent-executor",
      name: "Code Review Executor",
      profileId: config.executorProfileId,
      systemPrompt: config.executorSystemPrompt,
      maxIterations: 30,
      availableTools: { tools: config.availableTools },
      checkpoint: { createOnEnd: true, createOnError: true },
    };
  }

  private buildReviewerAgent(config: CodeReviewConfig): AgentLoopDefinition {
    return {
      id: "@standard/code-review-agent-reviewer",
      name: "Code Review Reviewer",
      profileId: config.reviewerProfileId,
      systemPrompt: config.reviewerSystemPrompt,
      maxIterations: 10,
      // Reviewer must have attempt_completion and structured output access
      availableTools: {
        tools: [...config.availableTools, "attempt_completion"],
      },
      checkpoint: { createOnEnd: true, createOnError: true },
    };
  }
}
```

---

## 4. 增强 attempt_completion 工具

### 4.1 当前能力 vs 需求

| 方面 | 当前 ref/roo-code 实现 | 需求 |
|------|----------------------|------|
| 参数 | 仅 `result: string` | `data` + `variables` |
| 变量操作 | 无 | 一次性设置多个变量（通过 `variables`）|
| 数据操作 | 无 | 一次性追加记录到数组变量（通过 `data`）|
| 与工作流集成 | 无 | 工具 metadata 通过 conversation → AgentLoopHandler 映射到 VariableManager |

### 4.2 工具参数结构

**核心原则**：`data`（产物输出）和 `variables`（状态变更）分离，职能清晰不重叠。

| 层级 | 参数 | TypeScript 类型 | 语义 |
|------|------|-----------------|------|
| 产物输出 | `data` | `Record<string, unknown>` | 每个 key 追加一条记录到同名的数组变量 |
| 状态变更 | `variables` | `Record<string, unknown>` | 每个 key-value **设置**（非追加）到同名变量 |

**`data` 追加规则**：
- 单个对象 `{"judges": {...}}` → append 一条记录（推荐用法）
- 数组 `{"judges": [{...}, {...}]}` → batch append 多条记录（少用，复杂批量操作应使用专门工具）
- `result` 文本不单独提供，Agent 的最后一条 assistant 消息天然承担此角色

**为什么不用数组强制类型**：避免 LLM 在"单个对象"与"数组"之间错选。始终传单个对象 → 尾插一条，语义更直观。

### 4.3 模块位置与文件结构

```
packages/sdk/resources/predefined/tools/builtin/attempt-completion/
├── schema.ts        # ToolParameterSchema（JSON Schema）
├── handler.ts       # 处理器（factory pattern，同 ask-followup-question）
├── description.ts   # ToolDescriptionData（LLM 可见的描述）
└── index.ts         # 桶导出

packages/sdk/resources/predefined/tools/builtin/
└── registry.ts      # [修改] 注册 attempt_completion

packages/sdk/agent/execution/coordinators/
└── agent-iteration-coordinator.ts  # [修改] 检测 attempt_completion 后设 shouldContinue=false

packages/sdk/workflow/execution/handlers/node-handlers/
└── agent-loop-handler.ts           # [修改] 添加 syncCompletionChanges()

packages/types/src/tool/
└── execution.ts     # [修改] ToolOutput 增加 metadata 字段
```

### 4.4 Schema 定义

```typescript
// packages/sdk/resources/predefined/tools/builtin/attempt-completion/schema.ts

import type { ToolParameterSchema } from "@wf-agent/types";

export const attemptCompletionSchema: ToolParameterSchema = {
  type: "object",
  properties: {
    data: {
      type: "object",
      description: "Output records. Each value is appended to the matching array variable. Use single objects (not arrays) for one-at-a-time appends.",
      additionalProperties: true,
      optional: true,
    },
    variables: {
      type: "object",
      description: "State changes. Each key-value pair is written directly as a workflow variable (always set, not appended).",
      additionalProperties: true,
      optional: true,
    },
  },
  required: [],
};
```

### 4.5 Handler 实现

采用与 `ask-followup-question` 一致的 factory pattern，非 class 方式：

```typescript
// packages/sdk/resources/predefined/tools/builtin/attempt-completion/handler.ts

import type { ToolOutput, BuiltinToolExecutionContext } from "@wf-agent/types";

export function createAttemptCompletionHandler() {
  return async (
    params: Record<string, unknown>,
    _context: BuiltinToolExecutionContext,
  ): Promise<ToolOutput> => {
    try {
      const { data, variables } = params as {
        data?: Record<string, unknown>;
        variables?: Record<string, unknown>;
      };

      return {
        success: true,
        content: "Task completed successfully.",
        metadata: {
          type: "completion",
          data: data ?? null,
          variables: variables ?? null,
        },
      };
    } catch (error) {
      return {
        success: false,
        content: "",
        error: error instanceof Error ? error.message : String(error),
      };
    }
  };
}
```

关键设计：
- handler 不直接操作 VariableManager——它只将 `data`/`variables` 存入 `ToolOutput.metadata`
- metadata 通过 `MessageBuilder.buildToolMessage()` 序列化到 conversation 的 tool message content 中（JSON string）
- AgentIterationCoordinator 在 tool execution 后提取 metadata，**emit `ATTEMPT_COMPLETION` 事件**（供 TriggerCoordinator 消费）
- 实际变量同步通过 `AgentLoopResult.completionData` 传播到 workflow 层（`AgentLoopHandler.syncCompletionData`）

### 4.6 与 AgentIterationCoordinator 的集成

在 `executeIteration()` 中，执行完 tool calls 后检测是否有 `attempt_completion`：

```typescript
// agent-iteration-coordinator.ts

const toolCallNames = response.toolCalls.map(tc => tc.name);
const hasCompletionTool = toolCallNames.includes("attempt_completion");

const toolResults = await this.toolExecutionCoordinator.executeToolCalls(
  entity, conversationManager, response.toolCalls.map(...),
);

// 1. 提取 completion metadata 并 emit ATTEMPT_COMPLETION 事件
let completionData: CompletionData | undefined;
if (hasCompletionTool) {
  const result = toolResults.find(r => {
    const output = r.result as { metadata?: { type: string } } | undefined;
    return output?.metadata?.type === "completion";
  });
  if (result?.result) {
    const meta = (result.result as any).metadata;
    completionData = { data: meta.data, variables: meta.variables };
  }

  const event = buildAttemptCompletionEvent({
    executionId: entity.id,
    agentLoopId: entity.id,
    nodeId: entity.nodeId,
    content: response.content,
    data: completionData?.data,
    variables: completionData?.variables,
  });
  await this.emitToRegistry(event, entity);  // → TriggerCoordinator 可消费
}

entity.state.endIteration(response.content);
// ... hooks ...

return {
  success: true,
  shouldContinue: !hasCompletionTool,  // ← 关键：有 attempt_completion 就停循环
  content: response.content,
  completionData,  // ← 通过结果传播到 AgentLoopHandler
};
```

这样设计：
- `ToolExecutionCoordinator.executeToolCalls()` 返回 `ToolExecutionResult[]`（含 toolCallId、result、error）
- handler 中 `data`/`variables` 通过 `ToolOutput.metadata` → `ToolExecutionResult.result.metadata` 向上传播
- `ATTEMPT_COMPLETION` 事件通过 `EventRegistry` 发出，供 TriggerCoordinator 匹配触发器
- `completionData` 通过 `AgentLoopResult` 一路传递到 `AgentLoopHandler`

### 4.7 与 AgentLoopHandler 的集成：变量同步回 Workflow

`syncCompletionData` 在 agent loop 执行完毕后调用，从 `AgentLoopResult.completionData`（由 AgentIterationCoordinator 通过结果链传播）同步到 VariableManager：

```typescript
// agent-loop-handler.ts — syncCompletionData

function syncCompletionData(
  completionData: { data?: Record<string, unknown>; variables?: Record<string, unknown> } | undefined,
  variableStateManager?: { setVariable: (name: string, value: unknown) => void; getVariable: (name: string) => unknown },
): void {
  if (!completionData || !variableStateManager) return;

  // data: 追加到数组变量（单值 → append, 数组 → batch append）
  if (completionData.data) {
    for (const [key, value] of Object.entries(completionData.data)) {
      const existing = (variableStateManager.getVariable(key) as unknown[]) || [];
      if (Array.isArray(value)) {
        variableStateManager.setVariable(key, [...existing, ...value]);
      } else {
        variableStateManager.setVariable(key, [...existing, value]);
      }
    }
  }

  // variables: 直接 setVariable
  if (completionData.variables) {
    for (const [key, value] of Object.entries(completionData.variables)) {
      variableStateManager.setVariable(key, value);
    }
  }
}
```

**双层变量模型**：
- agent loop 内部（AgentLoopEntity 状态快照）自由使用变量
- 只有 `attempt_completion` 显式通过 `data`/`variables` 指定的变更会通过 `AgentLoopResult.completionData` → `syncCompletionData` 写回 Workflow 的 VariableManager
- 避免 Agent Loop 的临时变量污染 Workflow 上下文
- 同时 `ATTEMPT_COMPLETION` 事件通过 `EventRegistry` 发出，供 `TriggerCoordinator` 匹配触发器（如 `set_variable`、`stop_workflow_execution`）

**为何从 conversation 扫描改为事件驱动**：
- 旧方案：`extractCompletionMetadata()` 在 agent loop 完成后反向扫描 conversation 中的 tool message → JSON 解析 → 提取 metadata。依赖 `getAllMessages()`、JSON 解析格式。
- 新方案：`AgentIterationCoordinator` 在 tool execution 后立即提取 metadata，emit `ATTEMPT_COMPLETION` 事件，同时通过 `AgentLoopResult.completionData` 原样传递。无需 conversation 扫描。事件可被 `TriggerCoordinator` 匹配触发器。

### 4.8 使用示例

```typescript
// LLM 通过结构化工具调用 attempt_completion

// 审查者完成审查，认为可以通过：
tool_call: {
  name: "attempt_completion",
  arguments: {
    data: {
      "judges": {                                     // 单个对象→ append 一条到 judges[]
        "iteration": 2,
        "file": "src/processor.ts",
        "score": 8,
        "comment": "Good restructuring"
      }
    },
    variables: {
      "complete": true,                               // setVariable("complete", true)
      "summary": "Code review completed.",
    }
  }
}
// → judges: [...现有记录, { iteration:2, file:"src/processor.ts", ... }]
// → complete: false → true
// → shouldContinue: false → agent loop 结束
// → result 文本：由 conversation 最后一条 assistant 消息承担

// 审查者发现还需继续修改：
tool_call: {
  name: "attempt_completion",
  arguments: {
    data: {
      "judges": {
        "iteration": 2,
        "file": "src/fetcher.ts",
        "score": 4,
        "comment": "Missing mutex around shared state access",
      }
    },
    variables: {
      "complete": false,
      "summary": "Issues found: fetch logic is not threadsafe.",
    }
  }
}
// → judges 追加一条
// → complete 保持 false → shouldContinue: false → 交给 LOOP_END 判断
```

**二层参数设计的优势**：

| 方面 | 三层（result + data + variables） | 二层（data + variables） |
|------|------------------------------------|--------------------------|
| LLM 认知 | 需区分"结果文本" vs "结构化数据" vs "变量" | 清晰二分：产物(data) vs 状态(variables) |
| 幻觉风险 | 文本与结构化数据描述同一内容时易矛盾 | data 仅做追加，variables 仅做设值，无重叠 |
| `result` 冗余 | LLM 生成的文本可能与 conversation 已有重复 | 去掉后 conversation 天然承接 |
| `data` 语义 | 值类型混合 | 单值→append, 数组→batch append |
| `variables` 语义 | 不明确 | 纯设值语义 |
```

---

## 5. 与现有架构的集成

### 5.1 新增模块位置

```
packages/sdk-kit/src/starter/
├── types.ts                          # StarterMetadata, WorkflowBundle, WorkflowStarter
├── starter-registry.ts               # StarterRegistry
├── bundle-installer.ts               # WorkflowBundle → 各 Registry
├── base-starter.ts                   # BaseStarter 抽象类
├── config-validator.ts               # 配置校验
│
└── starters/
    └── code-review-agent.starter.ts  # 代码审查 + 修改 Starter

packages/sdk/resources/predefined/tools/builtin/attempt-completion/
├── schema.ts        # ToolParameterSchema（JSON Schema）
├── handler.ts       # 处理器（factory pattern）
├── description.ts   # ToolDescriptionData（LLM 可见的描述）
└── index.ts         # 桶导出

packages/sdk/resources/predefined/tools/builtin/
├── registry.ts      # [修改] 注册 attempt_completion
└── index.ts         # [修改] 导出 attempt-completion

packages/sdk/agent/execution/coordinators/
└── agent-iteration-coordinator.ts    # [修改] 检测 attempt_completion → shouldContinue=false

packages/sdk/workflow/execution/handlers/node-handlers/
└── agent-loop-handler.ts             # [修改] syncCompletionData (从 result.completionData 同步)

packages/sdk/shared/utils/event/builders/
├── attempt-completion-events.ts       # [新增] buildAttemptCompletionEvent builder
└── index.ts                           # [修改] 导出新 builder

packages/types/src/events/
├── base.ts                            # [修改] EventType 增加 ATTEMPT_COMPLETION
├── attempt-completion-events.ts       # [新增] AttemptCompletionEvent 接口
└── index.ts                           # [修改] 导出新类型，加入 Event union

packages/types/src/agent-execution/
└── types.ts                           # [修改] AgentLoopResult 增加 completionData

packages/types/src/trigger/
└── trigger-schema.ts                  # [修改] eventTypeSchema 增加 ATTEMPT_COMPLETION

packages/types/src/tool/
└── execution.ts     # [修改] ToolOutput 增加 metadata?: Record<string, unknown>
```

### 5.2 对现有架构的影响

| 方面 | 影响 | 说明 |
|------|------|------|
| 现有 Registry | 无侵入 | Starter 复用已有的所有 Registry |
| Workflow 执行引擎 | 无侵入 | 展开后的 WorkflowTemplate 与手动构造完全一致 |
| Agent Loop 内部 | 新增工具 | `attempt_completion` 是新增的内置工具，不影响现有工具 |
| 变量系统 | 增强 | attempt_completion 通过 VariableManager 的标准 API 操作变量 |
| 向后兼容 | 完全 | 所有现有 Agent Loop 定义无需修改 |
| `config-processor` | 可能新增类型 | 可选择性地支持从 TOML 文件加载 Starter |

### 5.3 实施路径

```
Phase 1: 增强 attempt_completion
  ├─ ToolOutput 增加 metadata 字段 (types)
  ├─ 创建 attempt-completion 工具 (schema + handler + description)
  ├─ 注册到 builtin tool registry
  ├─ AgentIterationCoordinator: 检测 attempt_completion → shouldContinue=false + emit ATTEMPT_COMPLETION 事件 + 返回 completionData
  ├─ AgentExecutionCoordinator: 传递 completionData 到 AgentLoopResult
  ├─ AgentLoopHandler: syncCompletionData (从 result.completionData 同步，移除 conversation 扫描)
  ├─ 定义 ATTEMPT_COMPLETION 事件类型 + builder
  ├─ 注册到 EventType union + Event union + trigger-schema
  └─ ToolExecutionCoordinator.executeToolCalls() 返回 ToolExecutionResult[]

Phase 2: Starter 接口定义
  ├─ packages/sdk-kit/src/starter/types.ts
  ├─ packages/sdk-kit/src/starter/base-starter.ts
  └─ packages/sdk-kit/src/starter/starter-registry.ts

Phase 3: Bundle 安装器
  └─ packages/sdk-kit/src/starter/bundle-installer.ts

Phase 4: CodeReviewAgentStarter 实现
  └─ packages/sdk-kit/src/starters/code-review-agent.starter.ts

Phase 5: 集成 + 测试
  ├─ agent-loop-handler 变量同步测试
  ├─ starter activate/deactivate 测试
  └─ 端到端工作流测试
```

---

## 6. 关键设计决策

| 决策点 | 选择 | 理由 |
|--------|------|------|
| **attempt_completion 增强方式** | 扩展参数而非新工具 | 保持语义一致；LLM 已理解 "completion = 结束" 的语义 |
| **变量操作时机** | 工具处理器内执行，AgentLoopHandler 同步（事件驱动） | 双层模型确保隔离性；ATTEMPT_COMPLETION 事件可供 TriggerCoordinator 消费 |
| **data 追加语义** | append 而非 replace | 符合 "累积审查记录" 的业务语义 |
| **Starter 实现位置** | sdk-kit | 已有 Builder 模式可复用；sdk 核心层保持纯净 |
| **端条件表达** | LOOP_END.breakCondition 标准 Condition | 复用现有条件引擎，无需新增 DSL |
| **消息上下文聚合** | 通过 `default` 单一 Context 累积 | 简化数据流；LLM Node 和 Agent Loop 通过同一上下文读写 |
| **配置输入格式** | 代码（类型安全）+ 未来 TOML 支持 | 先代码后 TOML，渐进式 |
