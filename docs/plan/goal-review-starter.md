# Goal Review Starter — Phase 4 设计方案

## 1. 命名与定位

原 "Code Review Agent" Starter 更名为 **Goal Review Starter**，定位为通用的**目标驱动审查循环**模式，匹配 Codex Goal 机制：

- **Goal（目标）**：顶层需求 `rootRequirement` 作为独立变量持久存储，不淹没在对话上下文中
- **执行者（Executor）**：围绕目标迭代工作，可执行文件读写、命令运行等操作
- **审查者（Reviewer）**：独立评审，输出结构化 judge 记录 + status 信号
- **规划者（Planner）**：轻量 LLM，基于历史审查缺陷生成本轮子任务

### 与原始 Code Review Agent 的区别

| 方面 | 原始设计 | 改进后 |
|------|---------|--------|
| 命名 | code-review-agent | goal-review-agent（通用目标驱动） |
| rootRequirement | 仅存在于上下文消息头部 | 独立 `variables.rootRequirement`，每轮强制注入 |
| status | 仅 `complete: boolean` | 枚举 `status: planning / executing / reviewing / completed / stuck` |
| judges | 无生命周期 | `{ iteration, file, score, comment, resolved }`，Planner 优先处理未解决项 |
| Reviewer 工具集 | 与 Executor 共用 | 限制为只读（readFile, glob, grep） |
| 停滞检测 | 无 | 连续重复缺陷 → 自动标记 `stuck` |
| Planner 规划依据 | 仅上下文 | 强制读取 `rootRequirement` + 未解决 `judges` |
| Agent 定义 | 内联构建（private builder） | 独立 AgentTemplate 预注册，通过 agentLoopId 引用 |
| 上下文保护 | 无 | 超长时自动摘要压缩 |
| Executor 内层保护 | 仅有 maxIterations | 增加 maxToolCalls 单轮工具调用上限 |

## 2. 工作流架构

```
变量：
  rootRequirement: string    —— 原始目标，不起变量名冲突
  status: string             —— "planning"|"executing"|"reviewing"|"completed"|"stuck"
  complete: boolean          —— 快捷布尔，兼容 LOOP_END.breakCondition
  judges: array              —— [{ iteration, file, score, comment, resolved }]
  iterationCount: number

上下文：
  "default" —— 累积整个工作流的对话历史

流程：
START (注入 planner 系统提示 + 初始用户请求至 "default" 上下文)
  │
  ▼
LOOP_START (maxIterations, 将 variables 映射进循环作用域)
  │
  ▼
LLM Node (任务规划者)
  │  读取 "default" 上下文 + 注入 rootRequirement / judges 未解决项
  │  输出：本轮子任务描述
  │
  ▼
Executor AGENT_LOOP (引用预注册的 executor-agent 模板)
  │  全工具集（readFile, writeFile, editFile, bash, glob, grep 等）
  │  通过 messageInputs 读取 "default" 上下文
  │  通过 messageOutputs 将对话同步回 "default"
  │  内部 maxToolCalls 限制，防止无限工具轮
  │  内部 attempt_completion 宣告完成
  │
  ▼
Reviewer AGENT_LOOP (引用预注册的 reviewer-agent 模板)
  │  只读工具集（readFile, glob, grep）
  │  读取本轮修改后上下文，注入历史 judges
  │  调用 attempt_completion：
  │    data: { judges: { ... } }
  │    variables: { complete, status }
  │  将评审摘要追加到 "default" 上下文
  │
  ▼
LOOP_END
  ├── status === "completed" 或 status === "stuck" → 跳至 END
  └── 否则 → 跳回 task_planner，进入下一轮
```

### 停滞检测（prompt 层面）

停滞判定不依赖引擎层面的算法（无法可靠计算语义相似度、score 波动等），而是通过 **Reviewer 系统提示词** 引导模型自行识别：

> 在 Reviewer 的 system prompt 中说明：如果本轮评审结果与上一轮高度重复（相同文件、相似问题、评分无实质性变化），请将 `status` 设为 `"stuck"`。

Reviewer 模型根据自身对上下文的理解判断"是否陷入无进展循环"，由 LLM 自行输出 `status: "stuck"`。LOOP_END 节点通过 `breakCondition` 表达式 `status === "completed" || status === "stuck"` 统一处理退出。

## 3. Agent 模板独立定义（核心改进）

### 3.1 设计思路

Executor 和 Reviewer 不再由 `GoalReviewStarter.assemble()` 内部生成，而是**预定义为独立的 `AgentLoopDefinition` 常量**，存放在 `packages/sdk-kit/src/resources/predefined/agent-templates/`。

Starter 在 `assemble()` 时 **import 模板常量**，将其字段作为默认值，与用户配置的覆盖项合并后填入 `inlineConfig`。Workflow 节点的 `agentLoopId` 字段记录模板 ID 作为引用标记。

