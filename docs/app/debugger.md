# debugger（wf-debugger / wf-debug）规范与功能说明

> 本文说明 `crates/app/debugger/` 的定位、架构、数据格式、CLI 用法与扩展规范。设计背景见 `docs/plan/debugger/`。

## 一、定位与依赖边界

`wf-debugger` 是 app 层的**离线执行调试器**：库包名 `wf-debugger`，CLI 二进制名 `wf-debug`，工作区目录 `crates/app/debugger/`。

核心特征：

- **离线、只读**：输入是已录制/手写的 mock trace JSON，不构造 LLM、沙箱、存储等运行时基础设施，不真正执行工作流或 agent 循环；
- **复用引擎纯函数**：条件求值复用 `wf_core::condition::ConditionEvaluator`，路由/hook/trigger 解释复用其原生语义，不重写触发逻辑；
- **轻量 DAG**：仅依赖 foundation 层（`wf-types`、`wf-common`、`wf-core`）加 `serde/serde_json/clap/anyhow/similar`，不依赖 engine 层；无任何生产 crate 反向依赖 `wf-debugger`；
- 与 `tui-debug`（渲染层录制）无关，不冲突。

## 二、目录结构

```
crates/app/debugger/src/
├── main.rs          # 入口：解析 CLI → run → 非 0 退出码时 std::process::exit
├── lib.rs           # pub mod 声明 + re-export（Trace / replay_trace / run_check 等）
├── input/           # 输入装载与快照导入
│   ├── loader.rs    #   load_trace / parse_trace（强制校验 schema）/ load_variables / load_decision / load_trigger_templates
│   └── snapshot.rs  #   import_snapshot：引擎快照 dump（steps 或旧键 node_records）→ 规范化 trace
├── model/           # 数据模型与纯计算
│   ├── trace.rs     #   TRACE_SCHEMA_V1、Trace、TraceKind
│   ├── step.rs      #   StepRecord（单步核心字段 + 各维度可选视图 + 子执行层级 + children）
│   ├── views_*.rs   #   tool/llm/approval/visibility、budget/compression、loop/merge/route、interruption/checkpoint/interaction/hook/trigger 视图
│   ├── assertion.rs #   Assertion 枚举（16 种）与结果结构
│   ├── traverse.rs  #   walk(trace)：深度优先遍历，产出 StepVisit { path, depth, step }
│   ├── observe.rs   #   变量/消息 diff（基于 similar）
│   ├── report.rs    #   Finding / SectionReport / UnifiedReport / unify()
│   └── util.rs      #   payload 截断（4000 字符，UTF-8 边界安全）
├── engine/          # 回放、断言、格式化、编排
│   ├── replay.rs    #   replay_trace：逐步 diff + ReplaySummary 聚合
│   ├── assert.rs    #   run_assertions：16 种断言求值
│   ├── format.rs    #   format_text（ANSI 彩色）/ format_json
│   ├── timeline.rs  #   build_timeline：扁平事件时间线
│   ├── branches.rs  #   evaluate_decision：路由分支覆盖与死分支
│   └── runner.rs    #   run_check / render_*：命令编排与退出码计算
├── dimensions/      # 九个维度分析器（analyze(&Trace) -> SectionReport）
│   ├── loops.rs / merges.rs / interruptions.rs / checkpoints.rs
│   ├── interactions.rs / subexec.rs / cost.rs / compression.rs
│   └── spec.rs      #   领域专项：@standard/spec-workflow 流水线阶段校验
├── probes/          # 交互式解释器（复用引擎纯函数）
│   ├── hook.rs      #   hook 点逐条解释（enabled/condition/veto）
│   ├── trigger.rs   #   trigger 模板 dry-run（候选/落选原因/permits）
│   ├── policy.rs    #   内置 agent 模板工具契约快照（POLICY_SNAPSHOT_VERSION）
│   └── agent.rs     #   用 policy 快照核对 agent trace 的工具调用合规
└── cli/             # args.rs（clap 定义）+ commands.rs（子命令分发）
```

模块依赖方向：`input → model`，`engine/dimensions/probes → model`，`cli → 全部`；均为单向，无循环。

