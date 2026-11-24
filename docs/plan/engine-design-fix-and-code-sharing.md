# Engine 设计审查修复与代码共用方案

> 分析基准：本仓库 commit `00bd8970`。审查报告见 `docs/engine/design-review.md`。
> 性质：实施方案。把审查结论转成可落地的改动清单，标注每项的问题、改动范围、验收标准与取舍。
> 语言约定：代码/注释/日志用英文，本文档用中文。

## 0. 目标与范围

本次针对三类问题落地修改：
1. **客观 bug**：会话订阅计数泄漏（含广播关闭时的忙等）、Pending 落库与在飞槽占用的竞态。
2. **代码共用**：`merge_usage` 逐字重复、`LlmRequest` 字段构造重复、chat turn 校验三件套重复。
3. **文档归属**：把审查报告与形态总览沉淀到稳定位置；对话树现状说明独立成目录。

明确**不做**的项（见第 4 节取舍）：不抽 `wf-chat` crate、不强推流式 sink 抽象、M2 删除竞态与 M3 restricted 晋升留作后续。

## 1. 客观 bug 修复

### 1.1 订阅计数泄漏 + 广播关闭忙等

- 问题：`wf-api/src/conversation/stream.rs` 在 `spawn` 前调用 `runtime.add_subscriber()`，但 `remove_subscriber()` 全仓无调用点、也无 `impl Drop`，`runtime.rs` 的空闲淘汰条件 `subscriber_count() == 0` 因此永不成立，长驻服务随会话数泄漏。同时 `subscription.recv()` 的 `Err(_) => continue` 在广播通道关闭后无限空转。
- 改动：仿照 `wf-server/src/api/agent/loops/detached_handlers.rs` 的 `SubscriberGuard`，在 `conversation/runtime.rs` 新增 RAII 守卫 `SubscriptionGuard`，构造时 `add_subscriber`、析构时 `remove_subscriber`；让 `stream::subscribe` 把守卫随流一起移动，流被 drop 时自动回收。广播 `Err(RecvError::Lagged)` 保留 continue（下一帧快照修复），`Err(RecvError::Closed)` 退出循环。
- 验收：连续订阅/断开同一会话，`subscriber_count()` 回到基线；订阅流 drop 后会话可被空闲淘汰；关闭广播不再空转。

### 1.2 Pending 落库与在飞槽的竞态窗口

- 问题：`wf-api/src/conversation/turn.rs` 的 `submit_turn` 持有写锁，但先 `save_tree` 写入 `Pending` 节点（`:208`），之后才 `begin_turn` 占用在飞槽（`:219`）。读取侧 `store.rs::recover_pending`（经 `load_or_adopt` 调用，不持写锁）在窗口期看到 `Pending` 节点如slot为空，会把它误判为 `Interrupted` 并落库。
- 改动：调整顺序为**先占槽、后落库**。在 `append_turn` 算出 `version`、`node_id` 之后立即 `begin_turn`，再做 `save_tree` 与 `record_session`；若落库阶段出错，需释放刚占的槽（`finish_turn`）以免悬挂。读取侧的判定逻辑不变——占槽先行后，窗口自然消失。
- 验收：`submit_turn` 与 `snapshot`/`path_messages` 并发压测不再出现刚提交即 `Interrupted`；现有会话测试全绿。

## 2. 代码共用

### 2.1 `merge_usage` 去重

- 问题：`wf-api/src/chat.rs:287-310` 与 `conversation/turn.rs:743-765` 是逐字相同的 20 行实现（非零字段覆盖 + `.or()` 回退）。
- 改动：把合并逻辑实现为 `TokenUsageStats` 的方法，放在 `wf-types` 的 `llm` 模块（`crates/foundation/wf-types/src/llm.rs` 新增 `usage_merge` 子模块并 re-export），名为 `merge_non_zero(&mut self, reported: &TokenUsageStats)`；两处调用点委托给该方法，删除本地函数。之所以放 wf-types：两处调用点都在 wf-api、已依赖 wf-types；`TokenUsageStats` 由 `llm-types` 重导出，在此层加方法不碰子模块源码、零环风险。
- 验收：两处调用点不再有本地 `merge_usage`；现有 usage 相关测试通过。

### 2.2 校验三件套去重

