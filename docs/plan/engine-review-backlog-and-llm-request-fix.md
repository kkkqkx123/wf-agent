# Engine 审查任务清单与 LlmRequest 构造统一方案

> 分析基准：HEAD `aa455a66`。本文分两部分——第一部分是 engine 形态与架构审查的完整任务清单（含状态），第二部分是本次落地的代码修改方案。
> 语言约定：代码/注释/日志用英文，本文档用中文。
> 关联文档：`docs/engine/execution-modes.md`（四形态总览）、`docs/engine/design-review.md`（审查报告）、`docs/plan/engine-design-fix-and-code-sharing.md`（上一轮修复方案）、`/workspace/wf-agent-analysis.md`（本次分析结论）。

## 第一部分：完整任务清单

状态标记：`已完成` / `本次完成` / `待办` / `不做`。

### A. 分析与文档类

| # | 任务 | 结论/产物 | 状态 |
|---|------|-----------|------|
| A1 | 梳理 chat session / single shot / agent loop / 临时对话四形态区别 | `docs/engine/execution-modes.md`，结论为刻意分层、保留现状 | 已完成 |
| A2 | engine 架构与职责划分审查 | `docs/engine/design-review.md`，分层合理但有技术债 | 已完成 |
| A3 | 对当前 HEAD 复核文档事实（旧基准 `00bd8970` 行号已漂移） | 语义成立、行号失效，见本次分析 | 已完成 |
| A4 | 补 `docs/engine/conversation/` 平级目录（对话树现状说明） | 文档侧支持，零代码风险 | 待办 |
| A5 | 修正文档失真 D1–D3（设计文档行号、收敛承诺、测试覆盖口径） | 文档债 | 待办 |

### B. 上一轮已偿还的技术债（复核确认在 HEAD 落地）

| # | 任务 | 关键落点 | 状态 |
|---|------|----------|------|
| B1 | 会话订阅计数泄漏 + 广播关闭忙等 | `conversation/runtime.rs` 的 `SubscriptionGuard`（RAII Drop） | 已完成 |
| B2 | Pending 落库与在飞槽竞态 | `conversation/turn.rs` 先占槽后落库，失败释放 | 已完成 |
| B3 | 删除子树未等待在飞收敛（M2） | `runtime.rs` 的 `settled: Notify` + 两段式删除 | 已完成 |
| B4 | restricted 晋升缺 id 守卫（M3） | `agent_execution/lifecycle.rs` 的 `reject_live_restricted_reuse` + registry 层 | 已完成 |
| B5 | `merge_usage` 逐字重复 | `wf-types::llm::merge_usage_non_zero` | 已完成 |
| B6 | chat turn 校验三件套重复 | `wf-api::infra::validation::validate_chat_input` | 已完成 |
| B7 | 流式消费循环同构 | `wf-execution-shared::turn_accumulator::TurnAccumulator` | 已完成 |
| B8 | `LlmRequest` 构造去重——示范迁移 `title.rs` / `recap.rs` | `wf-types::llm::new_llm_request`（19 字段 builder） | 已完成 |

### C. 本次代码修改

| # | 任务 | 范围 | 状态 |
|---|------|------|------|
| C1 | 将剩余生产代码中的 `LlmRequest` 全字段构造统一到 `new_llm_request` | 7 文件 / 9 处构造点，详见第二部分 | 本次完成 |
| C2 | 编译与测试验证（check / clippy / 相关单测） | 改动 crate | 本次完成 |

### D. 结构性改进（后续决策，本次不做）

| # | 任务 | 结论 | 状态 |
|---|------|------|------|
| D1 | 把 `conversation_tree` 纯逻辑下沉到唯一调用方 `wf-api` | 方向正确（调用方唯一），但涉及十余文件 import 变更，单独 PR 更稳 | 待办 |
| D2 | 抽取独立 `wf-chat` crate | **不做**：会形成 `wf-chat → wf-workflow → … → wf-chat` 环，违反严格 DAG | 不做 |
| D3 | 线性 chat 与对话树双轨统一（同一 `session_id` 入口互斥） | 需明确会话执行类型标记，属产品+架构联合决策 | 待办 |
| D4 | 强行统一四种形态抽象 / 合并两类会话历史装配 | **不做**：会抹平抽象层差异；`projection.rs`（MessageRef）与 `conversation_session/history.rs`（消息内容）语义不同，`turn.rs` 已组合复用 | 不做 |
| D5 | 多路 `generate_stream` 驱动泛型化（agent / workflow / wf-api） | sink 副作用差异大，收益/风险不划算，维持现状 | 不做 |