## 三、Trace 数据格式（wf-debug-trace/v1）

输入为 JSON，顶层 schema 常量 `TRACE_SCHEMA_V1 = "wf-debug-trace/v1"`，装载时强制校验，不匹配即报错（不做静默迁移）。

`Trace`（model/trace.rs）核心字段：

| 字段 | 含义 |
|------|------|
| `kind` | `workflow` / `agent` |
| `graph_ref` / `agent_template` | 工作流引用 / agent 模板（空则跳过内置策略分析） |
| `initial_variables` | 初始变量，作为条件求值上下文 |
| `steps` | 步骤数组（可嵌套 `children` 表达子执行） |
| `assertions` | 断言列表 |
| `trigger_templates` / `budget` | 触发器模板定义 / 预算上限（token/cost/warn_at/context_limit） |

`StepRecord`（model/step.rs）核心字段：`index`、`node_id`、`input`/`result`、`success`/`error`/`error_kind`/`retryable`/`recovery_hint`、`branch_id`/`route_target`、`variables_before/after` 与 `messages_before/after`（供 diff）、以及**可选维度视图**：`tool_calls`、`llm_calls`、`approval`、`visibility`、`loop_round`、`merge`、`interruption`、`checkpoint`、`interaction`、`compressions`、`hooks_fired`、`triggers_seen`；子执行字段 `exec_id`/`parent_exec_id`/`depth` 与递归 `children`。

设计规范：**新增调试维度只加视图字段，不改 `StepRecord` 核心结构**；弱类型自由字符串字段应尽量避免。

引擎真实运行的快照可通过 `wf-debug import --snapshot <file>` 转为规范 trace（接受 `steps` 或旧键 `node_records`，容忍缺省字段，导入后重新过 schema 校验）。

## 四、CLI 子命令

所有子命令支持 `--json` 输出机读格式（serde_json pretty）；文本输出支持 `--no-color`。

| 子命令 | 参数 | 功能 | 退出码 |
|--------|------|------|--------|
| `replay` | `--trace` | 只读回放：逐步输出 input/result、变量/消息 diff、时长、错误，末尾聚合 summary | 恒 0 |
| `check` | `--trace [--agent]` | 综合检查：回放 + 九维度分析 + 断言 + agent 策略核对，统一报告 | 失败/违例为 1 |
| `branches` | `--decision [--variables]` | 路由决策点分支求值（复用 ConditionEvaluator）、死分支与覆盖率 | 恒 0 |
| `hooks` | `--trace` | hook 点解释：逐条 enabled/condition 过滤，汇总 veto 与 gate_blocked | 恒 0 |
| `triggers` | `--trace [--event-type --event-name]` | 模板 dry-run：候选、逐条落选原因、dry-run permits；或统计已见事件 | 恒 0 |
| `assert` | `--trace` | 仅执行 16 种断言 | 失败数 > 0 为 1 |
| `timeline` | `--trace` | 扁平事件时间线（node→loop→hook→trigger→llm→compression→interruption→checkpoint→interaction→merge） | 恒 0 |
| `agents` | `--trace [--agent]` | 内置模板策略核对：unexpected_success / unexpected_denial_text / discoverable 越权 / approval 缺失 | 有违规为 1 |
| `compression` | `--trace` | 压缩五阶段生命周期审计（见下） | errors > 0 为 1 |
| `import` | `--snapshot [--out]` | 引擎快照 → 规范 trace（`--out` 写文件否则 stdout） | 恒 0 |
| `spec` | `--trace` | spec-workflow 流水线阶段顺序与 gate 审批校验 | errors > 0 为 1 |

退出码约定：**信息展示类恒 0；校验类以失败/违规数决定 0 或 1**；加载/IO 错误经 anyhow 返回非 0。该约定使 `check` / `assert` / `agents` / `compression` / `spec` 可直接用作 CI 门禁。

## 五、维度分析器

每个维度暴露统一接口 `analyze(&Trace) -> SectionReport`，内部经 `walk` 遍历步骤并检查对应的可选视图字段，产出 `counts`（计数）与 `findings`（分级发现：Error/Warning/Info；仅 Error 使检查失败）。

