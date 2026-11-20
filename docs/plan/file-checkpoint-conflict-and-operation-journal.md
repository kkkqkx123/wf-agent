# 文件检查点冲突处理与操作账本补充设计

## 1. 背景

当前文件检查点已经具备三方合并、冲突门禁、冲突列举与整文件解决能力，但存在三类待收敛问题：

1. 冲突行为策略只在人工审批入口完整生效，自动合并与多特性合入路径没有统一使用同一决策点。
2. 冲突标记存在两套近似解析实现，后续格式演进时可能漂移。
3. ref 指针变化没有持久账本：`undo_edit`、`redo_edit`、编辑组回退、审批合并、冲突解决等操作只留下最终 ref 与提交图，缺少“谁在何时把哪个 ref 从哪个提交改到哪个提交”的记录。Git reflog 被显式关闭，因此指针变化历史不可重建。

此外，undo 与 redo 栈写入不是同一个原子边界。当前顺序是先 CAS 移动 ref，再写 SQLite redo 栈；若进程在两步之间退出，会得到“已撤销但无法重做”的状态。已有持久化 roundtrip 测试不能覆盖这个崩溃窗口。

本文给出下一阶段设计方案。目标是补齐上述缺口，不引入新的执行引擎、存储后端或全局撤销系统。

## 2. 范围与原则

### 2.1 纳入范围

- 统一 `ConflictBehavior` 在所有合并入口的语义。
- 收敛冲突标记解析为单一实现。
- 在 `checkpoint-file` 内增加 ref 操作账本与启动对账。
- 关闭 undo/redo 与 ref 更新之间的持久化窗口。
- 在检查点文档中明确三种回退机制的边界。

### 2.2 不纳入范围

- 不引入 jj-lib 或 jj operation log。
- 不实现仓库级全局 undo/redo。
- 不实现按 hunk 选择 ours/theirs。
- 不实现 rename-aware 合并；rename 仍按 delete + add 参与三方合并。
- 不改变现有 Git 对象库、ref 命名、提交 trailer 或 40 位十六进制 ID 约定。
- 不新增替代 crate，不保留双实现或兼容映射。

### 2.3 设计原则

- Git ref 与提交图仍是文件内容的唯一事实来源；账本只记录操作意图与附带状态，不参与当前 head 判定。
- 账本写入必须先于 ref CAS；完成状态必须在附带 SQLite 状态落定后写入。
- 对账只做幂等补偿，不猜测、不覆盖当前 ref。
- 冲突状态继续由 `Wf-State` / `Wf-Conflict-File` trailer 决定，文件内容中的标记只用于展示和人工编辑。
- 所有补偿路径都必须是“设置到已知结果”，不能是重复追加或重复弹出。

## 3. 冲突行为策略统一

### 3.1 现状问题

`ConflictBehavior` 定义三类行为：

- `Marker`：写入标准冲突标记并返回冲突视图；
- `Fail`：检测到冲突即失败，不产生合并提交；
- `Approval`：保持待审批，不执行合并。

当前该策略主要在 `file/approval/approve.rs` 的人工审批路径生效。以下入口没有统一接收该策略：

- `merge_entity_changes`；
- `on_agent_complete` 的 `ApprovalPolicy::Auto` 路径；
- `merge_features_to_main_with_options`；
- `merge_feature_into_main`。

结果是配置 `Fail` 时，自动路径仍可能创建带冲突标记的 feature 提交。

### 3.2 目标语义

所有会产生合并提交的入口必须调用同一个策略化执行函数。策略只定义在“检测到冲突后怎么处理”，不由每个入口自行解释：

| 策略 | 检测阶段 | ref 动作 | 返回 |
| --- | --- | --- | --- |
| `Marker` | 预计算三方合并结果 | 创建冲突提交并移动 feature ref；main 不动 | 返回带 `conflict_files` 和区间视图的结果 |
| `Fail` | 在任何 ref 更新前预检 | 不创建提交，不移动 ref | 返回 `MergeConflict` |
| `Approval` | 不进入合并执行 | 保留 review ref 与 pending 状态 | 返回未合并结果 |