---

## 第二部分：本次修改方案——LlmRequest 构造统一

### 1. 目标与收益

`LlmRequest` 是 19 字段、无 `Default` 的协议结构体。历史上有 8+ 处构造点逐字列全所有字段，每新增协议字段都要改所有调用点，易遗漏。上一轮已引入 `wf-types::llm::new_llm_request(profile_id, messages)`（可选字段一律填中性值），并示范迁移 `title.rs`/`recap.rs`。本次把**剩余生产代码构造点**全部迁到该 builder。

模式：用结构更新语法 `LlmRequest { 本路径特有的非默认字段, ..new_llm_request(profile, messages) }`；全默认构造直接 `new_llm_request(profile, messages)`。

### 2. 边界原则（必须保留的显式覆盖）

builder 绝不推断路径语义，以下字段由各调用点显式覆盖，行为与改动前逐字段等价：

- **chat 路径**：`tools` 恒为 `None`（类型层无工具）；保留 `parameters` / `generation` / `execution_id`。
- **agent 路径**：`tool_call_protocol` / `locked_tool_call_protocol` 需与实体配置对齐；`tools` 由工具池解析结果决定；保留 `execution_id` / `stream`；纯 chat 分支 `tools` 为 `None`。
- **workflow LLM 节点**：保留 `parameters` / `generation` / `tools` / `violation_policy` / `dead_loop_detection` / `timeout_ms` / 协议字段 / `execution_id`。
- **审批评审**：保留 `tools`（verdict 工具）/ `execution_id`。retry 构造已是 `..request` 结构更新，不改。

### 3. 逐点清单

| 文件 | 位置 | 迁移方式 | 显式覆盖字段 |
|------|------|----------|--------------|
| `crates/engine/wf-agent/src/agent_request.rs` | `:55` 纯 chat 分支 | 结构更新 | `tool_call_protocol` / `locked_tool_call_protocol` / `execution_id` / `stream` |
| 同上 | `:183` 常规分支 | 结构更新 | 上述 + `tools` |
| `crates/engine/wf-execution-shared/src/chat.rs` | `:117` | 结构更新 | `parameters` / `generation` / `execution_id` |
| `crates/engine/wf-workflow/src/handler/llm.rs` | `:103` | 结构更新 | `parameters` / `generation` / `tools` / 协议字段 / `violation_policy` / `execution_id` / `dead_loop_detection` / `timeout_ms` |
| `crates/engine/wf-agent/src/coordinator/iteration/handoff.rs` | `:135` / `:175` | 直接 builder（全默认） | 无 |
| `crates/engine/wf-execution-shared/src/interactive_script_session/driver.rs` | `:101` | 结构更新 | `execution_id` |
| `crates/app/wf-runtime/src/tool_approval.rs` | `:246` | 直接 builder（全默认） | 无 |
| `crates/app/wf-api/src/checkpoint/approval.rs` | `:234` | 结构更新 | `tools` / `execution_id`（`:272` retry 保留原样） |

### 4. 明确不改

- 子模块 `crates/infra/llm-suite/`（`llm-suite-types` / codecs / gateway 等）属外部依赖，其测试辅助构造点不在范围。
- `#[cfg(test)]` 下的测试构造 helper（如 `single_shot.rs:264`、`token_budget.rs:547`、`codecs_plugin.rs:184`、`wf-api/src/llm.rs:188`）收益低、风险近零但非生产路径，本轮不动。
- 不修改 `LlmRequest` 协议结构体本身，不为其加 `Default`（会掩盖必填语义）。

### 5. 验收标准

1. 9 处构造点不再逐字列出 19 字段；新增协议字段时这些路径自动获得中性默认值。
2. 各路径显式覆盖字段与改动前逐一等价，尤其 chat 无工具、agent 协议对齐、workflow 超时/违规策略。
3. `cargo check` / `cargo clippy` 改动 crate 通过且无新增告警；相关单测通过。
4. 符合 AGENTS.md：英文代码/注释、无 `unwrap`、严格 DAG 不变。
