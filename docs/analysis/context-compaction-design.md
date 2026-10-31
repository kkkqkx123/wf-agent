# 预算驱动深压实上下文管理设计

本文把附件中的 Budget-Driven Deep-Compaction 方案落到 wf-agent 现有压缩链上。
现状是一条"超预算 → 单次摘要 → 切视图"的链路；目标是把触发时机、压实幅度、
块生命周期和压缩出口都改成预算驱动，并在压实到尽头时进入交接模式。

结论来自代码实测，引用格式为 `文件:行`，路径相对仓库根目录；首次出现写全路径，
同一文件后文用省略形式。
设计遵循仓库既有约定：不改动无关模块，不保留历史兼容层，
`Message` 与事件结构只保留一份权威定义。

## 0. 结论速览

- 压缩的触发点从"估算超过 85% 窗口"改为三级判定：`T_hard` 激进压实、`T_compact` 深压实、压实后交接判定。阈值全部由模型窗口按比例推导，`CONTEXT_BUDGET_PERCENT` 折算出的预算值保留为请求级安全网。
- `MessageView::Compressed` 升级为分段视图，hot 区由若干"摘要段 + 原始区间"组成；`ConversationState.messages` 继续作为 append-only 权威历史，压缩只写摘要消息、只切视图。
- 新增 `CompactionState` 随会话与检查点一起持久化，承载块状态机、文件读取登记表、交接产物与 hot 估算基线。
- 压实复用现有摘要子 workflow `@standard/fold-summary` 与压缩服务管线，按块逐个调用，一次压缩产出一个完成事件，沿用现有版本锚点与写回幂等。
- 交接作为压缩的出口：压实后仍高位、无可压对象或全部块已到尽头时生成交接文档与索引，清空 hot 区只留文档、索引与最近两轮，原文全部落在本地 bare git 对象库。
- 工具输出在入 hot 区之前蒸馏并归档，文件读取按内容哈希去重，两者都由 agent 层完成，工具执行器保持无状态。
- 恢复只走归档与索引查询，hot 区因此可以真正压到很小。

## 1. 现状实测

### 1.1 会话与视图

- `ConversationState` 持有 `messages`、并行序号 `seqs`、`next_seq`、token 累计、`tracker`、`ledger`、`active_view` 与 `view_stable_estimate`，定义见 `crates/engine/wf-execution-shared/src/conversation_session.rs:10`。
- 历史只追加：`add_message` 追加消息、分配序号、增量更新估算，见同文件 `:113`。
- 压缩通过 `compress_with_tail` 把摘要批次追加进历史，然后把视图切成 `Compressed { summary, tail_begin }`，并重算视图估算，见 `:143`；`restore_full_view` 只切回 `Full`，见 `:169`。
- 请求装配读取投影视图：`view_messages` 见 `:262`，agent 侧唯一调用点在 `crates/engine/wf-agent/src/agent_request.rs:44`。
- 视图枚举定义在 `crates/foundation/wf-types/src/message/message_view.rs:13`，现有五个变体：`Full`、`Compressed`、`Tail`、`NoSystem`、`Range`。
- 视图估算维护在 `view_stable_estimate`，由 `estimated_view_tokens` 暴露，见 `conversation_session.rs:294`。

### 1.2 预算与触发

- 上下文预算来自模型画像窗口：`CONTEXT_BUDGET_PERCENT = 85` 与读取入口见 `crates/engine/wf-execution-shared/src/token_tracker.rs:105`、`:124`。
- 预算注入点在 agent 实体生命周期：`crates/engine/wf-agent/src/coordinator/lifecycle/entity.rs:215` 调用 `context_budget_from_profile` 后写入 `set_context_limit`（`conversation_session.rs:338`）。
- 决策点在每轮用量结算之后：`record_usage_and_compression`（`crates/engine/wf-agent/src/coordinator/iteration/compression.rs:237`）计算 `stable + dynamic`、按校准偏差放大，再用 `over_budget` 判定，见 `:301`、`:303`。
- 判定通过后构造 `ContextCompressionRequest`（`crates/engine/wf-execution-shared/src/token_events.rs:217`）、写压缩前检查点、同步派发钩子、发布审计事件，并把会话标记为已发信号，见 `compression.rs:309`、`:333`、`:357`。
- 下一轮开始前等待压缩落地：`settle_compression_flight`（`crates/engine/wf-agent/src/coordinator/iteration/compression.rs:16`），调用点在 `crates/engine/wf-agent/src/coordinator/iteration.rs:436`。
- 连续"压完仍超预算"有升级停车逻辑，见 `iteration.rs:449`。

### 1.3 压缩服务与摘要链

