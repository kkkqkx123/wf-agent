# 检查点与恢复

> 范围：`crates/engine/wf-agent/src/checkpoint.rs`、`checkpoint/strategy.rs`、`checkpoint/coordinator.rs`。

## 1. 策略：`AgentCheckpointStrategy`（`checkpoint/strategy.rs`）

- `AgentCheckpointTiming`（`strategy.rs:8-28`）：agent 级触发时机枚举——迭代前后、迭代错误、生命周期（start/end/pause/cancel/timeout）、工具前后、压缩前后、消息数兜底 `MessageInterval`，通过 `to_trigger()` 映射到统一的 `CheckpointTiming`（`strategy.rs:31-47`）。
- `AgentCheckpointStrategy`（`strategy.rs:71-73`）包装 `CadencedCheckpointStrategy`，提供 `never` / `every_iteration`（默认，`strategy.rs:88-96`）/ `every_n_iterations` / `on_error` 等预置。
- `from_agent_config()`（`strategy.rs:132-179`）集中装配：迭代节奏、错误快照、工具边界（默认开）、压缩边界（默认开）、消息数兜底（需配置 `message_interval`）；**生命周期边界（start/complete/pause/终态）始终开启**。`should_checkpoint()` 是统一判定入口（`strategy.rs:187-190`）。

## 2. 协调器：`AgentCheckpointIntegration`（`checkpoint/coordinator.rs`）

组合内部 coordinator、`StorageBackend`、策略与可选的文件检查点管理器/事件总线（`coordinator.rs:85-91`）。

### 创建路径分三类（门控递进）

| 路径 | 门控 | 行号 |
|------|------|------|
| `create_checkpoint_gated` | 迭代/工具边界走策略节奏门控 | `coordinator.rs:191-207` |
| `create_hook_checkpoint` | hook 主动请求，绕过节奏但尊重总开关/域开关，失败仅告警 | `coordinator.rs:215-245` |
| `create_lifecycle_checkpoint` / `on_pause` | 绕过节奏，保证暂停/终态持久化；生命周期触发时同步等待文件快照投影完成 | `coordinator.rs:252-275`、`:460-466`、`:421-423` |

### 并发正确性

- `create_checkpoint`（`coordinator.rs:314-454`）先做**进度坐标去重门**（`gate_latest_or_report` + `entity_progress_coords`，坐标含 seq_start/seq_end/seq_next/迭代数/工具数/状态/pending 数，`coordinator.rs:710-737`）——坐标相同则合并回最新行不落新行（`coordinator.rs:331-348`）。
- 再以 `check_predecessor_unchanged` 做**链头校验**，链头移动则用新进度重建一次，仍冲突则报错要求重试（`coordinator.rs:379-406`）。

### 快照构建

`build_snapshot`（`coordinator.rs:759-920`）完整捕获：状态 + 对话（消息、视图、序列号、估算 ledger、tracker、compaction 簿记）、变量、错误/中断/事件记录、迭代轨迹、流式缓冲、pending 工具 id、层级、声明式 `loop_config`。

## 3. 恢复

`restore_entity` 产出 `RestoredAgentLoop`（`coordinator.rs:31-48`）——对话状态权威：历史、序列坐标、读投影、ledger、tracker 一并恢复。

- **`RestoreMode`**（`coordinator.rs:56-60`）：
  - `Branch`：用新执行 id 继续（默认，利于审计）；
  - `Replay`：只读；
  - `InPlace`：复用原 id（须保证源已终态/暂停）。
- 恢复时从迭代轨迹重建 `completed_tool_results` 幂等表（`coordinator.rs:616-628`）；损坏的 tracker 视为损坏快照而非缺失（`coordinator.rs:573-582`）。
- 另提供 `preview_through_seq`（序列前缀只读预览）与 `timeline`（按序列区间排列的检查点时间轴，`checkpoint.rs:8-14`）。

## 4. 与迭代的协作

- 迭代内边界 checkpoint 由 `boundary_checkpoint` 走策略门控，失败只 warn 不破坏迭代；消息数兜底走水位线机制（`coordinator/iteration/checkpoint.rs:9, 43`）。
- 暂停时持久化 `cp.on_pause`（仅 Paused 状态触发，每暂停回合快照一次）。
- 压缩完成后经策略门控触发 `AfterCompression` 检查点（见 05-compression.md）。