- 问题：空文本+空图 / `validate_user_images` / 空 profile 的校验散落在 `wf-execution-shared/src/chat.rs`、`wf-api/src/chat.rs:55-90`、`conversation/turn.rs:84-98`（错误文案略有差异）。
- 改动：保留原语层 `chat.rs` 的校验不动（它在 engine 层、错误类型是 `ExecutionSharedError`）；在 wf-api 内把 app 层共用的"图片校验 + 文本/图片非空"抽到 `infra/validation.rs`（已存在），新增 `validate_chat_input(&str, &[UserImageInput]) -> ApiResult<()>`，chat.rs 与 turn.rs 调用它。profile 校验因取值来源不同（参数/树/会话）保留在各自入口。
- 验收：chat.rs 与 turn.rs 的图片/空消息校验统一调用同一函数；错误文案保持一致。

### 2.3 `LlmRequest` 字段构造去重

- 问题：`LlmRequest` 无 `Default`，19 个字段在 `chat.rs`、`single_shot.rs` 测试、`title.rs`、`recap.rs`、`agent_request.rs`、`wf-workflow/handler/llm.rs`、`handoff.rs` 等 8+ 处逐个列全，新增字段时易漏。
- 改动：在 `wf-types` 的 `llm` 模块新增构造函数 `new_llm_request(profile_id, messages) -> LlmRequest`，把所有可选字段填 `None`/默认，调用方再用字段覆盖或 `..` 结构更新补充少量非默认字段。先迁移**风险最低、语义最标准**的两处（`title.rs`、`recap.rs` 的旁路请求）作为示范，其余在后续 PR 逐步迁移。
- 边界：chat 强制 `tools: None`、agent 需协议对齐、workflow 需 `violation_policy/timeout_ms`——这些调用方必须显式覆盖，构造函数不得擅自推断。
- 验收：示范两处改用构造函数后行为不变；结构更新保证未来新增字段有默认值兜底。

## 3. 文档归属

- `docs/engine/execution-modes.md`：四形态统一说明，留在 `docs/engine/` 根做导航。
- `docs/engine/design-review.md`：审查报告，留在根。
- 本方案：`docs/plan/engine-design-fix-and-code-sharing.md`。
- 后续：对话树现状说明在 `docs/engine/` 新建平级 `conversation/` 目录（本次先不建，避免空目录）；`chat/` 保持平级。

## 4. 明确不做的项与理由

| 项 | 结论 | 理由 |
|----|------|------|
| 抽 `wf-chat` crate 与 agent/workflow 平级 | 不做 | chat 是交互外壳，分派 runChat/runAgent/runWorkflow；独立会形成 `wf-chat→wf-workflow→…→wf-chat` 环，违反严格 DAG；且 `wf-agent`/`wf-workflow` 对 `conversation_tree` 零引用 |
| 对话树纯逻辑下沉到 wf-api | 本次不做，列为后续 | 方向正确（调用方唯一），但涉及十余文件 import 变更，单独 PR 更稳 |
| 流式 sink 抽象 | 本轮已做（纯累加边界） | 只抽 `TurnAccumulator` 做事件分类 + 文本/usage 累加；sink 副作用（mpsc / runtime+events）与断开检查仍在各自调用方，不泛型化传输层 | 详细设计见 `deferred-fixes-implementation-design.md` |
| M2 删除子树未等待在飞收敛 | 本轮已做 | `delete_session` 改为"锁内取消 → 释放锁 → 锁外等收敛 → 重新拿锁删库"，`SessionRuntime` 加 `Notify` 收敛通知 + 30s 超时 abort 兜底 | 详见第 6 节与本轮方案文档 |
| M3 restricted 晋升缺 id 守卫 | 本轮已做（两层） | 应用层 `reject_live_restricted_reuse` 按运行态拦截；引擎层 `registry.register` 拒绝替换非终态 real entity，放行终态晋升与 placeholder 替换 | 原"不做"判断的是"禁止同 id 重用"这一错误命题；真正风险是"运行中同 id 重跑"，详见方案文档 |

## 5. 执行顺序与验收

1. 写本方案文档。
2. 修 H1 订阅泄漏 + 忙等。
3. 修 H2 竞态顺序。
4. merge_usage 抽取 → 校验三件套抽取 → LlmRequest 构造函数示范迁移。
5. `cargo clippy --all-targets --all-features` 全量检查。
6. 关键单测：`conversation` 相关测试、usage 相关测试、chat 相关测试。