- 服务入口为钩子处理器 `context_compression`，见 `crates/app/wf-runtime/src/trigger_listener/compression/service.rs:15`；跨尝试策略为 `CompressionPolicy`（`.../compression/policy.rs:11`），含 `tail_keep`、`max_retries`、`run_timeout_ms`、终态回落模式。
- 管线在调用前把快照裁剪到预算的 9/10，见 `.../compression/pipeline.rs:34`、`:41`；去重容量为 1024，见 `:25`。
- 摘要链模板 `@standard/fold-summary`：`start → fold → llm → end`，见 `crates/engine/wf-resource/src/predefined/workflow/child/fold_summary.rs:13`、`:80`；节点超时 150s、链路超时 240s，见 `.../child/summary_stage.rs:13`、`:14`。
- 写回消费者按版本锚点比对后调用 `compress_with_tail`，见 `crates/engine/wf-agent/src/conversation_compression.rs:210`。
- 写回操作只有 `Append`，见 `crates/engine/wf-execution-shared/src/context_store.rs:16`。

### 1.4 已有的可复用资产

- 文件折叠：`FoldPolicy{min_tokens:800,max_tokens:2000,max_items:32,max_retries:2}` 见 `crates/infra/wf-integration/src/code_context.rs:13`；本地折叠 `apply_local_fold` 见 `crates/engine/wf-execution-shared/src/fold_operation.rs:183`。
- 单次 LLM 调用：`generate_text_once` 与 `generate_with_tools_once` 见 `crates/engine/wf-execution-shared/src/single_shot.rs:155`、`:47`。
- 归档载体：`checkpoint-file` 的 bare git 对象库提供 `write_blob`/`read_blob`/`blob_size`，见 `crates/infra/checkpoint/checkpoint-file/src/git_store/objects.rs:53`、`:65`、`:80`；引用命名空间受限于 `refs/wf/`，见 `.../git_store/refs.rs:74`。
- 检查点快照：`AgentStateSnapshot` 已含历史、视图、序号区间、账本与 tracker，见 `crates/foundation/wf-types/src/checkpoint/agent/snapshot.rs:15`、`:21`。
- 工作区目录：`.wf/{meta,tmp}` 由 `ensure_project_wf_volatile_dirs` 建立，见 `crates/infra/wf-config/src/layout.rs:72`。

### 1.5 差距

| 维度 | 现状 | 目标 |
|---|---|---|
| 触发 | 单一 85% 预算阈值 | `T_hard`/`T_compact` 双阈值加压实后交接判定 |
| 压实幅度 | 一次摘要覆盖全量旧历史，尾部保留 2 条 | 按块压实，压到 `H_target` |
| 块生命周期 | 摘要进入历史后不再变更 | `Raw`/`Compressed`/`Sealed` 状态机，含重压上限与整块删除 |
| 压缩出口 | 压不动就升级停车，等人处理 | 交接文档 + 索引，模型继续工作 |
| 工具输出 | 仅在摘要链里折叠一次 | 入 hot 区前即蒸馏并归档 |
| 文件重复读 | 不去重 | 按内容哈希替换为指针 |
| 恢复 | 无恢复入口，只切视图 | 归档 + 索引 + 检索工具 |

## 2. 设计取舍

### 2.1 保留 append-only 权威历史

附件方案用 `Map<Ref, Message>` 与可变块承载全部状态。本实现保留历史数组与序号坐标，块只记录区间与摘要消息 id。
理由有三条：检查点与分支恢复已经以历史数组为权威（`snapshot.rs:21`、`conversation_session.rs:225`）；
审计与消息落库直接消费历史（`crates/foundation/wf-types/src/storage/message.rs:6`）；
摘要消息本身就是渲染单元，放进历史后无需第二套渲染数据源。

块状态机不改变历史，只改变"哪些摘要消息与哪些原始区间进入 hot 区"。
这让"重压"落成"追加一条新摘要消息 + 旧摘要标记为非活跃"，与现有版本锚点天然一致。

### 2.2 一次压缩 = 一个事件 + 一批块结果

附件把压实写成逐块 `await`。本实现把块计划放进同一次压缩请求，由服务端串行执行摘要链，
聚合成一个完成事件。好处是复用现有 `settle_compression_flight` 背压、版本锚点与去重表，
压缩进行中的会话行为保持不变。

### 2.3 归档载体

归档复用 `checkpoint-file` 的 bare git 对象库。裸 `write_blob` 落的是无引用对象，
会被既有 GC 当作不可达内容回收（`crates/infra/checkpoint/checkpoint-file/src/file/merge.rs:196`
按 `refs/wf/` 可达闭包裁剪），因此归档必须挂在引用上。

写法是每条执行一个归档引用 `refs/wf/context/<execution_id>`，用 `commit_on_ref`
（`crates/infra/checkpoint/checkpoint-file/src/git_store/objects.rs:405`）把本批内容写进树，
路径约定为 `blocks/<block_id>.json`、`tools/<message_id>.json`、`index.json`。
`commit_on_ref` 内部先 `write_blob` 再建树，内容寻址保证同一内容只存一份。