**优势**：
- 模板定义与 Starter 实现解耦，每个模板是一个独立的 `.ts` 文件
- 同一模板可被多个 Starter 引用（import）
- 工具集、系统提示词等固化在模板中，Starter 代码只需关注覆盖逻辑

### 3.2 预定义模板（`resources/predefined/`）

```typescript
// packages/sdk-kit/src/resources/predefined/agent-templates/goal-review-executor.ts
export const executorTemplate: AgentLoopDefinition = {
  id: "@standard/goal-review-executor",
  name: "Goal Review Executor",
  profileId: "gpt-4o",
  systemPrompt: `You are an executor working toward a goal. ...`,
  maxIterations: 30,
  availableTools: {
    tools: ["readFile", "writeFile", "editFile", "glob", "grep", "bash", "attempt_completion"],
  },
};
```

```typescript
// packages/sdk-kit/src/resources/predefined/agent-templates/goal-review-reviewer.ts
export const reviewerTemplate: AgentLoopDefinition = {
  id: "@standard/goal-review-reviewer",
  name: "Goal Review Reviewer",
  profileId: "o3-mini",
  systemPrompt: `You are a strict code reviewer. ...`,
  maxIterations: 10,
  availableTools: {
    tools: ["readFile", "glob", "grep", "attempt_completion"],
  },
};
```

### 3.3 Starter 组装时的合并逻辑

```typescript
// 在 GoalReviewStarter.buildWorkflow() 中:
private buildExecutorInlineConfig(config: GoalReviewConfig) {
  return {
    profileId: config.executorProfileId ?? executorTemplate.profileId,
    systemPrompt: config.executorSystemPrompt ?? executorTemplate.systemPrompt,
    maxIterations: config.executorMaxIterations ?? executorTemplate.maxIterations,
    availableTools: {
      tools: config.executorTools ?? executorTemplate.availableTools?.tools ?? [],
    },
  };
}
```

Starter 不再产生 `WorkflowBundle.agentLoops`。节点通过 `agentLoopId` 声明模板身份，`inlineConfig` 承载模板 + 覆盖的完整配置：

```typescript
{
  id: "executor_agent",
  type: "AGENT_LOOP",
  config: {
    agentLoopId: "@standard/goal-review-executor",  // 引用标记
    inlineConfig: {
      ...executorInline,                             // 合并后的字段
      messageInputs: [...],
      messageOutputs: [...],
    },
  },
}
```

### 3.4 用户配置覆盖

用户的 TOML 配置文件中指定需要覆盖的字段：

```toml
# 用户 config.toml — 仅覆盖需要修改的字段
executorProfileId = "gpt-4o-turbo"     # 覆盖模板默认的 profileId
reviewerMaxIterations = 15             # 覆盖模板默认的 maxIterations
```

未覆盖的字段自动使用模板默认值。模板本身作为 SDK 代码发布，用户无需关注模板内部细节。

## 4. 配置接口

```toml
starterId = "@standard/goal-review-agent"

# === 顶层目标 ===
rootRequirement = "请审查 src/processor.ts 并修复性能问题"

# === 循环控制 ===
maxIterations = 10
contextCompression = { enabled = true, maxTokens = 32000, strategy = "summary" }

# === LLM 模型配置 ===
plannerProfileId = "gpt-4o-mini"           # 轻量模型，仅做任务分发

# === Agent 模板引用（覆盖特定字段） ===
# Executor/Reviewer 从模板注册表中按 ID 加载，此处只做选择性覆写
[agentOverrides.executor]
profileId = "gpt-4o"
maxIterations = 50

[agentOverrides.reviewer]
profileId = "o3-mini"
maxIterations = 15

# === 系统提示词（仅在想要覆盖模板默认提示词时使用） ===
plannerSystemPrompt = """
You are a task planner for a goal-driven review loop. Given the root goal,
the conversation history, and unresolved review defects, determine the
single most impactful next task for the executor.
"""
```

## 5. 文件结构

```
packages/sdk-kit/src/
├── index.ts                              # 桶导出
├── starter/
│   ├── types.ts                          # 基础类型
│   ├── base-starter.ts                   # 抽象基类
│   ├── starter-registry.ts               # Starter 注册中心
│   └── starters/
│       ├── index.ts
│       └── goal-review-starter.ts        # GoalReviewStarter（import 模板）
└── resources/
    └── predefined/
        ├── index.ts                      # 桶导出
        └── agent-templates/
            ├── index.ts                  # 桶导出
            ├── goal-review-executor.ts   # Executor 模板常量
            └── goal-review-reviewer.ts   # Reviewer 模板常量
```

预定义模板是纯 TypeScript 常量，随 SDK-kit 包发布。用户无需额外配置即可使用；需要自定义时通过 Starter 配置参数覆盖。

