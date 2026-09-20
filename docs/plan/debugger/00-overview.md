# 调试器（debugger）设计方案 · 总览

> 适用范围：本目录下的文档描述在 `wf-agent` 中引入「工作流 / agent 调试功能」的目标、可观测点分析与模块设计。
> 文档遵循 AGENTS.md 约定：中文撰写、自然语言为主、避免完整代码段；所有事实声明均附 `file:line` 引用。

## 一、背景与动机

当前框架已具备两类执行引擎与一套完整的可观测数据底座：

- 工作流引擎 `wf-workflow`：基于图（`StaticNodeType` 节点 + `Edge` 边）的有向执行（`crates/foundation/wf-types/src/node/static.rs:13`、`crates/foundation/wf-types/src/workflow/edge.rs:21`）。
- Agent 引擎 `wf-agent`：自主 LLM 循环（`AgentLoopEntity` + `IterationExecutor`，`crates/engine/wf-agent/src/entity.rs:17`、`crates/engine/wf-agent/src/coordinator/iteration.rs:68`）。
- 横切机制：hook（`fire()` → `FireSummary`，`crates/engine/wf-execution-shared/src/hooks/fire.rs:85`）、trigger（`matcher→arbiter→governor→state`，`crates/engine/wf-workflow/src/trigger/`）、变量存储（`VariableStore = Arc<DashMap<String,Value>>`，`crates/engine/wf-workflow/src/variable.rs:5`）、消息上下文（`__msg_ctx__` 前缀，`crates/engine/wf-workflow/src/message_context.rs:18`）、checkpoint / 交互历史。

但项目**缺少一个针对执行逻辑本身的调试器**：

- 现有 `tui-debug` 仅录制 TUI 渲染 diff（`crates/app/tui/tui-debug/src/lib.rs`，feature-gated `diff-record`），与执行语义无关；
- 没有工具能回放一次执行、逐步展示「每步输出 / 消息 diff / 变量 diff」、对 hook/trigger 的触发决策做可解释展示、或在出错时给出实际值。

## 二、目标

引入一个**独立编译目标** `debugger`（app 层模块），核心能力：

1. 使用 **mock 交互历史**驱动，无需真实 LLM / sandbox / runtime；
2. 逐步展示**输出、消息 diff、变量 diff**，出错时给出**实际值**；
3. 对**节点 / 边路由**支持**可选的分支穷尽**探索；
4. 提供 **hook 调试**与 **trigger 调试**；
5. 同时提供**格式化输出**与**断言**能力。

## 三、定位与边界（关键决策）

| 维度 | 决策 |
|------|------|
| 层级 | app 层（`crates/app/debugger/`），独立 crate |
| 编译目标 | 独立 `[[bin]]`（如 `wf-debug`）+ `lib.rs`（可单测的核心逻辑） |
| 数据来源 | 仅 mock / 录制的交互历史，无运行时装配 |
| 依赖边界 | **不依赖** `wf-runtime` / `wf-server` / `wf-api`（它们拉入 LLM、存储、配置等重基础设施）；仅依赖 engine + foundation 层 |
| 与 `tui-debug` 关系 | 不复用、不冲突；前者是渲染层录制，本方案是执行逻辑调试 |

不依赖 `wf-runtime` 这点至关重要：debugger 走的是引擎层的**纯函数 / 轻量异步**接口（图遍历、`RouteHandler` 路由、`evaluate_expression`、`fire()`、`matcher/arbiter/governor`），可以在不构造 `LlmGateway`、`SandboxRuntime`、存储后端的前提下运行，从而保持 DAG 干净且构建轻量。

## 四、依赖与 DAG 约束

新增 crate 在 DAG 中作为 app 层叶子节点，只允许向下依赖：

- `wf-workflow`（图遍历、路由、变量、消息上下文、hook、trigger 子系统）
- `wf-agent`（实体、迭代、agent 侧 hook、`TriggeredAgentExecutionManager`）
- `wf-execution-shared`（hook `fire`/`FireSummary`、条件求值、共享类型）
- `wf-types`（`StaticNodeType`、`Edge`、`Condition`、`Hook*`、`Trigger*`、`Message`、`BaseEvent`）
- `wf-core`（`condition::ConditionEvaluator`、`EventBus`、`now`）
- `wf-common`（ID 生成、`generate_id`）

须遵守 AGENTS.md：**所有子 crate 形成严格 DAG，禁止循环依赖**。debugger 不反向被任何生产 crate 依赖，因此不会污染主构建。

## 五、整体架构（目标结构）

```
crates/app/debugger/
├── Cargo.toml            # 独立 crate；[[bin]] wf-debug；依赖仅 engine+foundation
├── src/
│   ├── lib.rs            # 声明 pub mod（扁平文件，无 mod.rs）
│   ├── model.rs          # 调试用 trace / 断言 / diff 数据结构
│   ├── replay.rs         # 回放引擎：workflow / agent 两种回放 + 分支穷尽
│   ├── observe.rs        # 变量 diff / 消息 diff 提取器（对比前后快照）
│   ├── hook_dbg.rs       # 复用 fire() 的 hook 调试
│   ├── trigger_dbg.rs    # matcher/arbiter/governor 全链路 dry-run
│   ├── format.rs         # 文本 / JSON 格式化输出
│   ├── assert.rs         # 断言引擎（期望值 vs 实际值）
│   └── cli.rs            # clap 入口：子命令 + mock 数据源选择
└── examples/             # 内置 mock 交互历史样例（供一键复现）
```

## 六、复用优先，不重复实现

debugger 不重写引擎逻辑，而是**包装已有纯函数**：

- 路由决策 → 复用 `RouteHandler` 的条件求值语义（`crates/engine/wf-workflow/src/handler/route.rs:43`）；
- 变量求值 → 复用 `evaluate_expression` / `VariableResolver`（`crates/engine/wf-workflow/src/variable.rs:631`）；
- 表达式条件 → 复用 `wf_core::condition::ConditionEvaluator`（被 `route.rs:5`、`matcher.rs:191` 使用）；
- hook → 复用 `fire()`，仅注入 mock `HookHandlerRegistry` + mock `HookContext`；
- trigger → 复用 `matcher::candidates` / `arbiter::arbitrate` / `governor::request`，用 mock `TriggerTemplateRegistry` 与 `TriggerActionRunner`（不真正执行动作）。

## 七、输入模式

debugger 支持两种 mock 数据入口（详见 `01-execution-observability.md`）：

1. **导入已录制 trace**：用户/测试提供一份描述执行的 JSON（每步输入/输出、变量快照、消息快照、hook 火点、trigger 事件）；
2. **从图定义静态综合**：仅给 graph 定义 + 初始变量，由 debugger 做静态分析（拓扑、可达性、分支穷尽），不实际运行节点。

## 八、与现有代码的位置关系

- 现有 `tui-debug`：`crates/app/tui/tui-debug/src/lib.rs`（feature-gated）——仅作渲染录制参考，不纳入本方案。
- 模块规范：AGENTS.md 要求每个 crate 的 `lib.rs` 直接声明 `pub mod`，扁平文件、禁止 `mod.rs`；本方案严格遵循。
- 工作区归属：需在根 `Cargo.toml` 的 `members` 中追加 `crates/app/debugger`（参照 `crates/app/cli/wf-headless/Cargo.toml` 的 `[[bin]]` 写法）。