附件里的 `a:repo@commit:path#L10-L40` 面向真实仓库对象库。本实现的归档对象由引擎自己的对象库承载，
天然内容寻址、天然可去重，且不依赖工作区处于某个 git 仓库内。
引用语法定义为 `a:<blob-id>`，payload 自带 `kind`、`path`、`commit`、`line_range` 字段，
恢复时按需回读；引用可达性由归档 ref 保证，与 payload 内容无关。

### 2.4 与 nudge 增速门的取舍

billion-context 的触发是需求侧增速门：模型收到带候选区间的提示，自行决定是否调用压缩工具，
未响应时把门槛减半。附件把这条路径换成预算驱动，理由写在附件的取舍声明里：模型拒绝会导致失控。

本设计采用预算驱动，同时吸收两处可复用的部分：

- 候选区间仍按真实尺寸排序并展示给摘要模型，模型只参与"压哪些内容、留哪些事实"的取舍；
- 分层蒸馏保留两层（首次压缩与重压），不做 T3 查表索引。理由是仓库会话的块数量远低于
  长驻助手会话，两层足以覆盖，第三层的检索成本高于收益。

不采纳的部分：增速量 50K 固定地板、tier 计数触发、`decompress` 内联恢复。
前两者属于提示节拍，与本设计的阈值判定职责重叠；后者与"恢复外置到归档查询"的出口冲突。
需要提前恢复时改用 `read_archive` 回读，并把摘要段替换成原始区间片段。

### 2.5 范围边界

首版只作用于 agent 会话上下文。workflow 的 LLM 节点上下文继续走现有单摘要路径
（`crates/engine/wf-workflow/src/handler/llm/token_budget.rs:144`），
两套信号共享事件定义，处理器各自注册。

## 3. 数据结构

### 3.1 视图升级

`MessageView::Compressed` 替换为分段视图：

| 变体 | 语义 |
|---|---|
| `Compacted { segments: Vec<ViewSegment> }` | hot 区 = 若干片段按序投影 |
| `Tail { last_n }` | 保持现状 |
| `NoSystem` | 保持现状 |
| `Range { start_seq, end_seq }` | 保持现状，供分支与预览使用 |
| `Full` | 保持现状 |

`ViewSegment` 两个分支：`Summary { message_id }` 指向历史中已有的摘要消息；
`Range { start_seq, end_seq }` 指向历史中的原始区间。投影按片段顺序拼接，摘要消息不重复计入。

旧 `Compressed` 的等价表达是"一个 `Summary` 片段 + 一个覆盖尾部的 `Range` 片段"。
除定义与文档注释外，该变体的引用共 7 处，其中生产路径三处：投影实现
（`crates/foundation/wf-types/src/message/message_view.rs:48`）、视图估算
（`crates/engine/wf-execution-shared/src/conversation_session.rs:50`）与写回切视图（同文件 `:153`），
另四处为测试构造（`message_view.rs:136`、`:152`、`conversation_session.rs:662`、
`crates/infra/checkpoint/checkpoint-base/src/delta/calculator/agent_tests.rs:52`）。
请求装配经由 `view_messages` 间接消费，不需要单独改造。

### 3.2 压缩状态

`ConversationState` 新增 `compaction: CompactionState`，随会话序列化并进入检查点。

| 字段 | 语义 |
|---|---|
| `mode` | `Normal` 或 `Handoff` |
| `blocks` | 块列表，按创建顺序 |
| `file_reads` | 以路径为键的读取登记表 |
| `handoff` | 交接产物，`None` 表示未交接 |
| `hot_tokens` | hot 区 token 估算缓存，渲染后刷新 |
| `growth_baseline` | 上次压实结束时的 hot 大小，用于增长统计 |
| `window_tokens` | 模型窗口，阈值推导的基准 |
| `thresholds` | 由窗口与比例解析出的四个绝对阈值 |
| `dirty` | 恢复后尚未重算 hot 估算，首次投影时置回 `false` |

`CompressionBlock` 字段：

| 字段 | 语义 |
|---|---|
| `block_id` | `b` 加零填充序号，会话内唯一 |
| `state` | `Raw` / `Compressed` / `Sealed` |
| `topic` | 摘要主题，一行 |
| `summary_message_id` | 摘要消息在历史中的 id，`Raw` 时为空 |
| `source_range` | 覆盖的历史序号闭区间 |
| `size_tokens` | 当前在 hot 区占用的 token |
| `source_tokens` | 首次压缩前的原始 token |
| `direct_seqs` | 直接覆盖的消息序号 |
| `effective_seqs` | 传递闭包，含被合并块的原消息 |
| `archive_refs` | 归档指针列表 |
| `repress_count` | 重压次数 |
| `generation` | 0 表示首次压缩产物，每重压加一 |
| `active` | 是否进入 hot 区 |
| `created_at` / `compressed_at` | 时间戳 |