补充约束：

- `ApprovalPolicy::Auto` 仍决定“是否提交并尝试合并”，`ConflictBehavior` 决定“冲突时怎么办”。
- `Auto + Fail` 时自动合并失败并记告警，不允许降级为 `Marker`。
- `Auto + Approval` 时提交审查后保持 pending，不创建 feature 合并提交。
- 多特性合入 main 的 `atomic` 预检继续复用只读预测；`Fail` 必须在任何一个 main 合并提交之前完成全部预测。
- 低层合并函数不应再在冲突时直接返回裸错误来驱动策略；应返回结构化冲突结果，由统一策略层决定是转换成错误还是成功结果。

### 3.3 实现落点

- 在 `file/git_merge.rs` 中提取无副作用的合并计划计算：输入 base/ours/theirs 三棵树，输出合并内容、冲突路径和冲突区间。该计算同时供预览和实际合并使用。
- `approve_changes`、`merge_entity_changes`、`merge_features_to_main_with_options`、`on_agent_complete` 统一调用策略化合并入口。
- 保留现有 `GitMergeOutcome`、`MergeOutcome`、`GitConflictDetail` 等读模型；只收敛决策路径，不改变对外字段语义。
- `preview_merge_features_to_main` 与执行路径共享同一合并计划计算，避免预测和实际结果因实现分叉而不同。

## 4. 冲突标记解析收敛

### 4.1 现状问题

`file/git_merge.rs` 的 `parse_conflict_regions` 与 `provenance/conflicts.rs` 的 `parse_marker_conflicts` 分别实现了标记扫描。两者都支持 diff3 三段标记、旧式两段标记和未闭合标记，但输出类型不同，状态机重复。

### 4.2 目标设计

- 以 `file/git_merge.rs` 中的区间解析为唯一底层实现。
- 底层解析器只负责从字节中解析冲突区间，输出包含起始行、结束行、base/ours/theirs 内容的中性区间记录。
- `provenance/conflicts.rs` 删除自己的状态机，调用统一解析器后映射为 `ConflictView`。
- `approval.rs` 的 `ConflictView::to_conflict_marker` 与 `inject_conflict_markers` 继续负责展示与注入，不参与门禁判定。

### 4.3 必须保持的语义

统一后必须保留现有测试覆盖的行为：

- diff3 标记带 base 区间；
- 旧式两段标记不产生 base 区间；
- 未闭合标记按文件尾作为结束；
- 干净文件返回空区间；
- 二进制冲突只有文件级记录，不产生区间；
- 多个区间的起始行是合并输出中的真实行号。

## 5. Ref 操作账本

### 5.1 目标

记录所有由文件检查点管理器发起的 `refs/wf/*` 指针变化，形成可审计、可恢复的操作链。账本不是全局撤销系统，也不提供任意历史时间旅行；本阶段只用于：

- 查询 ref 指针变化历史；
- 启动补偿未完成的附带状态；
- 为后续受控逆向操作提供可靠依据。

### 5.2 记录范围

必须记录的管理器操作：

- 普通文件提交与编辑组提交；
- `undo_edit`、`redo_edit`；
- 编辑组回退；
- review 提交、批准、驳回；
- review 合入 feature；
- feature 合入 main；
- feature revert；
- 冲突解决提交；
- 上下文归档 ref 写入；
- 外部同步产生的受管 ref 更新。

不记录：

- blob/tree/commit 对象写入；
- 纯 SQL 元数据更新；
- 用户仓库自身的提交与 ref 操作；
- GC 的对象删除。

### 5.3 持久化结构

在现有 `checkpoint-file` SQLite 元数据库中扩展表，沿用 `storage/migrations.rs` 的初始化入口，不新建数据库。