| 维度 | 观察对象 | 主要规则 |
|------|----------|----------|
| loops | 循环轮次（轮次、失败、恢复、上限） | fail_fast 失败轮 Error；吸收型策略 Warning；达 max_iterations Warning |
| merges | JOIN 汇合（分支结果、失败吸收） | 失败分支 Error；推导 outcome 与声明不符 Warning |
| interruptions | 中断 pause/stop/cancel/timeout | 未恢复中断 Error（含耗时） |
| checkpoints | 检查点标记（timing、来源 Policy/Hook） | 缺 checkpoint_id Warning |
| interactions | 交互问答生命周期 | 残留 pending Error；超时/丢弃 Warning |
| subexec | 子执行层级（depth、parent、等待） | 声明 depth 与实际不符 Error；缺 parent_exec_id Warning |
| cost | LLM 用量与预算 | estimated 调用不计入预算口径；超限 Error、达 warn_at Warning |
| compression | 压缩五阶段（requested→routed→completed/failed/discarded） | 失败、降级、仍超预算、孤儿请求、handoff 竞态、冗余 run_id、连续 no_taker ≥3 次 Error |
| spec | spec-workflow 阶段序列与 gate 审批 | 阶段逆序 Error；gate 无已完成 interaction Warning、pending Error；非 spec 工作流返回空 section |

## 六、断言

`Assertion` 枚举共 16 种（model/assertion.rs）：StepSuccess、StepResult、Variable、Route、HookOutcome、TriggerMatched、ToolNeverCalled、ToolNeverSucceeded、ToolDeniedWith、LoopRounds、MergeOutcome、InterruptionCount、CheckpointPresent、InteractionSettled、TokenBudget、CostBudget。

失败时必须给出 **expected / actual / message** 三元组——「出错给实际值」是断言层的硬契约（实际值取节点的真实 `result`/`error`、变量实际值、hook 实际 veto reason 等）。

## 七、agent 策略核对机制

`probes/policy.rs` 保存内置 agent 模板（main/explorer/worker）的工具契约**静态快照**（available / discoverable / require_approval 列表），带 `POLICY_SNAPSHOT_VERSION`；引擎模板变更时快照会显式漂移，便于发现失同步。`probes/agent.rs` 遍历 trace 核对：可用集之外的工具成功、拒绝文案不含策略门控短语、discoverable 工具越权直调、require_approval 工具成功但缺匹配 approval 记录；预期内的策略拒绝只计数不报违规。未知模板仅报 `known_template=false`，`--agent` 参数可覆盖 trace 声明的模板。

## 八、扩展规范

新增调试维度时：

1. 在 `model/` 对应 `views_*.rs` 增加视图结构（或在 `step.rs` 上加可选字段）；
2. 在 `dimensions/` 新建 `<name>.rs`，实现 `analyze(&Trace) -> SectionReport`，只读遍历，附带 `#[cfg(test)]` 单测；
3. 接入 `dimensions.rs` 的 `analyze_all`，自动进入 `check` 的统一报告；
4. 如需独立 CLI，在 `cli/args.rs` 加子命令、`cli/commands.rs` 加分发，遵守退出码约定；
5. 新增断言：扩展 `model/assertion.rs` 的枚举并在 `engine/assert.rs` 的 `evaluate_one()` 增加分支，失败必须返回 expected/actual。

代码约定：遵循 AGENTS.md——禁止 `mod.rs`（一律 `pub mod` 扁平文件）；代码、注释、错误信息全英文；禁止在注释中引用文档结构标识符；禁用 `unwrap`（测试中可用 `expect`）。

## 九、构建与验证

```shell
cargo build -p wf-debugger        # 构建 lib + wf-debug 二进制
cargo test -p wf-debugger          # 各模块内置单测
cargo tree -p wf-debugger          # 校验 DAG 仅向下（foundation + 第三方）
```

示例：`wf-debug check --trace examples/xxx.json`（失败时文本/JSON 均给出期望 vs 实际）。
