# 当前项目现状与差距分析

> 分析对象：`/workspace`（模块化 agent 框架）。基线日期 2026-10-26。
> 结论先行：检查点的“元件”已经齐全且质量较高，缺的是**回合**这一层索引、**会话锚**这一条主线，以及面向用户的回溯入口。

## 一、检查点子系统分层

`crates/infra/checkpoint/` 按四层拆分，上层只依赖门面：

| 层 | crate | 现状 |
|---|---|---|
| 叶子 | `checkpoint-base` | 错误、actor、策略、差异、增量计算 |
| 状态 | `checkpoint-state` | 执行状态快照与恢复，Full/Delta 链 |
| 文件 | `checkpoint-file` | Git 对象库、SQLite 元数据、文件历史、分支、冲突、GC |
| 门面 | `wf-checkpoint` | 协调器、投影重试、状态与文件的关联，对上统一导出 |

这个分层对回溯很关键：回溯需要的两侧（会话状态、工作区文件）都已经有独立实现，回溯本身只应做**组合与索引**，不应重新实现捕获与恢复。

## 二、文件检查点已有能力

存储形态：

- 依赖 Git 对象库保存文件树与 blob，引用限定在 `refs/wf/` 命名空间：`refs/wf/main` 为主线，`refs/wf/edit/<actor>` 为单个 actor 的编辑线，`refs/wf/review/*` 为待评审提交，`refs/wf/feat/*` 为协作目标，`refs/wf/human` 为人类外部编辑。
- 元数据落 SQLite（WAL 模式），迁移脚本在 `checkpoint-file/src/storage/migrations.rs`，已有 `meta_kv`、`source_index`、`redo_stacks`、`empty_dirs`、`oversized_files` 等表；迁移是幂等的。
- 提交使用 `commit_on_ref` 的 CAS 重试，引用写入与删除有独立封装。

捕获能力：

- `FileCheckpointManager` 以 actor 分区（`ActorCache`），会话作用域与待提交批次都在管理器内。
- `create_workspace_checkpoint` 扫描工作区、判定删除、记录 gitlink 与空目录、按大小上限排除超大文件（清单单独入库，恢复时永不删除）。
- 提供自包含性判定（`self_contained` / `self_contained_blocker`）与基于借用基线的实现模式。

恢复能力：

- `restore_checkpoint`、`restore_latest`、`restore_workspace`、`restore_workspace_scoped`、`restore_selected`（按文件标识到目标内容的映射恢复指定文件集合）、`restore_latest_workspace`。
- 恢复结果结构区分已恢复、已删除、跳过、跳过嵌套、恢复的 gitlink。
- 状态与文件的关联：`record_state_file_link` 与 `state_file_link_status`（已关联 / 待处理 / 缺失），`restore_state_files` 按关联恢复。

历史与治理：

- 撤销/重做：`undo` 与 `redo_store`，重做栈按 actor 持久化，重启后仍可重做。
- 评审、冲突、冲突解决、分支、合并、回滚特性线、编辑会话、索引重建。
- GC：`GcRetention { keep_recent_heads }`、`StoreStats { commits, refs, loose_objects, store_bytes, last_gc_ms }`，对外暴露存储统计与手动回收。

## 三、执行状态检查点已有能力

- `AgentStateSnapshot`（在 `wf-types/src/checkpoint/agent/snapshot.rs`）已经包含 `conversation_snapshot`、`conversation_view`、`message_seq_start` / `message_seq_end` / `message_next_seq`、`conversation_ledger`、`conversation_tracker`、`loop_config`、迭代历史、待处理工具调用标识等。
- 消息序号语义已在捕获侧落地：`wf-agent` 的检查点协调器在拍快照时读取会话的 `seqs`、`next_seq`，写入快照的三个序号字段。
- Full/Delta 链：`base_checkpoint_id`、`previous_checkpoint_id`、`chain_root_id`、`chain_position`，支持按链恢复与压缩策略。
- 恢复入口：`resume_from_checkpoint`（校验归属后按模式恢复并回写会话）、`auto_resume`（依赖快照里的 `loop_config` 自动恢复）。