`FileReadEntry` 字段：`path`、`content_hash`（blake3）、`commit`、`last_read_at`、`message_seq`、`archive_ref`。
哈希计算放在 `wf-execution-shared`（该 crate 已依赖 `blake3`，见 `crates/engine/wf-execution-shared/Cargo.toml:30`），
agent 侧只调用封装函数，不新增依赖。

`HandoffArtifact` 字段：`handoff_message_id`（历史中的交接文档消息 id）、`index`（`Vec<IndexEntry>`）、`reason`、`created_at`、`validation`。

`IndexEntry` 字段：`block_id`、`topic`、`time_range`、`archive_ref`、`keywords`、`size_tokens`。

`Raw` 块不写摘要消息，`size_tokens` 取区间估算；`Compressed`/`Sealed` 块取摘要消息估算。
摘要消息统一为 system 角色，内容带块标识与归档指针，与现状 `Message::system_text` 用法一致
（`crates/infra/llm-suite/crates/llm-suite-types/src/message.rs:303`）。

### 3.3 阈值结构

`CompactionThresholds{h_target, t_compact, t_handoff, t_hard}` 由 `window_tokens` 与四个比例推导，
解析逻辑放在 `wf-execution-shared`，与会话一起持久化，避免恢复后比例漂移。

默认比例对齐附件：

| 项 | 比例 | 说明 |
|---|---|---|
| `H_target` | 0.30W | 深压实目标常驻 |
| `T_handoff` | 0.55W | 交接触发线，判定发生在压实之后 |
| `T_compact` | 0.70W | 压缩触发线 |
| `T_hard` | 0.85W | 硬上限安全阀 |

`R_repress = 5000`、`R_min_range = 2000`、`K_repress = 2` 为绝对 token 值。

## 4. 阈值与状态机

### 4.1 判定顺序

每轮结算后依次判定，任一步命中即停止：

1. `hot >= T_hard`：执行最激进压实（跳过 `R_min_range` 下限，允许丢弃小尺寸的已压缩块），若仍超阈则进入交接，理由 `HARD_LIMIT`。
2. `hot >= T_compact` 且处于 `Normal`：深压实到 `H_target`，随后重新判定交接。
3. 处于 `Normal` 且 `should_handoff`：进入交接。
4. 其余情况保持不动。

`should_handoff` 三个条件取或：

- 压实后 `hot > min(T_handoff, 1.5 × H_target)`，理由 `POST_COMPACT_STILL_HIGH`；
- 既无 `Raw` 块达到 `R_min_range`、也无满足重压条件的块，理由 `NO_COMPRESSIBLE_TARGET`；
- 全部活跃块均为 `Compressed`/`Sealed` 且都不超过 `R_repress`，同时 `hot > 1.5 × H_target`，理由 `ALL_BLOCKS_SMALL`。

`Handoff` 模式只保留硬上限判定，命中即丢弃最旧活跃块。

### 4.2 块状态迁移

- `Raw → Compressed`：首次压缩，要求区间估算不少于 `R_min_range`。
- `Compressed → Compressed`：重压，要求 `size_tokens > R_repress` 且 `repress_count < K_repress`；重压输入是块摘要，原始内容留在归档，供恢复与校验按需回读。
- 重压到达 `K_repress` 时迁移到 `Sealed`。
- `Compressed`/`Sealed → 删除`：仅置 `active = false`，引用指向归档，不再生成摘要。
- `Raw` 不删除：未压缩内容属于待压对象，常规压实只会把它压成摘要，硬上限路径与交接才会清场。

禁止事项：`Sealed` 不再重压；`size_tokens <= R_repress` 的 `Compressed` 不重压；单次重压的输入块必须同 `generation`。

### 4.3 与现有预算的关系

`context_limit` 继续作为请求级安全网，用于保留 `over_budget` 判定
（`context_store.rs:34`）与 provider 拒绝后的强制压缩路径（`compression.rs:139`）。
它的取值是窗口乘预算百分比，默认 `85`，可由画像元数据按模型覆盖
（`token_tracker.rs:109`、`:130`）。

压缩决策改用 `thresholds`。两者同源于模型窗口：默认百分比下 `context_limit` 与 `T_hard`
数值相同，画像覆盖百分比时二者分离，`T_hard` 仍按固定比例推导。这样 provider 拒绝后的兜底
逻辑不需要改动，深压实的触发也不受画像覆盖影响。

## 5. 每轮流水线

agent 侧的接入点收敛在三处：工具结果落历史前、每轮用量结算后、下一轮请求装配前。

### 5.1 工具结果入历史前