账本记录应包含以下信息：

- 单调递增的操作序号；
- 全局唯一操作标识，供幂等补偿使用；
- 操作类型与人类可读原因；
- actor、session、tool；
- 目标 ref；
- 本次 CAS 期望的旧 id 与新 id；
- 关联 ref 列表，例如 review 提交对应的 feature ref；
- 操作完成后 redo 栈的目标快照；
- 需要同步落定的附带状态摘要；
- 状态：`intent`、`applied`、`abandoned`、`diverged`；
- 创建时间与完成时间。

附带状态摘要必须表达“最终值”，不允许只记录增量。例如 undo 记录该 actor 的 redo 栈最终内容，而不是“追加一个 id”。这样重复补偿不会重复写入。

### 5.4 写入协议

每次 ref 更新按以下顺序执行：

1. 读取当前 ref head，计算新 head 与附带状态最终值。
2. 写入或更新 `intent` 记录。记录中的旧 id、新 id 是本次 CAS 的期望值。
3. 执行 ref CAS。
4. CAS 冲突时，用新读到的 head 更新同一条 `intent` 记录并重试，不产生多条试探记录。
5. CAS 成功后，写入附带状态：redo 栈快照、review 状态、source index 等。
6. 将记录置为 `applied`。

对象写入仍在 ref 更新之前完成，保持现有“不会让 ref 指向缺失对象”的性质。

### 5.5 启动对账

在 `app/wf-runtime/src/bootstrap/file_checkpoint.rs` 中，manager 创建后、GC 启动前执行对账，与现有 `retry_pending_state_file_links()` 并列。

对每条 `intent` 记录：

- 当前 ref 等于旧 id：CAS 未落地，置为 `abandoned`。
- 当前 ref 等于新 id：CAS 已落地，重放附带状态并置为 `applied`。
- 当前 ref 与两者都不同：检查是否存在后续账本记录可以连接当前链。能连接则按顺序补记前序操作为 `applied`；不能连接则置为 `diverged`，发事件并保留记录，绝不自动改 ref。

对账必须按同一目标 ref 的操作序号顺序执行。redo 栈快照等附带状态按操作序号从旧到新重放，最终收敛到最新已落地操作对应的值。

### 5.6 账本压缩

账本随 GC 一起压缩：

- 只删除 `applied` 且超过保留期的记录；
- `intent`、`diverged` 永不自动删除；
- 删除前必须确认没有依赖该记录的对账链；
- 压缩本身不记录进账本。

## 6. undo/redo 崩溃窗口处理

### 6.1 当前缺口

`undo_edit` 与 `rollback_edit_group` 的当前顺序是：CAS 移动 ref → 写 redo 栈。两者之间退出时，ref 已回退但 redo 栈没有新条目。

`redo_edit` 的顺序是先 CAS 恢复 ref，再弹出 redo 栈；同一类窗口也会导致 redo 已使用但栈未缩短。

### 6.2 目标处理

把这些操作纳入账本后，redo 栈的最终内容随 `intent` 一起写入：

- undo 的账本记录保存“回退后 redo 栈应包含哪些 id”；
- redo 的账本记录保存“恢复后 redo 栈应包含哪些 id”；
- 编辑组回退保存整组回退后的 redo 栈快照；
- 普通新提交保存清空后的 redo 栈快照。

正常路径仍按顺序执行，但启动对账可以重建任何中断步骤。这样不需要把 Git ref 与 SQLite 放进同一个物理事务，也不需要改变当前 ref 的权威地位。

`RedoStackStore` 增加按 actor 写入完整快照的能力，供正常路径和对账路径共用；现有 push/pop 语义保留给调用层表达业务意图。

## 7. 文档与 API 语义澄清

在 `docs/infra/checkpoint/checkpoint-file.md` 增加回退机制对照表，明确：

