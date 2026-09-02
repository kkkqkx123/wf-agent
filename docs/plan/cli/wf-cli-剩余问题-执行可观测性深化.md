# wf-cli 剩余问题 - 执行与可观测性深化方案

> 状态：方案设计 / 待评审
> 日期：2026-09-02
> 范围：`crates/app/wf-cli` 执行生命周期与可观测性域（execution / checkpoint / state_tracker / event / audit / execution_graph）
> 关联文档：`docs/plan/cli/wf-cli-剩余问题-分阶段深化总览.md`、`docs/plan/wf-cli-api-gap-analysis.md:4.2/4.5`、`docs/api/03-workflow域.md`、`docs/api/04-infra基础设施.md`
> 源码锚点：`args.rs:542` `ExecutionSub` / `cmd/execution.rs:10` / `cmd/checkpoint.rs:1` / `cmd/event.rs:1` / `cmd/audit.rs:1` / `wf-api/src/workflow/execution_graph.rs` / `wf-api/src/infra/state_tracker.rs` / `wf-api/src/infra/events.rs`

---

## 一、现状与剩余问题

### 1.1 已完成

- `execution list/show/run/status/pause/resume/cancel/inspect/performance/bottleneck/errors/compare/progress/state` 13 子命令（`cmd/execution.rs:15-352`）已打通。
- `checkpoint create/list/show/restore` 4 子命令（`cmd/checkpoint.rs:13-57`）已打通。
- `event list/stats/timeline/follow` 4 子命令（`cmd/event.rs:14-55`）已打通，但 `follow` 为一次性 `timeline`。
- `audit summary/report/timeline/iterations/tool-calls/llm-calls/node-executions` 7 子命令（`cmd/audit.rs:13-68`）已打通。
- `workflow execution-graph show`（`cmd/workflow.rs:295`）仅 `get_execution_graph` 基础。

### 1.2 剩余问题

| 编号 | 域 | 现状 | 剩余缺口 | 对应 `wf-api` | 影响 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| B1 | execution list | 按 `agent_loop_registry::summaries` 与 `list_executions` 分支，缺统一视图与 `workflow filter + status` 组合、分页 | 未透传 `WorkflowExecutionListOptions{offset,limit,order}`；缺 `execution delete/cleanup_completed/statistics` | `workflow::execution::{delete_execution, list_executions}` + `agent_loop_registry::{statistics, cleanup_completed}` | 执行列表无法按工作流维度运维 |
| B2 | execution show | `timeline/iterations` 为可选分支，缺 `variable_history/context_evolution/execution_path` 与 `--variables` 未默认联动 | 未暴露 `agent_loop_registry::{variable_history, context_evolution, execution_path}` + `execution_state::{context_transitions, node_transitions, variable_snapshots}` | `workflow::execution_state::*` + `agent_loop_registry::*` | 调试需多次调用 |
| B3 | execution run | 仅 `execute` 阻塞，未支持 `stream/pause/resume/status` 的实时流与 `background` 真后台 | `background: true` 仍为同步 `execute`；未复用 `workflow_execution::stream` 的 `OutputSink` 流式 | `workflow::workflow_execution::{stream, status, pause, resume, cancel}` + `run.rs:402` 的流式管线 | 长执行无法前台流式 |
| B4 | checkpoint | 缺 `delete/chain/time_range/delete_by_entity` | 未暴露 `checkpoint::{delete_checkpoint, list_checkpoints_by_time_range, get_checkpoint_chain, delete_by_entity}` | `workflow::checkpoint::*` | 无法做 checkpoint GC |
| B5 | state_tracker | 仅 `get_state_at_iteration/list`，缺 `variable_history/most_changed/memory/call_stack/context_transitions` | 未暴露 `state_tracker::{get_variable_history, get_most_changed_variables, get_memory_usage, get_peak_memory_usage, get_context_transitions}` | `infra::state_tracker::*` | time-travel 调试不完整 |
| B6 | event | `follow` 非流式；`list` 未支持 `workflow_id/agent_loop_id/event_types[]` 组合过滤 | 未使用 `infra::subscription::spawn_event_subscription` 的 `Stream`；`stats` 仅全量 | `infra::events::{stats, history, timeline}` + `subscription::spawn_event_subscription` + `EventType` 枚举 | 无法实时跟随执行 |
| B7 | audit | 已全但三级回退（live→persisted→checkpoint）无显式 `source` 标记 | `audit_report` 未在输出中标注 `AuditSource` | `audit::{audit_report, AuditSource}` | 排障时不知数据来源 |
| B8 | execution_graph | 仅 `get_execution_graph`，缺 `analyze_efficiency/slow_nodes/alternative_paths/path_probability` 深层分析 | 未暴露 `execution_graph::{analyze_efficiency, get_slow_nodes, get_alternative_paths, get_path_probability_analysis, enumerate_paths}` | `workflow::execution_graph::*` | 执行图谱仅结构，无分析 |

---

## 二、修改目标