新增 `CompactionIngest`，在工具消息进入历史前执行：
落历史集中在 `crates/engine/wf-agent/src/coordinator/iteration.rs:624`，
入历史前处理放在该循环之前，逐条处理 `tool_messages`。处理含两步：

1. 按工具类型蒸馏：测试/构建类保留结论、错误串与 `文件:行号`；搜索类保留命中数与首若干条；
   diff 类保留变更文件与增删行数；命令类保留退出码、耗时与首尾若干行。
   原始文本先写入归档，蒸馏文本替换消息正文，消息 `metadata` 记录归档指针。
2. 文件读取去重：`read_file` 的结果按 `path` 查询 `file_reads`，内容哈希相同则把正文替换为
   `[file <path> @ <commit> already read, see m<seq>]`，哈希不同则登记新条目。

蒸馏规则与附件分派一致，正则按工具 id 匹配。工具 id 定义见
`crates/engine/wf-tools/src/predefined/filesystem/grep_search.rs:8` 等文件，`read_file` 见
`crates/engine/wf-tools/src/predefined/filesystem/read_file.rs:8`。
本轮已有 `apply_local_fold`（`fold_operation.rs:183`）在摘要链内做同类折叠，二者分工：
入历史前蒸馏作用于每条新工具结果，摘要链内折叠作为兜底保留。

### 5.2 块同步

`record_usage_and_compression` 在预算计算前先执行块同步：

- 为每条新消息分配消息引用 `m<seq>`，追加到区间尾部的 `Raw` 块；
- 相邻同主题的 `Raw` 块合并；
- 保护区内消息始终可见：首条 user、最后一条 user、当前轮的工具调用与其结果；
- 刷新 `hot_tokens` 与 `view_stable_estimate` 的一致性。

保护区规则落在渲染层，用视图片段表达，不复制消息，也不修改历史。

### 5.3 压实执行

命中 `T_compact` 时构造块计划：按 `size_tokens` 从大到小挑选候选 `Raw` 块，
在 `hot_tokens <= H_target` 前持续追加；候选耗尽后按同样顺序挑选可重压块；
仍高于目标则按升序丢弃小尺寸的已压缩块。`Raw` 块不在常规压实中丢弃，只能被压缩或由交接清场。

计划作为压缩请求的一部分发出，携带每项的块 id、序号区间、操作类型与目标摘要提示。
请求仍走 `compress_with_tail` 的写回通道，但写回内容从"一批摘要消息"扩展为
"若干摘要消息 + 块更新指令"。写回消费者据此追加摘要消息、更新块状态、刷新视图片段。

### 5.4 现有升级路径的归宿

`iteration.rs:449` 的"连续压完仍超预算就停车"是现有兜底。当前 `still_over_budget_streak`
由写回消费者对每次压缩完成都计一次（`conversation_compression.rs:107`），
达到阈值后下一轮停车（`iteration.rs:449`）。

交接上线后，常规模式下的"压完仍高"直接进入交接，不再回发压缩信号，
因此这条连击只会来自强制压缩与交接模式下的激进压实。停车阈值保留为最后一道保护，
对应的产品语义是：交接也无法让请求落进窗口时才停下来等人。

## 6. 压缩服务改造

### 6.1 请求与事件

`ContextCompressionRequest` 增加两类字段：`plan: Vec<CompactionPlanItem>` 与 `reason`。
`plan` 为空表示传统单摘要模式，workflow 一侧继续使用该形态。

完成事件 `ContextCompressionCompletedMeta` 增加 `blocks: Vec<BlockOutcome>` 与 `reason`。
`BlockOutcome` 携带块 id、摘要消息、新状态、重压次数与归档指针。
沿用 `array_version` 锚点与 `run_id` 关联（`token_events.rs:694`）。

### 6.2 管线

`CompressionPipeline` 的执行体按计划逐项调用摘要链：

- 单项输入为区间切片、块 id、压缩深度与提示变体；
- 单项失败按 `max_retries` 重试，重试仍失败则整批失败，终态处理沿用
  `CompressionFallbackMode`（`crates/foundation/wf-types/src/workflow/definition.rs:65`）；
- 整批预算取 `run_timeout_ms`，单项超时取节点 150s 上限；
- 全部成功或达到重试上限后聚合发布一个完成事件。

`COMPRESSION_HANDLED_CAPACITY` 去重表语义不变，键仍为执行加目标加版本。

### 6.3 提示结构

摘要提示从"压缩整段历史"改为"压缩给定区间"。提示包含：

- 区间内消息引用清单与每块 token 规模；
- 判据：当前任务步骤是否还需要该内容；
- KEEP 类事实清单：路径与行号、签名、确切错误串、含理由的决策、确切数值、目标演进；
- 输出约定：主题一行、摘要主体、`doNotForget` 列表、关键词。

重压提示的输入是块摘要，要求在不丢 KEEP 类事实的前提下进一步收缩。