## 6. 实现要点

### 6.1 Starter 实现（简化后）

```typescript
import { executorTemplate, reviewerTemplate } from "../../resources/predefined/agent-templates/index.js";

class GoalReviewStarter extends BaseStarter<GoalReviewConfig> {
  assemble(config: GoalReviewConfig): WorkflowBundle {
    return {
      workflow: this.buildWorkflow(config),
      promptTemplates: [this.buildPlannerPrompt(config)],
      // agentLoops 已移除 — 预定义模板通过 inlineConfig 引用
    };
  }

  private buildExecutorInlineConfig(config: GoalReviewConfig) {
    return {
      profileId: config.executorProfileId ?? executorTemplate.profileId,
      systemPrompt: config.executorSystemPrompt ?? executorTemplate.systemPrompt,
      maxIterations: config.executorMaxIterations ?? executorTemplate.maxIterations,
      availableTools: {
        tools: config.executorTools ?? executorTemplate.availableTools?.tools ?? [],
      },
    };
  }

  private buildWorkflow(config): WorkflowTemplate {
    const executorInline = this.buildExecutorInlineConfig(config);
    // ...
    {
      id: "executor_agent",
      type: "AGENT_LOOP",
      config: {
        agentLoopId: executorTemplate.id,  // 引用标记
        inlineConfig: {
          ...executorInline,               // 模板 + 覆盖的合并结果
          messageInputs: [{ externalName: "default", internalName: "system-context" }],
          messageOutputs: [{ internalName: "system-context", externalName: "default" }],
        },
      },
    }
  }
}
```

### 6.2 WorkflowTemplate 变量

| 变量名 | 类型 | 初始值 | 说明 |
|--------|------|--------|------|
| `rootRequirement` | string | config | 只读，每轮 planner/reviewer 强制注入 |
| `status` | string | `"planning"` | 枚举，控制循环流向 |
| `complete` | boolean | `false` | 短路布尔，兼容 LOOP_END |
| `judges` | array | `[]` | `[{ iteration, file, score, comment, resolved }]` |
| `iterationCount` | number | `0` | 迭代计数器 |

### 6.3 节点拓扑

```
start → loop_start → task_planner → executor_agent → reviewer_agent → loop_end
loop_end → end                           (DEFAULT, status === "completed"|"stuck")
loop_end → task_planner                  (CONDITIONAL, nextIteration === true)
```

共 7 个节点：START, LOOP_START, LLM(planner), AGENT_LOOP(executor), AGENT_LOOP(reviewer), LOOP_END, END

### 6.4 maxToolCalls 内层保护（规划中）

`maxToolCalls` 字段预留在模板定义中作为元数据，实际强制执行需要 AgentLoopCoordinator 层支持（Phase 4+）。当前通过 `maxIterations` 控制外层循环次数。

### 6.5 上下文压缩策略（规划中）

当 "default" 上下文累计消息长度超过 `contextCompression.maxTokens` 时：

```
1. 保留最近一轮 executor + reviewer 的完整消息（最新交互）
2. 对更早轮次的对话做摘要：
   - 提取每轮关键变更摘要（哪些文件改了、改了啥）
   - 提取 judge 记录（已持久化在变量中，无需保留原文）
   - 丢弃中间工具调用日志
3. 将压缩后的摘要作为单条 system 消息插入上下文头部
```

配置：

```toml
contextCompression = { enabled = true, maxTokens = 32000, strategy = "summary" }
```

### 6.6 同步机制

```
Executor/Reviewer 内部:
  attempt_completion({
    data: { judges: { iteration, file, score, comment, resolved } },
    variables: { complete: true/false, status: "completed"|"reviewing"|... }
  })
    → ToolOutput.metadata 携带 data + variables
    → AgentIterationCoordinator 提取，emit ATTEMPT_COMPLETION 事件
    → AgentLoopResult.completionData 向上传播
    → AgentLoopHandler.syncCompletionData() 同步到 Workflow VariableManager
```

**新增**：`resolved` 字段默认 `false`。Reviewer 每次重新审查时，若某文件的 score >= 8 且无新增缺陷，可标记 `resolved: true`。

Planner 读取 judges 时自动过滤：`judges.filter(j => !j.resolved)`。

## 7. 向后兼容

| 方面 | 影响 |
|------|------|
| 现有 Registry | 无影响，模板不引入新的 Registry |
| 现有 Workflow | `inlineConfig` 新增 `systemPrompt` 字段，向后兼容 |
| Agent Loop Handler | `resolveAgentRuntimeConfig` 读取 `inlineConfig.systemPrompt`（之前硬编码为空字符串） |
| `@standard/code-review-agent` | 更名为 `@standard/goal-review-agent`，旧的 id 保留为 alias（可选） |
| 自定义 Starter | 无影响，模板导入对使用者透明 |