1. 执行面达到 `wf-server` 的 `GET /executions` 同等过滤能力与 `POST /executions/{id}/pause|resume|cancel` 同等生命周期。
2. 可观测性三件套（checkpoint / event / audit）达到"链式回溯 + 实时跟随 + 来源可追"。
3. 时序与状态追踪达到 time-travel 最小可用（iteration 级变量/内存/调用栈）。

---

## 三、分阶段修改方案

### 阶段 B1 - 执行列表与生命周期补齐

**改动**：

- `args.rs:544` `ExecutionSub::List` 新增 `#[arg(long)] limit/offset` 与 `#[arg(long)] order`（`asc|desc`），已存在 `workflow` 过滤与 `status` 过滤保留。
- `cmd/execution.rs:16` 合并分支：`workflow` 非空时走 `list_executions(ctx, Some(WorkflowExecutionListOptions{ workflow_id_filter: Some(wf), status_filter: status.clone(), limit, offset }))`（需核对 `WorkflowExecutionListOptions` 字段），`workflow` 为空时走 `agent_loop_registry::summaries` 的 `status` 过滤；两者均按 `order` 排序后 `take(limit)`。
- `ExecutionSub` 新增 `Delete { id, force }` 与 `Cleanup { before: Option<String> }`：
  - `Delete` → `workflow::delete_execution(ctx, id).await?` + `agent_loop_registry::delete_agent_loop(ctx, id).await?` 双删（忽略 `not_found`）
  - `Cleanup` → `agent_loop_registry::cleanup_completed(ctx, before_timestamp).await?`（`before` 解析为 `DateTime`）
- `Status` 分支增强：先 `workflow_execution::status`，失败回退 `agent_loop_registry::summary`，再失败回退 `workflow::get_execution`，三级回退均在 `Text` 模式打印 `source: live|persisted|checkpoint`。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/execution.rs`

**验收**：`wf execution list --workflow wf-1 --status completed --limit 2 -o json` 返回过滤结果；`wf execution delete <id> --force` 双删成功。

---

### 阶段 B2 - Inspect 与 Show 深度补齐

**改动**：

- `args.rs:605` `ExecutionSub::Inspect` 新增 `--variable-history --context-transitions --node-transitions --memory`。
- `cmd/execution.rs:191` `Inspect` 分支扩展：
  - `--variable-history` → `state_tracker::get_variable_history(ctx, id, name)`（需新增 `--var-name` 参数）
  - `--context-transitions` → `workflow_execution_get_context_transitions`
  - `--node-transitions` → `workflow_execution_get_node_transitions`
  - `--memory` → `state_tracker::get_memory_usage` + `get_peak_memory_usage`
- `ExecutionSub::Show` 的 `--variables` 保留，新增 `--context-evolution` 直通 `agent_loop_registry::context_evolution`。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/execution.rs`

**验收**：`wf execution inspect <id> --variables --context --call-stack --memory -o json` 返回 5 块数据；`Json` 模式下 `variables/contextEvolution/callStack/memory` 均非 `null`（当执行存在时）。

---

### 阶段 B3 - 实时流与后台执行

**改动**：

- `args.rs:569` `ExecutionSub::Run` 已有 `background`，新增 `--stream`（默认 `true` 当 `output=Text` 且非 `background`）。
- `cmd/execution.rs:98` `Run` 分支：
  - `background=false + stream=true` → 调用 `workflow_execution::stream(ctx, params).await` 得到 `ExecutionStream`，复用 `run.rs:402` 的 `SessionRenderer` 管线（`LlmDelta` 经 `DeltaBuffer` 按行 flush 到 `OutputSink`，`ToolStart/End` 走 `DiagWriter`），结束时输出 `OutputEnvelope success "execution-run-stream"` + `executionId`
  - `background=true` → `tokio::spawn(execute(...))` 后立即返回 `executionId`，`Text` 模式打印 `background execution <id> (use wf execution status <id>)`
- `Pause/Resume/Cancel` 已有，补充 `--reason` 可选透传（需 `ApiError` 携带）。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/execution.rs`、`crates/app/wf-cli/src/run.rs`（复用 `DeltaBuffer/render.rs`）

**验收**：`wf execution run --workflow wf-1 --stream -o text` 流式输出与 `wf run --workflow` 同源；`--background` 立即返回 `executionId` 且 `wf execution status <id>` 可查询。

---

### 阶段 B4 - Checkpoint 链与 GC

**改动**：

- `args.rs:786` `CheckpointSub` 新增 `Delete { id }` / `Chain { execution_id }` / `Gc { execution_id, before }`。
- `cmd/checkpoint.rs:13` 新增：
  - `Delete` → `checkpoint::delete_checkpoint(&ctx.storage, id).await?`
  - `Chain` → `checkpoint::get_checkpoint_chain(&ctx.storage, execution_id, "checkpoint").await?`
  - `Gc` → `checkpoint::delete_checkpoints_by_entity(&ctx.storage, execution_id).await?`（`before` 时按 `list_checkpoints_by_time_range` 过滤后批量删除）
- `List` 已有，补充 `--limit --offset` 分页（透传 `Pagination`）。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/checkpoint.rs`