首版把提示变体做成新的资源模板键，与现有 `@standard/llm-summary` 并列
（`crates/engine/wf-resource/src/predefined/workflow/child/summary_stage.rs:9`）。

## 7. 交接

### 7.1 产物

交接由一次独立 LLM 调用生成，复用子 workflow 形态：新增 `@standard/handoff-doc` 模板，
输入为全部活跃块的摘要、主题与源引用清单，输出结构化交接文档。

文档字段：`goal`、`constraints`、`verified_facts`（带 `source_refs`）、
`decisions`（带 `rationale` 与 `source_refs`）、`artifacts`（`path`、`commit`、`description`）、
`open_questions`、`do_not_forget`。

### 7.2 校验与重试

生成后在 agent 侧做自动校验：统计被引用块覆盖的 KEEP 类事实，与文档内容比对，
缺失项进入 `missing_keep_classes`。缺失非空则带缺失清单重试一次；
重试仍缺失则把缺失项直接追加到 `do_not_forget`，并在 `validation` 中留痕。

校验只需要摘要文本与源引用，纯本地计算，不额外消耗模型调用。

### 7.3 进入交接

进入交接做四件事：

1. 把交接文档作为 system 消息追加进历史，索引序列化为 JSON 后写入归档树
   （`refs/wf/context/<execution_id>` 下的 `index.json`），供工具按需读取；
2. 全部活跃块置为非活跃；
3. 视图切为"交接文档 + 索引摘要 + 最近两轮原始区间"；
4. 记录 `mode = Handoff`、`reason`、`created_at` 与校验结果。

`Handoff` 模式下渲染顺序固定为交接文档、索引、最近两轮，并在文档头部注入一行
"先读交接文档与索引再继续"的硬约束说明。压缩判定跳过 `T_compact`，只保留 `T_hard`。

### 7.4 交接后的恢复通道

模型通过两个新工具访问 cold 区：

- `search_history`：按关键词或主题检索索引条目，返回块 id、主题、时间范围与归档指针；
- `read_archive`：按归档指针回读原始内容，支持按行号区间截取。

两者都是无状态工具，通过 `ToolExecutionContext.checkpoint_session` 拿到执行身份
（`crates/engine/wf-tools/src/executor/trait_def.rs:33`），再经归档读写封装访问对象库。

当前 `CheckpointSession` 只暴露 `actor()` 与 `entity_id()`（见
`crates/infra/checkpoint/checkpoint-file/src/session.rs:114`、`:119`），
`FileCheckpointManager::git_ref` 是 crate 内可见
（`crates/infra/checkpoint/checkpoint-file/src/manager_store.rs:53`）。
因此需要在 `checkpoint-file` 内新增归档访问器，以 `entity_id` 拼装
`refs/wf/context/<id>` 引用名，提交与读取都经它完成，对外由 `wf-checkpoint` 门面再导出。
工具层只依赖该访问器，不接触裸 git 对象库。

索引本身也落在对象库，避免工具侧持有会话句柄。

## 8. 渲染与请求装配

hot 区渲染由视图投影承担：`view_messages` 按 `Compacted` 片段拼接消息。
渲染规则：

- `Normal`：按块创建顺序输出活跃摘要消息，再输出最近两轮原始区间；
- `Handoff`：交接文档、索引摘要、最近两轮，其余内容不出现；
- 压缩调用本身产生的内部消息不进入投影。

请求装配仍在 `agent_request.rs:44` 一处读取视图，因此工具可见性、系统提示注入、
历史归一化的顺序保持不变。

`hot_tokens` 每次投影后刷新，与 `view_stable_estimate` 共用同一套增量估算；
两者差异仅在动态开销：`hot_tokens` 只含稳定内容，决策时由
`dynamic_request_overhead`（`context_store.rs:132`）补齐，与现状一致。

## 9. 归档与恢复

### 9.1 归档对象

归档写入 `checkpoint-file` 的对象库，每条执行一个引用 `refs/wf/context/<execution_id>`。
三类对象：

| 对象 | 树内路径 | 引用 |
|---|---|---|
| 区间原文 | `blocks/<block_id>.json` | `a:<blob-id>` |
| 工具输出原文 | `tools/<message_id>.json` | `a:<blob-id>` |
| 交接索引 | `index.json` | `a:<blob-id>`，读出走 ref 头指向的提交树 |

写入走 `commit_on_ref`（`objects.rs:405`），一次调用写一条或多条路径，返回提交 id。
回读先用 `read_ref`（`crates/infra/checkpoint/checkpoint-file/src/git_store/refs.rs:146`）
取提交、`tree_to_files`（`objects.rs:173`）展开树得到 blob，再 `read_blob`（`objects.rs:65`）；
已知 blob id 时可直接 `read_blob`，不必走树。
内容寻址保证同一内容只存一份，工具输出重复时天然去重。