| 机制 | 作用对象 | 是否移动 ref | 是否写工作区 | 是否可 redo | 是否新增历史 |
| --- | --- | --- | --- | --- | --- |
| `undo_edit` | 单个 Actor 的 edit ref | 是 | 否 | 是 | 否，指针回退 |
| `apply_undo_last_input` | 工作区文件 | 否 | 是 | 否 | 否，不创建 checkpoint |
| `revert_feature_on_main` | main 内容 | 是，追加提交 | 否 | 否 | 是，revert commit |
| `resolve_conflicts` | feature 冲突内容 | 是，追加提交 | 可选 | 否 | 是，解决提交 |
| 操作账本 | 操作事实与附带状态 | 否 | 否 | 否 | 是，仅审计记录 |

同时在 `wf-api` 的 undo 文档注释中保持并强化现有边界：文件级 undo 不改执行状态、不改 ref、不动 redo 栈；调用方若需要审计，应显式创建工作区 checkpoint。

## 8. 实施顺序

1. **统一冲突策略**  
   提取合并计划计算，接入策略化合并入口，让所有配置路径使用同一决策。

2. **收敛标记解析**  
   删除 provenance 中的重复状态机，统一区间解析并保留两套既有测试语义。

3. **扩展账本存储**  
   在现有 SQLite 迁移中加入操作表与读写接口，不改变现有数据表语义。

4. **接入 ref 更新协议**  
   所有管理器发起的 ref CAS 走“写 intent → CAS → 写附带状态 → 标记 applied”流程。

5. **接入启动对账与 GC 压缩**  
   在 bootstrap 中对账，在 GC 中压缩已完成记录。

6. **补充文档**  
   更新 `docs/infra/checkpoint/checkpoint-file.md` 与 API 层说明。

## 9. 验证矩阵

### 冲突策略

- 同一路径并发修改分别以 `Marker`、`Fail`、`Approval` 走完全量审批。
- `ApprovalPolicy::Auto + ConflictBehavior::Fail` 不创建冲突提交，不移动 feature ref。
- `ApprovalPolicy::Auto + ConflictBehavior::Approval` 保持 review pending。
- 文件级部分审批保留未选文件在 review 中。
- feature 合入 main 冲突时 main ref 不变。
- 多特性 `atomic` 预检冲突时不产生任何 main 提交。
- 统一解析器覆盖 diff3、旧式两段、未闭合、二进制与多区间。

### 账本与对账

- 在 intent 写入后、CAS 前模拟退出：记录收敛为 `abandoned`，附带状态不变。
- 在 CAS 后、附带状态前模拟退出：对账重放附带状态并标记 `applied`。
- 在附带状态后、完成标记前模拟退出：对账只补齐完成标记，不重复追加。
- 同一 ref 并发两次操作后崩溃：按序号还原操作链。
- 当前 ref 与账本链无法连接：标记 `diverged`，不修改 ref，事件可见。
- undo、redo、编辑组回退后重启，redo 栈与账本最终状态一致。
- 重复启动对账不产生额外状态变化。
- GC 只清理已完成的过期记录，保留未决与分歧记录。

### 回归

- actor 隔离、审批门禁、冲突阻塞、`list_conflicts`、feature revert 行为不变。
- 现有文件生命周期、恢复演练、链接补偿、explicit undo 测试全部保持通过。
- 编译与测试沿用现有命令，不为本阶段新增独立工具链。

## 10. 验收标准

- 所有产生合并提交的入口对同一 `ConflictBehavior` 给出一致结果。
- 冲突解析只有一份实现，UI 视图与存储侧共用同一区间数据。
- 所有管理器发起的 ref 指针更新均有账本记录。
- 上述三个崩溃点均可由启动对账收敛，redo 不丢失、不重复。
- 账本不参与当前 head 判定，Git 仍是文件内容与 ref 状态的唯一事实来源。
- 不提供全局 operation undo；账本只支撑审计与受控补偿。
