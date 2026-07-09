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
Executor AGENT_LOOP
  │  全工具集（readFile, writeFile, editFile, bash, glob, grep 等）
  │  通过 messageInputs 读取 "default" 上下文
  │  通过 messageOutputs 将对话同步回 "default"
  │  内部 attempt_completion 宣告完成
  │
  ▼
Reviewer AGENT_LOOP
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

### 停滞检测逻辑（外层 REVIEW-LOOP 内）

```
每轮 review 完成后:
  比较本轮 judges 与上轮 judges:
    若连续 2 轮新增缺陷高度重复（score 相近、comment 语义相似度 > 阈值）
    → 设置 status = "stuck"，终止循环
```

## 3. 配置接口

```toml
starterId = "@standard/goal-review-agent"

# === 顶层目标 ===
rootRequirement = "请审查 src/processor.ts 并修复性能问题"

# === 循环控制 ===
maxIterations = 10

# === LLM 模型配置 ===
plannerProfileId = "gpt-4o-mini"           # 轻量模型，仅做任务分发
executorProfileId = "gpt-4o"                # 执行模型
reviewerProfileId = "o3-mini"               # 审查模型（建议更强模型）

# === 系统提示词 ===
plannerSystemPrompt = """
You are a task planner for a goal-driven review loop. Given the root goal,
the conversation history, and unresolved review defects, determine the
single most impactful next task for the executor.
"""

executorSystemPrompt = """
You are an executor working toward a goal. Make code changes, run tests,
and call attempt_completion when you believe the task is done.
"""

reviewerSystemPrompt = """
You are a strict reviewer. Review all changes against the root goal.
For each file, assign a score (1-10) and actionable feedback.
Call attempt_completion with status completed only if ALL criteria are met.
"""

# === 初始消息（注入 "default" 上下文的起始内容） ===
[[initialPrompts]]
role = "system"
content = "You are a goal-driven review assistant..."

[[initialPrompts]]
role = "user"
content = "Root goal: {{rootRequirement}}\nTarget path: {{targetPath}}"
```

## 4. 文件结构

```
packages/sdk-kit/src/starters/
├── index.ts                          # 桶导出
└── goal-review-starter.ts            # GoalReviewStarter 实现
```

## 5. 实现要点

### 5.1 Starter 实现

```typescript
// packages/sdk-kit/src/starters/goal-review-starter.ts

class GoalReviewStarter extends BaseStarter<GoalReviewConfig> {
  // 固定的元数据
  metadata: StarterMetadata = {
    id: "@standard/goal-review-agent",
    name: "Goal Review Agent",
    version: "1.0.0",
    description: "Goal-driven review loop with planner, executor, and reviewer",
    configurable: { ... },
  };

  assemble(config): WorkflowBundle {
    return {
      workflow: this.buildWorkflow(config),         // 完整 WorkflowTemplate
      agentLoops: [
        this.buildExecutorAgent(config),             // Executor AgentLoopDefinition
        this.buildReviewerAgent(config),             // Reviewer AgentLoopDefinition
      ],
      promptTemplates: [ this.buildPlannerPrompt(config) ],
    };
  }
}
```

### 5.2 WorkflowTemplate 结构

**variables**:
- `rootRequirement: string` (readonly) — 顶层目标，不参与消息上下文
- `status: string` (初始值 `"planning"`)
- `complete: boolean` (初始值 `false`)
- `judges: array` (初始值 `[]`)
- `iterationCount: number` (初始值 `0`)

**nodes**（共 7 个节点）:

| ID | Type | 职责 |
|----|------|------|
| `start` | START | 接收外部输入，注入初始消息到 "default" 上下文 |
| `loop_start` | LOOP_START | 初始化 review-loop，映射变量 |
| `task_planner` | LLM | 读取上下文，输出子任务 |
| `executor_agent` | AGENT_LOOP | 引用 executor 配置，全工具集执行 |
| `reviewer_agent` | AGENT_LOOP | 引用 reviewer 配置，只读审查 |
| `loop_end` | LOOP_END | 检查 status 决定是否跳出 |
| `end` | END | 输出最终评审结果 |

**edges**:
```
__start__ → start → loop_start → task_planner → executor_agent → reviewer_agent → loop_end
loop_end → end                           (DEFAULT)
loop_end → task_planner                  (CONDITIONAL, nextIteration === true)
```

### 5.3 AgentLoopDefinition

**Executor**:
- 全工具集（readFile, writeFile, editFile, bash, glob, grep, attempt_completion）
- `maxIterations: 30`
- 通过 `messageInputs` 读取 "default" 上下文
- 通过 `messageOutputs` 写回 "default" 上下文

**Reviewer**:
- 只读工具集（readFile, glob, grep, attempt_completion）
- `maxIterations: 10`
- 通过 `messageInputs` 读取 "default" 上下文
- 通过 `dataInputs` 接收 `judges` 数组
- 通过 `messageOutputs` 写回 "default" 上下文

### 5.4 同步机制

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

## 6. 向后兼容

| 方面 | 影响 |
|------|------|
| 现有 Registry | 无侵入，复用已有 6 个 Registry |
| 现有 Workflow | 无影响，GoalReviewStarter 生成标准的 WorkflowTemplate |
| Agent Loop 内部 | 依赖已存在的 `attempt_completion`（Phase 1） |
| `@standard/code-review-agent` | 更名为 `@standard/goal-review-agent`，旧的 id 保留为 alias（可选） |