归档提交与文件检查点共用同一条 bare 对象库，但引用命名空间独立。
GC 只按 `refs/wf/` 前缀枚举可达集合（`objects.rs:490`），归档 ref 天然被覆盖，
不需要额外保护逻辑。

### 9.2 生命周期

归档对象与会话同寿命，不做主动清理。压缩块删除只改 `active`，归档指针保留。
工作区清理策略沿用检查点库现有规则，不为归档单独引入清理器。

### 9.3 分支与恢复

`branch_state_through_seq`（`conversation_session.rs:225`）克隆整个状态，块随之过去。
需要在截断后重建块：丢弃 `source_range` 越过目标序号的块，把索引指向的历史消息
不存在的块降级为 `Raw`，清理 `file_reads` 中指向被截断消息的条目。
检查点快照新增 `conversation_compaction` 字段，恢复时按原样回填，
缺失时按空状态初始化并标记 `dirty`，下一次读取重算 hot 估算。

## 10. 配置

`configs/infrastructure/limits.toml` 新增 `[compaction]` 节，默认值取附件建议：

| 键 | 默认 | 含义 |
|---|---|---|
| `h_target_ratio` | 0.30 | 深压实目标 |
| `t_handoff_ratio` | 0.55 | 交接判定线 |
| `t_compact_ratio` | 0.70 | 压缩触发线 |
| `t_hard_ratio` | 0.85 | 硬上限 |
| `r_repress` | 5000 | 可重压最小尺寸 |
| `r_min_range` | 2000 | 最小可压区间 |
| `k_repress` | 2 | 单块最大重压次数 |
| `recent_raw_turns` | 2 | hot 区保留的最近轮次 |
| `tool_distill` | true | 工具输出即压开关 |
| `file_dedup` | true | 文件读取去重开关 |

校验在 `crates/infra/wf-config/src/processor/limits.rs` 补齐，与服务端校验同一入口：
比例必须严格递增 `H_target < T_handoff < T_compact < T_hard`，
且 `r_min_range < r_repress`。非法值直接拒绝启动，不做静默回退。

`CompactionLimits` 定义在 `crates/foundation/wf-types/src/config/limits.rs`，与
`CompressionLimits`（`:62`）并列。窗口来源继续取画像 `context_window_size`，
与 `context_budget_from_profile`（`token_tracker.rs:137`）同源。

## 11. 与两套参照方案的差异

| 参照方案 | 本设计 | 理由 |
|---|---|---|
| `Map<Ref, Message>` 可变消息表 | 历史数组 + 序号坐标 + 视图片段 | 检查点、审计、落库已以历史为权威 |
| 块内直接持有原文 | 块只持摘要消息 id，原文进归档 | 一份权威数据，避免两处副本 |
| 逐块独立请求与事件 | 一个请求带块计划，一个完成事件 | 复用背压、版本锚点与去重 |
| `a:repo@commit:path#L10-L40` | 归档 ref 下的 blob 指针，payload 自带定位信息 | 归档由引擎对象库承载，不依赖工作区 git |
| 重压走原块就地变更 | 追加新摘要消息，旧块标记失效 | 版本锚点与写回幂等只支持追加 |
| 交接后禁止压实 | 保留硬上限判定 | provider 仍可能拒绝超大请求 |
| 模型自选压缩时机（nudge 提示） | 预算驱动，模型只参与内容取舍 | 提示被忽略时不会让上下文失控 |
| 三层蒸馏 T1/T2/T3 | 两层：首次压缩与重压 | 块数量规模不需要第三层 |
| `decompress` 内联恢复 | `read_archive` 回读并替换片段 | 恢复走归档，hot 区保持小 |

## 12. 实施阶段

1. 数据结构：视图分段化、`CompactionState`、阈值解析、配置与校验。此阶段不改行为，
   压缩仍走单摘要路径，用等价视图表达，测试覆盖投影与序列化。
2. 归档与工具：对象库读写封装、`search_history`、`read_archive`、索引落盘。
3. 入历史前处理：工具输出蒸馏、文件读取去重，接入工具消息落历史的循环之前（`crates/engine/wf-agent/src/coordinator/iteration.rs:624`）。
4. 预算与状态机：三级判定、块同步、保护区、hot 估算对齐。
5. 深压实：块计划、服务端逐项执行、写回块更新、重压与删除。
6. 交接：交接模板、校验与重试、清场渲染、交接后工具可见性。
7. 持久化：检查点字段、分支重建、恢复重算。

每阶段结束时跑一次全特性编译检查与受影响包单测，不分散多次构建。

### 12.1 触点清单