这意味着“把会话状态恢复成某个检查点的样子”在引擎侧已经打通，回溯不必新造状态恢复机制。

## 四、触发时机与门控

- `CheckpointTiming` 覆盖执行前、执行后、出错、工具前后、压缩前、完成、按间隔、手动、从不等。
- 三条创建路径：`create_checkpoint_gated`（按策略节奏决定是否真正落盘）、`create_lifecycle_checkpoint`（绕过节奏，生命周期必拍）、`create_hook_checkpoint`（钩子触发）。
- 迭代边界另有 `boundary_checkpoint` 与按消息间隔触发的 `maybe_message_checkpoint`。

这条设计让“逐回合再额外拍两次”显得不必要：回合开始与结束的邻近位置通常已有检查点，回溯账本应优先引用它们。

## 五、应用门面与传输层已有接口

门面（`crates/app/wf-api`）：

- 文件域：创建文件检查点、从检查点恢复工作区、扫描工作区、列出文件变更、按 actor 与主线做差异、列出分区与冲突、获取 actor 工作区。
- 撤销域：预览撤销上次输入、应用撤销（含校验版本）。
- 记录域：保存、读取、删除、打标签、按实体/执行列举、取最新、取链、按时间范围列举、按执行 GC。
- 工作流域：预览恢复、带文件恢复（`restore_checkpoint_with_files`，状态优先、文件侧给出禁用/缺失/失败/已落盘四种结果）、关联文件摘要。
- agent 域：运行/流式/暂停/恢复/取消、`resume_from_checkpoint`、`persist_conversation`。

传输层（`crates/app/wf-server`）：

- `/api/v1/checkpoints/*`：列表、详情、标签、链、恢复、恢复预览、恢复并继续、GC、按实体、按时间范围、最新。
- `/api/v1/file-checkpoint/*`：创建、恢复、分区、冲突、变更、内容、树、GC 与统计、状态关联、会话、按会话回滚、撤销、重做、撤销上次输入预览与应用。

接口密度已经很高，回溯需要的是**新增一个会话级读模型与一个组合动作**，而不是重做这些端点。

## 六、关键缺口：会话没有稳定锚点

这是本次设计里最重要的现状结论。

- 命令行每个标签页用本地生成的标识作为 `execution_id`（`tui/tabs.rs` 的 `new_tab_with_parent`），并只把它当作请求路径的一部分。
- 服务端的 agent loop 请求体（`RunAgentLoopBody`）**没有会话字段**；服务端 `agent_loop_id` 每次运行重新生成，并把它作为消息归属写入存储。
- 客户端每回合把完整 `conversation` 回传，服务端据此“看到”历史；服务端的 `conversation_history` 只按 `agent_loop_id` 查询，因此跨回合无法拼成一个会话。
- 网页侧同样：会话标识取 URL 的 `selectedId`，为空时用新会话常量作为请求路径标识，历史同样由客户端回传。
- 对比之下，`chat` 域已经用 `session_id` 锚定会话（`ChatTurnParams` / 聊天路由请求体都带 `session_id`，并按会话解析历史），说明“会话锚”在本仓库已有先例，只是 agent loop 域没有。

后果：

1. 无法按会话列出“这个会话有哪些回合”，回溯面板没有数据源。
2. 无法在没有客户端历史的情况下重建会话，`--resume` 只能依赖回放与本地状态。
3. 消息按运行分散存储，且存在重复回写风险（已有去重清理函数），回溯若直接按消息删除会放大不一致。

## 七、命令行现状