## 6. 落地记录（实际实现与方案的偏差）

| 项 | 方案原文 | 实际实现 | 偏差原因 |
|----|---------|---------|---------|
| merge_usage 复用 | 作为 `TokenUsageStats` 的方法放 wf-types | 改为自由函数 `merge_usage_non_zero` 放 `wf-types/src/llm/usage_merge.rs` | `TokenUsageStats` 定义在外部 crate `llm-types`，按孤儿规则不能在 wf-types 为其实现 inherent 方法；自由函数同等复用且零环风险 |
| 校验三件套 | 抽到 `infra/validation.rs` | 一致：`validate_chat_input`，chat.rs 与 turn.rs 均调用；顺带移除 turn.rs 重复的前置判空，两处路径完全统一 | 无 |
| LlmRequest 构造 | wf-types 构造函数 + title/recap 示范 | 一致：`new_llm_request` 放 `llm/request_builder.rs`，`build_title_request`/`build_recap_request` 用结构更新语法覆盖非默认字段 | 无 |

新增文件：`wf-types/src/llm/usage_merge.rs`、`llm/request_builder.rs`（均含单测）。
`add_subscriber`/`remove_subscriber` 收敛为 `SubscriptionGuard` 私有方法，外部只经 RAII 守卫使用。

### 验收结果（第一阶段）
- `cargo check -p wf-types -p wf-execution-shared -p wf-api`：通过。
- `cargo clippy --all-targets --all-features`：通过（仅既有 workspace 字段与 wf-server 告警，与本次改动无关；改动文件无新增告警）。
- `cargo test -p wf-types --lib`：149 passed（含新增 4 例 usage/request 单测）。
- `cargo test -p wf-api --lib conversation`：7 passed。

## 7. 第二阶段：三项暂缓项的落地（本轮）

详细设计见 `docs/plan/deferred-fixes-implementation-design.md`。三项均已完成，落地决策如下：

| 项 | 落地实现 | 关键取舍 |
|----|---------|---------|
| M3 restricted 守卫 | 应用层 `agent_execution/lifecycle.rs` 的 `reject_live_restricted_reuse`（run/stream 两入口）；引擎层 `registry.rs` 的 `register` 拒绝替换非终态 real entity | 用 `effective_config().is_none()` 判 placeholder（放行 spawn 替换），终态实体放行（保留 restricted 晋升与 in-place resume）；只拦"非终态 real 被替换" |
| M2 删除竞态 | `conversation/runtime.rs` 加 `settled: Notify` + `settle()`；`finish_turn`/`finish_job` 真正释放 slot 后唤醒；`surgery.rs` 的 `delete_session` 两段式（锁内取消→锁外等收敛→锁内删库），30s 超时 abort 兜底 | 等待不持写锁避免死锁；`notified()` 先注册再复查 slot 避免丢唤醒；超时兜底用已在 spawn 时注册的 `execution_tasks.abort` |
| 流式 sink 抽象 | 新增 `wf-execution-shared/src/turn_accumulator.rs` 的 `TurnAccumulator`/`AccumAction`；chat.rs 与 conversation/turn.rs 的消费循环改用累加器 | 只抽纯累加（事件分类 + 文本/usage/refusal 折叠），sink 副作用与断开/取消检查留在各调用方；agent forwarder（不同事件类型、纯转发）不入抽象 |

新增文件：`wf-execution-shared/src/turn_accumulator.rs`（含 2 单测）；`registry.rs` 新增 2 单测（拒绝替换 live real、允许替换 terminal real）。
`AgentLoopEntity::status()` 用 trait 方法读状态，`effective_config()` 是 inherent 方法；守卫只读 live registry，与读持久化历史的 ephemeral 守卫互补。

### 第二阶段验收结果
- `cargo check -p wf-types -p wf-execution-shared -p wf-agent -p wf-api`：通过。
- `cargo clippy -p wf-types -p wf-execution-shared -p wf-agent -p wf-api --all-targets`：改动处无新增告警。
- `cargo test -p wf-execution-shared --lib turn_accumulator`：2 passed。
- `cargo test -p wf-agent --lib`：214 passed（含新增 2 例守卫单测）。
- `cargo test -p wf-api --lib`：568 passed。
- `cargo test -p wf-api --lib conversation`：7 passed。