**验收**：`wf checkpoint chain <execution-id> -o json` 返回链式；`wf checkpoint delete <id>` 后 `list` 不再包含。

---

### 阶段 B5 - State Tracker 深度

**改动**：

- `args.rs:665` `ExecutionSub::State` 已有 `at_iteration`，新增 `--variable <name> --most-changed --memory` 三互斥 flag。
- `cmd/execution.rs:334` `State` 分支扩展：
  - `--variable name` → `state_tracker::get_variable_history(ctx, id, name).await?` → `success "execution-state-variable-history"`
  - `--most-changed` → `state_tracker::get_most_changed_variables(ctx, id, limit).await?`
  - `--memory` → `state_tracker::get_memory_usage` + `get_peak_memory_usage` 聚合

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/execution.rs`

**验收**：`wf execution state <id> --variable myVar -o json` 返回历史；`--most-changed` 返回按变更次数排序。

---

### 阶段 B6 - Event 实时跟随

**改动**：

- `args.rs:868` `EventSub::List` 已有 `execution/types/limit`，新增 `--workflow --agent-loop --follow --interval <ms>`。
- `cmd/event.rs:14` `List` 增加 `workflow_id/agent_loop_id` 透传 `EventQueryOptions`。
- `EventSub::Follow` 重构为真流式：
  - 非 `follow` 时保持现有 `history` 一次性
  - `follow` 时调用 `infra::subscription::spawn_event_subscription(ctx, filter).await` 得到 `Receiver<UnifiedEvent>`，`tokio::select!(event = rx.recv(), _ = tokio::signal::ctrl_c())` 循环，每事件 `render_envelope(JsonLines)` 或 `println!`（`Text` 时 `[{time}] {type} {executionId}` 行），`Ctrl-C` 优雅退出
  - 增加 `--interval` 用于轮询降级（当 subscription 不可用时 `loop { history + sleep }`）

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/event.rs`

**验收**：`wf event follow <execution-id> -o jsonl` 在执行进行时持续输出行；`Ctrl-C` 退出码 0；`wf event list --workflow wf-1 --types workflow.completed -o json` 过滤生效。

---

### 阶段 B7 - Audit 来源显式化与 Execution Graph 分析

**改动**：

- `cmd/audit.rs:21` `Report` 分支在 `data` 中注入 `source` 字段（`audit::audit_report` 内部已返回 `AuditSource`，需在 `data` 中 `serde_json::json!({"source": format!("{:?}", source), "report": report})`）。
- `args.rs:498` `WorkflowSub::ExecutionGraph` 扩展 `--analysis --slow-nodes --efficiency --path-probability --alternative-paths`：
  - `--analysis` → `execution_graph::analyze(ctx, id).await?`
  - `--slow-nodes` → `execution_graph::get_slow_nodes`
  - `--efficiency` → `execution_graph::analyze_efficiency`
  - `--path-probability` → `execution_graph::get_path_probability_analysis`
  - `--alternative-paths` → `execution_graph::get_alternative_paths`

**涉及文件**：`crates/app/wf-cli/src/cmd/audit.rs`、`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/workflow.rs`

**验收**：`wf audit report <id> -o json` 的 `source` 字段为 `Live|Persisted|CheckpointSnapshot`；`wf workflow execution-graph <id> --analysis -o json` 返回分析视图。

---

## 四、依赖与顺序

```
B1 列表生命周期 ─► B2 Inspect 深度 ─► B5 State 深度
        │
        ├─► B3 实时流 （可与 B2 并行，依赖 B1 的 list 过滤稳定）
        ├─► B4 Checkpoint GC （独立）
        └─► B6 Event 流 （依赖 B1 的 execution id 语义）
                    │
                    └─► B7 Audit+Graph 分析 （最后，依赖 B6 的事件语义）
```

---

## 五、测试

| 用例 | 覆盖 |
| :--- | :--- |
| `tests/execution_lifecycle.rs` | `run --background → status → pause → resume → cancel → delete → cleanup_completed` 全链路 |
| `tests/checkpoint_chain.rs` | `create ×3 → list → chain → delete → gc` |
| `tests/event_follow.rs` | `event follow` 的 `MemorySink` 流式断言（合成事件经 `EventBus::publish` 后被 `history` 捕获，`follow` 的 `--interval 0` 轮询分支） |
| `tests/state_time_travel.rs` | `state --at-iteration 0/1 --variable --most-changed --memory` |

---

## 六、风险

| 风险 | 缓解 |
| :--- | :--- |
| `workflow_execution::stream` 与 `DomainAdapter` 生命周期 | `stream` 的 `ApiContext` 需在 `adapter.shutdown()` 前保持，`follow` 的 `tokio::spawn` 需显式 `drop(rx)` 后再 `shutdown` |
| `event follow` 在 CI 非 TTY 下阻塞 | `follow` 增加 `--once` 降级（非 TTY 时自动单次 `history`），`validate` 中禁止 `follow + Silent` 组合 |