- 命令表 `tui-core/src/command.rs` 的 `COMMANDS` 是命令、快捷键、斜杠、面板的唯一事实源；当前**没有**回溯、撤销、检查点相关斜杠命令。
- 低层动作 `KeyAction` 无回溯相关取值；`Esc` 目前解析为取消（键位表测试直接断言）。
- 命令面板路由为 `FooterRoute`，面板状态为 `PanelState`（命令、模型、技能、排队、工作流、提及、认证），均在 `tui-components/src/footer.rs`；新增面板需要同步路由标识与绘制高度。
- 交互态已有双击退出模式（Ctrl-C 连按）与 `InteractiveEvent::PanelCandidates` 异步候选回填机制，可复用于回溯点拉取。
- 回合发起在 `interactive/turn.rs` 的 `start_turn`：构造 `TurnParams`（含完整会话历史）并走 `/agent-loops/cli/stream`，服务端忽略路径标识。
- 标签页有独立会话持有者与重连状态机，具备“会话标识迁移、重连、释放”的基础设施，可作为会话锚的承载点。

## 八、网页现状

- `services/checkpoints.ts` 已封装检查点列表、循环检查点、文件变更分页、恢复与预览、恢复并继续、差异、文件内容与树、时间线、审批、分区、冲突、编辑会话、撤销/重做、索引、评审、合并、GC、状态关联、执行检查点等大量接口。
- `chat-session.svelte.ts` 的 agent 运行把 `selectedId`（或新会话常量）作为请求路径标识，并把完整会话历史放进请求体。
- 消息气泡已具备重试、继续、反馈类动作，回溯按钮可以同型扩展。
- 现状没有任何“回到某回合”的视图或状态。

## 九、差距矩阵

| 回溯所需 | 现状 | 差距 |
|---|---|---|
| 回合索引（提示词、边界、前后树） | 无 | 需新增账本 |
| 会话稳定标识 | agent loop 域无，chat 域有 | 需在 agent loop 请求与消息元数据上补会话锚 |
| 回合边界前后的文件树 | 检查点链已按迭代产生 | 需做“按回合取最近边界”的映射，缺时补拍 |
| 会话状态恢复 | 已有快照与恢复入口 | 语义需从“续跑”扩展为“退回到某回合之前” |
| 文件选择性恢复 | `restore_selected` 已具备 | 需补冲突判定、恢复树、事务与缺失预检 |
| 组合动作与失败回滚 | 带文件恢复已有状态优先语义 | 需新增回溯专用事务与补偿方向 |
| 传输接口 | 检查点与文件域接口丰富 | 缺会话级回溯列表、预览、应用 |
| 命令行入口 | 命令表、面板、异步候选齐备 | 缺命令项、面板状态、双击 Esc 手势 |
| 网页入口 | 消息动作、服务层封装齐备 | 缺按钮、模态、服务函数与状态回填 |
| 磁盘余量与存储预算 | 有存储统计，无可用空间探测 | 需新增余量检查与预算闸门 |
| 缺失对象降级 | Git 对象库有读取错误路径 | 缺批量预检与“仅会话”投影 |

## 十、可复用点与主要风险

可直接复用：

- `AgentStateSnapshot` 的消息序号三件套与 `loop_config`，是会话边界最干净的坐标。
- `FileCheckpointManager` 的 actor/会话作用域与 `restore_selected`，是文件侧回退的现成能力。
- `restore_checkpoint_with_files` 的“状态优先、文件尽力”语义，可作为回溯失败降级的范本。
- `wf-checkpoint` 的投影重试与 `spawn_blocking` 模式，避免阻塞运行时。
- 命令表 + 面板路由 + 异步候选事件，是命令行入口的唯一改动面。
- 网页服务层的既有封装风格与消息气泡动作扩展点。

主要风险：

1. 会话锚改造牵动消息持久化与历史读取，需与 `--resume`、回放、去重逻辑一起验收。
2. 回合边界与既有检查点节奏可能错位（例如回合内无任何检查点），必须有按需补拍兜底。
3. 回溯是破坏性操作，冲突判定与“先拍恢复树”不能省，否则用户手工改动会被静默吞掉。
4. 存储回收与账本提交顺序必须显式设计，否则崩溃后不可恢复。
5. 命令行与网页若各自实现截断逻辑，会和后端账本分叉；会话回退必须以服务端返回的会话为准。