| 层 | 文件 | 改动 |
|---|---|---|
| foundation | `crates/foundation/wf-types/src/message/message_view.rs` | 新增 `Compacted` 变体与 `ViewSegment`，替换 `Compressed` |
| foundation | `crates/foundation/wf-types/src/checkpoint/agent/snapshot.rs` | 新增 `conversation_compaction` 字段 |
| foundation | `crates/foundation/wf-types/src/config/limits.rs` | 新增 `CompactionLimits`，补齐默认值与校验常量 |
| foundation | `crates/foundation/wf-types/src/message/` | 新增压缩状态与块结构，`Message.metadata` 承载归档指针 |
| execution-shared | `crates/engine/wf-execution-shared/src/conversation_session.rs` | 状态字段、阈值解析、视图投影、分支重建、hot 估算 |
| execution-shared | `crates/engine/wf-execution-shared/src/token_events.rs` | 请求与完成事件增加块计划与块结果 |
| execution-shared | `crates/engine/wf-execution-shared/src/context_store.rs` | 计划构造、三级判定入口 |
| execution-shared | 新增归档封装模块 | 对象库读写、引用拼装、索引序列化 |
| agent | `crates/engine/wf-agent/src/coordinator/iteration/compression.rs` | 判定改为三级，压实后重评交接 |
| agent | `crates/engine/wf-agent/src/coordinator/iteration.rs` | 工具消息蒸馏与去重的接入点 |
| agent | `crates/engine/wf-agent/src/conversation_compression.rs` | 块结果写回、视图刷新 |
| agent | `crates/engine/wf-agent/src/checkpoint/coordinator.rs` | 快照采集与恢复回填 |
| runtime | `crates/app/wf-runtime/src/trigger_listener/compression/pipeline.rs` | 计划逐项执行与聚合 |
| runtime | `crates/app/wf-runtime/src/trigger_listener/compression/policy.rs` | 计划级重试与超时 |
| resource | `crates/engine/wf-resource/src/predefined/workflow/child/` | 新增交接文档模板、摘要提示变体 |
| tools | `crates/engine/wf-tools/src/predefined/` | 新增 `search_history`、`read_archive` |
| config | `configs/infrastructure/limits.toml` | 新增 `[compaction]` 节 |
| checkpoint | `crates/infra/checkpoint/checkpoint-file/src/` | 归档访问器与引用命名空间 |
| checkpoint | `crates/infra/checkpoint/checkpoint-base/src/delta/calculator/agent.rs` | 增量计算纳入新字段 |

## 13. 验证

| 场景 | 断言 |
|---|---|
| 阈值抖动 | 压实后 hot 落在 `T_handoff` 附近，连续 100 轮无 `Compact` 与 `Handoff` 振荡 |
| 重压防套娃 | 3K 的 `Compressed` 块拒绝重压；6K 块重压两次后转 `Sealed` |
| 交接触发 | 全部块不超过 5K 且 hot 超过 1.5 倍目标，进入交接且文档含全部 KEEP 类 |
| 工具蒸馏 | 100K 测试日志压到 2K 以内，错误串与文件行号全保留 |
| 文件去重 | 同内容二次读取 token 下降九成以上，指针指向首次读取 |
| 闭包完整性 | 任意块重压两次后，`effective_seqs` 仍覆盖全部原始消息 |
| 硬上限 | 超 `T_hard` 先激进压实，仍超则交接并给出 `HARD_LIMIT` |
| 恢复 | 交接后 `search_history` 命中全部索引条目，`read_archive` 回读内容一致 |
| 分支 | 截断到早于某块序号的检查点后，块集合与视图自洽，无悬空摘要 |

单测分布在 `wf-types`（视图投影、序列化）、`wf-execution-shared`（阈值、状态机、估算）、
`wf-agent`（入历史前处理、交接校验、分支重建）、`wf-runtime`（管线计划执行）四处。
集成验证用现有压缩示例跑一条长会话，覆盖压实到交接的完整路径。

## 14. 风险

- 摘要质量下行：重压是摘要的摘要，最多两层；`Sealed` 兜住上限，KEEP 类校验兜住事实丢失。
- 成本上升：深压实按块调用，单轮可能连发多次摘要。缓解手段是按大小排序、命中目标即停；
  同一版本的重复信号由去重表拦截（`.../compression/pipeline.rs:271`），不会并发重跑。
- 交接文档偏差：文档生成依赖模型，自动校验只能发现 KEEP 类遗漏，语义漂移靠索引与归档兜底。
- 归档依赖工作区对象库：工作区迁移或对象库损坏会让 cold 区不可读。归档与会话绑定，
  迁移工具需要一并搬运 `refs/wf/context/` 命名空间；该命名空间位于 `refs/wf/` 前缀下，
  已被 GC 可达集合覆盖，无需额外配置。
- 事件体积：块计划与块结果进入事件元数据，长会话下单事件可能较大。首版接受，
  后续可按需把计划体放到归档、事件只留引用。
