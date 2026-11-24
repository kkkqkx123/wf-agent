# 上下文压缩：完整逻辑替换的设计方案

本文回答一个问题：如果要让第三方能够**完整替换**上下文压缩逻辑（而不只是调整阈值、建议规划区间或替换摘要工作流），应当如何改造当前项目。

**设计立场先行**：Weave 与 DSH 只作为「重视插件生态的样例」用于分析本项目的扩展机制该如何改造，**不是功能对标清单**。本方案交付的是一套通用、完备的替换与扩展契约；缓存前缀复用、分页接续、模型主动申请等特定策略，都是某个引擎或插件在这套契约上自行定义的能力，不作为框架需求、不排期、不占设计篇幅——避免特定思路污染整体设计。

分析基线是仓库 `525f4087`（2026-11-23，含对比文档论断）。所有现状声明均经源码核验并附 `file:line`；对比文档中已过时的判断在 §1 先行纠正。遵循 `AGENTS.md`：设计文档以自然语言为主，仅给接口形态签名草图，不贴完整实现；新增动态派发须按 `AGENTS.md:85` 登记到已存在的 `docs/archive/dynamic.md`（该文件已由 network 能力建立，见 `docs/archive/dynamic.md:1-40`，本方案追加一节）；新文件用 `engine.rs` 形式，不建 `mod.rs`（`AGENTS.md:95`）。

## 0. 结论速览

* 当前项目**不缺压缩能力**，缺的是把内核从流程里抽出来的**可替换契约**。现状是「插件提建议 + 同名工作流换摘要链 + hook 接管」三套机制叠加（`docs/analysis/context-compression-plugin-extension.md:20-38`），没有任何 `CompactionEngine` 风格的 trait 或动态注册接缝（全仓核验无此 trait）。
* 核心改造：抽出 `CompactionEngine` trait（三方法 + 只读调用上下文），把现有 `CompressionPipeline` 包装成 `builtin` 引擎；**一个执行实例只绑定一个引擎实例，运行期冻结**，所有触发入口汇聚到它。
* **实例级唯一引擎**是「整体替换、杜绝多套压缩逻辑并行」的根本保证：在每次执行新建的 `coordinator()`（`executor.rs:148`）解析一次引擎、写入 `AgentLoopEntity`，并下线 advisor、同名 workflow 链接管、hook 接管三套并行机制。
* 四层契约必须同时成立才叫「完整替换」：**接口契约**（trait）、**注册契约**（native-only + 名字唯一 + 进程级工厂表）、**装配契约**（实例级唯一绑定 + 计划/信号/接管解耦）、**写回契约**（破坏式 Replace 默认开启，但 ShrinkProof 由宿主复算）。
* Weave/DSH 的功能差异不作为设计项：溢出反应式压缩本项目**已有**（`compression.rs:141`），历史检索工具**已有**（`context.rs:13-50`）；缓存前缀复用、分页接续不纳入本方案；模型主动申请不单开扩展点。

## 1. 现状核验与对比文档纠偏

### 1.1 对比文档中已过时或不准确的判断

| 对比文档论断 | 核验结论 | 证据 |
|---|---|---|
| 「未发现溢出反应式压缩」 | **不成立**：provider 返回 `context_length_exceeded` 时强制压缩的链路已存在 | `crates/engine/wf-agent/src/coordinator/iteration/compression.rs:141` `publish_forced_compression`；调用点 `iteration/streaming.rs:164,202`、`iteration/llm_call.rs:113`、`crates/engine/wf-workflow/src/handler/llm/stream.rs:57,135,168` |
| 「归档有 IndexEntry，未见模型可调用的检索工具」 | **不成立**：`search_history` 与 `read_archive` 已注册到 `ToolRegistry` | `crates/engine/wf-tools/src/predefined/context.rs:13-50`（`ToolDefinition` 在 `:13,29`，注册在 `:49-50`） |
| 「缺手动/模型申请/闲置三入口」 | **部分成立**：手动、闲置确缺；模型申请不作为本方案改进项（见 §8.2）；溢出已存在 | 全仓 `SlashCommand` 无 compact（`wf-mini/src/input/commands.rs:200`、`web-app/src/lib/config/commands.ts:15`）；`context_compact\|force_compact` 零匹配 |
| 「metrics 在 foundation 层」 | 路径写错，实际在 infra 层 | `crates/infra/wf-metrics/src/collectors/compaction.rs:11`、`record_handoff_entered` 在 `:39` |
| 「LlmAuth/LlmImages 均 native-only」 | 部分不成立：LlmImages 是 native-only（`native/plugin.rs:1232-1264`），LlmAuth 已有 Lua/Wasm 通道（`wasm/plugin/codec.rs:87`、`lua/plugin.rs:421`） | 同上 |
| 「advisor 只能提交 items/drops」 | 不完整：advise 输出还含 directive，可经 strategy 影响工作流 | trait `wf-plugin-sdk/src/contributions.rs:712-730`；校验 `compaction/plan.rs:270-337`；`compaction/strategy.rs:45` |
| 「缓存前缀复用、分页接续缺失」 | 事实成立，但**不作为设计输入**：二者是特定算法/场景，交由引擎或插件在本契约上自行定义，不进框架、不排期 | 全仓 `cachePreservePrefix\|preserve_prefix` 零相关命中；`paginat\|resume summary` 无压缩相关命中 |

### 1.2 已核验的压缩链路事实

* **预算与决策**：四档阈值定义在 `compaction/budget.rs:6-9`，解析函数 `resolve_compaction_thresholds` 在 `:95-107`。`over_budget`（`context_store.rs:36`）只是 `estimated > token_limit` 的简单闸门，用于 `should_emit_hot`（`context_store.rs:523`），**不参与四档决策**。真正的决策函数是 `compaction/decision.rs:29` `decide_compaction`，返回 `CompactionDecision { Hold, Compact{aggressive}, Handoff, DropOldestActive }`（`decision.rs:18`）。`t_handoff` 不进入 `decide_compaction`，只在 handoff 判定 `conversation_session/compression.rs:366-374` 出现。
* **计划与建议**：`compaction/plan.rs:453` `build_plan` 选区间、定块、算预算。插件建议经 `try_advisor_plan_with`（`:195`，`pub(crate)`）逐条校验并夹取 items（`:270-314`）、drops（`:315-330`）、directive（`:331-337`），空建议回退内建（`:338-340`）。
* **信号与接管**：`compression_request`（`context_store.rs:44`）→ `dispatch_compression_signal`（`:205`）→ `fire`（`:244`）→ 固定处理器名 `context_compression`（`token_events/keys.rs:149`）→ `notify`（`:281`）。结果三态 `CompressionSignalOutcome { Taken, Vetoed, Untaken }`（`context_store.rs:176`）。`CONTEXT_COMPRESSION_SIGNAL` 被 `hook_allows_trigger` 拒绝作为 trigger（`wf-types/src/hook.rs:234-242`）。
* **执行与写回**：`CompressionPipeline` 是 `pub(crate)`（`wf-runtime/src/trigger_listener/compression/pipeline.rs:77`，`run_routed:138`、`spawn:210` 同可见性）。consumer 在 `lifecycle/run.rs:46-76` 定义、`:149-155` 硬挂载。`WritebackOp` 只有 `Append` 一个变体（`context_store.rs:16-30`），`apply_versioned_writeback` 的 match 只有一臂（`conversation_compression.rs:243-265`）；压缩只追加摘要并切换 `MessageView::Compacted { segments }`（`message_view.rs:20`、`ViewSegment:35-40`），随后打压缩后 checkpoint（`conversation_compression.rs:127-142`）。
* **三套扩展机制均为进程级**：advisor 表 `COMPACTION_ADVISORS`（`compaction/strategy.rs:61-63`，插件同步 `plugin_bridge/advisors.rs:11`）；同名 workflow 链 `strategy_workflow_ids`（`strategy.rs:45`，bootstrap 解析一次 `bootstrap/trigger.rs:103`）；hook 表 `HookHandlerRegistry`（`hooks/registry.rs:33`，compression handler 注册 `bootstrap/trigger.rs:257`）。另有 `PROCESS_LIMITS`（`strategy.rs:96`）亦为进程级。
* **执行实例绑定点已存在**：consumer 签名 `(bus, agent_loop_id, conversation, checkpoint, limits)`（`conversation_compression.rs:51-57`），按 `event.execution_id == agent_loop_id` 过滤（`:61-62`）；调用时传 `entity.id()`（`run.rs:52,70`）。实体 `AgentLoopEntity.id: Id`（`entity.rs:46-47`），每轮执行由 `executor.rs:148` 新建 coordinator、由 `lifecycle/entity.rs:160-161` 建实体。
* **checkpoint 的引擎状态缺口**：`CompressionCheckpoint`（`conversation_compression.rs:42-45`）派生于 `run.rs:59-62`；checkpoint 按 execution id 存取（`checkpoint/coordinator.rs:444,535`、`checkpoint-file/file/actor.rs:72`），恢复 `restore_entity`（`coordinator.rs:474-487`）。但快照携带的 `loop_config`（`loop_config.rs:65`）**无引擎字段**，引擎选择来自进程全局，不随 checkpoint 恢复。
* **配置面**：`CompactionLimits` 共 13 字段（`wf-types/src/config/limits.rs:59-96`），默认比值 0.30/0.55/0.70/0.85（`wf-config/src/processor/limits.rs:41-49`）。bootstrap 解析 `config.rs:161` → 注入 executor `inner.rs:438-443` → 装配 trigger `trigger.rs:54,86,95-103`。

### 1.3 差距的性质

对比文档列出四个「缺」，其中真正属于框架层缺口的是**引擎可替换性**与**手动入口**；溢出入口已有，模型申请不单开扩展点，缓存/分页不纳入设计。内核已有分块、handoff、版本写回、KEEP 校验、归档、蒸馏、去重等完整资产，因此改造重点是**契约、装配与实例隔离**，把具体算法留在引擎内部——包括未来某个引擎自行实现缓存复用或分页接续，框架不预设也不阻碍。

## 2. 目标与非目标

* **目标**：第三方 native 插件实现完整压缩引擎并替换内建实现；**一个执行实例绑定唯一引擎，运行期冻结，杜绝多套压缩逻辑并行**；所有触发入口汇聚到该引擎；破坏式写回默认开启但可审计、可回滚。
* **非目标**：
  - 不与 Weave/DSH 做功能对齐；二者仅作插件生态对照样例。
  - 不把缓存前缀复用、分页接续、模型主动申请压缩等特定策略做成框架需求或扩展点；它们由引擎/插件自行定义。
  - 不开放 Lua/Wasm 压缩通道（沿用 advisor 原生专属先例）。
  - **不做向后兼容**（`AGENTS.md:5-6`）：被新引擎取代的 advisor 与 hook 接管机制直接下线，不留双轨、不做 deprecate 缓冲。

## 3. L0 契约层：抽出 CompactionEngine trait 与只读调用上下文

### 3.1 接口形态

在 `crates/engine/wf-execution-shared/src/compaction/engine.rs` 定义 trait（新文件，非 `mod.rs`）。宿主只通过 trait 调用三方法，在 Rust 里是编译期约束：

* `compact_if_needed`：对应 `record_compaction_decision`（`compression.rs:328`）的自动/安全步骤调用，返回 `Option<Outcome>`，`None` 表示不触发。
* `compact_now`：手动命令调用（及未来任何通用触发入口），不看触发线；返回 `Outcome`（含状态与原因）。
* `compact_region`：溢出反应式压缩调用，对应 `publish_forced_compression`（`compression.rs:141`），入参为需要收缩的区间。

三方法都接收只读调用上下文 `CompactionCall<'a>`：消息快照（seq + role + tool_call_id，只读）、token 计量器、LLM 摘要路由、归档句柄、预算视图、取消令牌。**写入权不在上下文里**——引擎返回「要写什么」的 outcome，宿主决定「怎么写」，避免引擎成为不可审计的黑盒。

### 3.2 Outcome 三态

* `Noop`：不产生变化。
* `View { segments, summaries, archive_refs }`：追加摘要并切换投影，走现有 append-only 路径。
* `Replace { start_seq, end_seq, summary, archive_ref, proof }`：破坏式替换，**默认开启**（见 §6），但 ShrinkProof 由宿主复算。

### 3.3 动态派发约定

`Arc<dyn CompactionEngine>` 属新增动态派发，须按 `AGENTS.md:85` 在 `docs/archive/dynamic.md` **追加一节「压缩引擎动态派发」**，沿用该文件既有的「必要性说明 + 位置表格 + 未采用对照」格式（`docs/archive/dynamic.md:5-40`）。`dyn` 仅出现在注册表与实例字段边界；对外以工厂返回具体类型为主。

## 4. L1 注册与配置层

* 新增 native-only 贡献类型 `CompactionEngine`（贡献枚举现 31 种，`wf-plugin/src/contributions/types.rs:28-90`）。照抄 `CompactionAdvisor` 全链路：贡献类型 `types.rs:49-52`、原生 trait `wf-plugin-sdk/src/contributions.rs:712-730`、注册器 `manager/registrar.rs:264-279`、管理器表 `manager/registry.rs:45`、native FFI `native/plugin.rs:1266-1279`、加载桥接 `plugin_bridge/advisors.rs:3-28`。差异是 advisor 建议会被引擎校验夹取，而引擎要**接管**决策。
* 工厂 trait 接收插件私有配置 JSON，构造 `Arc<dyn CompactionEngine>`。注册表为**进程级工厂表**（按 name 索引，供实例创建时取用），注册以 name 为 key，**重名直接失败**（沿用 `strategy.rs:77-81` 的占用返回 false 语义）。工厂表是「有哪些引擎可用」，不是「某实例正在用哪个引擎」——后者是实例级（§5）。
* 配置新增 `compaction.engine`（默认 `builtin`）与 `compaction.engine_config`（宿主不解释的私有 JSON）。**未知引擎名在启动期报错**，不静默回退——否则用户以为换上了、实际还在跑内建。
* `strategy` 的语义收紧：仅当 `engine: builtin` 时用于选择摘要工作流（`strategy.rs:45-56`）；其他引擎忽略 strategy 并在启动日志提示，两个旋钮职责不重叠。
* 注册表按 profile 归属（Q1 推荐）：配置按 agent profile 解析出引擎名，工厂表仍进程级按名索引；**实例级绑定在 §5**，二者是不同维度，不冲突。

## 5. L2 装配层：实例级唯一引擎与三套机制收敛

这是满足「某个执行实例整体替换、避免多套压缩逻辑并行」的核心章节。

### 5.1 实例级唯一绑定

* 在每次执行新建的 `coordinator()`（`executor.rs:148`）处，按该实例的 profile 配置解析一次引擎名 → 从工厂表取唯一实例 → 写入 `AgentLoopEntity`（`entity.rs:46` 旁新增实例字段），**运行期冻结不可换**。
* consumer（`conversation_compression.rs:51-57`，调用 `run.rs:46-76`）与所有触发入口统一从该实例字段读取引擎，并沿用既有 `execution id` 过滤（`:61`）。这样每个执行实例从创建到销毁只有一套压缩逻辑在作用。
* checkpoint 快照补 `engine_id`（+ 私有 config）字段：写入侧 `checkpoint/coordinator.rs:444`，恢复侧 `restore_entity`（`:474-487`）按 id 重建引擎实例并回填 `AgentLoopEntity`；native 引擎实例不可序列化，故存 id+config 而非对象。补上当前 `loop_config`（`loop_config.rs:65`）缺引擎字段的缺口，使「实例级引擎」跨恢复仍保持唯一。

### 5.2 下线三套并行机制（Q3、Q4：不保留，不做兼容）

* **advisor 下线**：删除 `CompactionAdvisor` 贡献类型（`contributions.rs:712-730`）、贡献枚举项（`types.rs:49-52`）、进程表 `COMPACTION_ADVISORS`（`strategy.rs:61-63`）、插件注册与桥接（`plugin_bridge/advisors.rs:3-28`）、`plan.rs` 的建议查询与校验夹取（`:173-343`，保留纯 `build_plan` 作为 builtin 内部实现）。其「微调规划」能力由 builtin 引擎自身配置覆盖，插件要彻底换策略就实现完整引擎。
* **同名 workflow 链接管下线**：`strategy_workflow_ids`（`strategy.rs:45`）与 `ExecuteContextCompression` 预留动作（`bootstrap/trigger.rs:257`、`wf-config/src/processor/trigger/template.rs:164-174`）从「压缩接管机制」移除；摘要 workflow 仍是可被 builtin 引擎引用的普通资源，但不再是独立扩展面。
* **hook 接管下线**：`CONTEXT_COMPRESSION_REQUESTED` 的接管 handler、否决/替换语义与 `CompressionSignalOutcome { Taken, Vetoed, Untaken }`（`context_store.rs:176`）移除；信号降级为纯审计事件（保留发布 `token_events/compression.rs:118`）。`hook_allows_trigger` 对压缩信号的屏蔽（`hook.rs:234-242`）保留，防止用户模板误当 trigger。
* `PROCESS_LIMITS`（`strategy.rs:96`）等进程级压缩参数随配置一并实例级化，避免跨实例串扰。

### 5.3 消费与编排解耦

* `coordinator/iteration/compression.rs` 不再自己建计划、发信号、等接管，改为一次 `engine.compact_if_needed(&call)`；阈值判定（`decision.rs:29`）、计划构造（`plan.rs:453`）、背压（`compression.rs:16`）成为 builtin 引擎内部实现。
* `CompressionPipeline`（`pipeline.rs:77`）上移并包装为 `WorkflowCompactionEngine`，成为 builtin 引擎实现；claim/重试/降级（claim_table.rs:16-40、policy.rs:59-72、terminal_report.rs:170-205）不再拥有「唯一接管权」。
* `spawn_conversation_compression_consumer`（`run.rs:149-155` 硬挂）改为宿主统一 consumer：引擎返回 outcome，宿主做版本校验、写回、checkpoint；生命周期绑定执行实例而非某引擎实现。

## 6. L3 写入契约层：破坏式 Replace 默认开启

* **默认行为**：`WritebackOp` 增加 `Replace` 变体（`context_store.rs:16-30`），并作为默认压缩写回路径；append-only 视图投影（`MessageView::Compacted`，`message_view.rs:20`）保留为引擎可选返回 `Outcome::View` 的分支。默认开启意味着引擎返回的 `Replace` 无需额外开关即可生效——因此下面三项安全边界不可协商。
* **安全边界（默认开启的前提）**：
  1. `capabilities()` 声明能力：引擎声明支持 destructive replace；不支持者由宿主拒绝其 `Replace` outcome 并记错。
  2. **ShrinkProof 宿主复算**：新视图 token 必须显著小于旧视图（阈值复用 `r_min_range` 原则，`budget.rs:119`，默认 2000），不信任引擎自报——这是默认开启下防止失控的硬闸门。
  3. 固定提交顺序：**archive → hash verify → persist → publish**；任一步失败回滚到 checkpoint 压缩前快照（机制已存在，`conversation_compression.rs:127-142`、`handoff.rs:253-257`）。
* 完成事件元数据 `ContextCompressionCompletedMeta` 扩充以携带替换视图信息；`apply_versioned_writeback` 的 match（`conversation_compression.rs:258-264`）增加 Replace 分支。

## 7. L4 宿主服务句柄层

给引擎一套稳定、只读、可版本化的句柄——这正是 Weave 最缺、只能靠指纹反射拿宿主私有方法的地方（`docs/archive/dynamic.md:37-40` 已明确「不为统一 id/kind 造 trait」的对照约定）。wf-agent 是同进程 Rust，直接以 trait 提供：

| 句柄 | 能力 | 边界 |
|---|---|---|
| MessageSnapshot | 按 seq 读消息/角色/工具配对、token 估算 | 不可写、不可跳版本 |
| TokenMeter | 逐条计量、窗口解析、预算视图 | 复用 `token_tracker.rs` 与 `budget.rs:95-107` |
| LlmRouter | 按 provider/model 发起摘要请求，带超时取消 | 仅摘要用途，受 `summary_call_budget_tokens` 约束（`wf-config/src/processor/limits.rs:117-152`） |
| ArchiveStore | 写区间归档、读 span、按关键词检索 | 复用已存在的 `search_history`/`read_archive`（`context.rs:13-50`） |
| CheckpointHandle | 打压缩前后快照 | 仅压缩相关时机 |
| ProgressSink | 上报阶段、进度、收缩比 | 不影响控制流 |

## 8. L5 触发入口层：汇聚到实例唯一引擎

### 8.1 保留的入口

* **手动**：内建 `CliCommand` `/compact`（支持 status/resume 子命令），调用实例引擎的 `compact_now`。挂载点：贡献类型 `wf-plugin-sdk/src/contributions.rs:83`、注册 `manager/registrar.rs:436`、CLI 命令表 `wf-mini/src/input/commands.rs:200`、TUI `tui-core/src/command.rs:1178`、Web `web-app/src/lib/config/commands.ts:15`。
* **闲置**：从 checkpoint journal 取最近一次实际活动时间（`checkpoint/coordinator.rs:535`），不引入外部计时器。
* **溢出反应式**：把已有的 `publish_forced_compression`（`compression.rs:141`）接到实例引擎的 `compact_region`，不新写路径。

### 8.2 不为「模型主动申请压缩」单开扩展点（Q7）

用户决策：模型主动申请工具不作为本方案改进项，设计不与 Weave 对齐；若未来要做，应做成**通用事件触发工具**——模型调用一个通用工具，给出事件类型约束与提示词覆盖，而不是为压缩单开专用工具。理由：为很可能无用的单一功能单开扩展点违背设计原则。

核验表明该方向可复用现成能力，无需新造：

* **事件类型约束**：hook 词汇表与过滤已完备——`is_known_hook_point`（`hook.rs:73-77`）、`hook_effect` 分类（`hook.rs:159-166`）、`HookDefinition` 含 hook_type/condition/enabled/priority/payload/handler（`hooks/types.rs:28-49`）、`fire()` 按类型过滤（`fire.rs:140,162`）。通用工具只需把模型给出的事件类型约束映射到这些既有类型。
* **提示词覆盖**：通用静态注入已有 `transform_context.messages`（`wf-workflow/src/handler/llm/messages.rs:27-40,180-190`）；压缩专用 directive（`keys.rs:95`、`pipeline.rs:38,280-281`）是压缩路径内部产物，通用工具应走通用注入而非复制它。
* **回执**：项目**没有**通用 queued/skipped/blocked 多态 envelope，只有 `ToolExecutionResult`（`tool/execution.rs:33-59`）与失败 envelope `tool_failure.rs:1-12`。因此七态回执不纳入设计；通用工具沿用现有结构化结果即可。

结论：**本方案不新增任何模型侧工具扩展点**。若未来引入通用事件触发工具，它属于通用工具/事件体系，压缩只是其可触发事件之一，与压缩引擎契约解耦。

## 9. L6 状态、可观测与安全边界

* **状态**：checkpoint 按 engine name 分区存 `engine_state` blob + `engine_id`（§5.1），引擎自带版本号；恢复时版本不匹配丢弃并回退 Full 视图。这同时保证实例级引擎跨恢复唯一。
* **可观测**：补 `compaction/start`、`compaction/end` 事件，与现有 Requested/Completed/Failed/Discarded（`token_events/compression.rs:118,251,289`）并列；ProgressSink 喂给 TUI 状态行（`tui-components/src/context_sidebar.rs:16`、`tui-core/src/status_line.rs:134`）与 Web 状态卡（现 Web 只显示 token usage，`web-app/src/lib/utils/stream-parser.ts:31`）；metrics 增加引擎名维度（`crates/infra/wf-metrics/src/collectors/compaction.rs:11`）。
* **安全边界四条不可协商**：native-only；引擎名唯一、冲突即失败（§4）；**ShrinkProof 宿主复算**（§6，默认开启下的硬闸门）；失败熔断上移到 host（连续失败达阈值后停止自动压缩，只允许手动恢复，不信任引擎自融）；`engine_api_version` 参与启动校验（仿 `ADVISOR_PROTOCOL_VERSION`，`compaction/strategy.rs:39`）。

## 10. 落地路线与验收

| 阶段 | 范围 | 关键改动 | 验收标准 |
|---|---|---|---|
| **P0** | 契约与装配 | trait + 贡献类型 + 进程级工厂表 + 配置项；实例级唯一绑定（entity 字段 + checkpoint engine_id）；包装 `WorkflowCompactionEngine`；下线 advisor/workflow 接管/hook 接管三套机制；host 统一 consumer | 现有 e2e 与回归全绿；单一实例只跑一套压缩逻辑（日志可见引擎名）；`docs/archive/dynamic.md` 追加登记完成 |
| **P1** | 写入与手动入口 | `WritebackOp::Replace` 默认路径 + 宿主复算 ShrinkProof；`/compact` 手动命令；checkpoint 引擎状态回填 | 手动压缩可复现；破坏式写回失败可回滚；跨 checkpoint 恢复后引擎仍唯一 |
| **P2** | 可观测与示例 | 进度事件/状态卡/分引擎指标；一个示例插件引擎替换 builtin 并跑通同一套测试 | 示例引擎完整替换 builtin、行为等价；缓存前缀/分页等特定策略由示例引擎自行验证（框架不内置） |

**落地纪律**：遵循 `AGENTS.md:5-6` 无向后兼容要求，不做双轨、不留兼容分支；每阶段只改原模块，不新建并行系统（与 `docs/plan/context-compression-followup-design.md:3` 的既有纪律一致）。

## 11. 待拍板开放点清单

已由用户拍板并落定的项不再列为开放点：Q2（Replace 默认开启 + 宿主复算 ShrinkProof）、Q3/Q4（被新引擎取代的 advisor 与 hook 接管机制直接下线、不做兼容）、Q7（不为模型主动申请单开扩展点；未来走通用事件触发工具，不与 Weave 对齐）、缓存前缀/分页接续（不作为设计输入，交由引擎自定义）。其余暂按推荐方案，仅保留以下真正待定项：

| # | 开放点 | 备选 | 建议 |
|---|---|---|---|
| Q1 | 引擎配置的 profile 归属粒度 | 按 agent profile / 按单次执行显式指定 | 按 profile 解析引擎名，工厂表进程级按名索引；实例级绑定见 §5，二者不冲突 |
| Q5 | `engine_state` 版本不匹配策略 | 丢弃回退 Full 视图 / 拒绝启动 | 运行期丢弃并告警，启动期仅校验 `engine_api_version` |
| Q6 | 手动命令端覆盖顺序 | CLI → TUI → Web / 三端同时 | 先 CLI 与 TUI 共用，Web 随后（Web 当前不消费压缩事件，`stream-parser.ts:31`） |
| Q8 | ShrinkProof 的 r_min 阈值 | 复用 `r_min_range`（`budget.rs:119`，默认 2000）/ 新配一项 | P1 落地时对照头房预算原则核定；这是默认开启下的关键闸门，需在示例中验证 |
| Q10 | 实例级引擎绑定的具体落点 | `executor.rs:148` coordinator() / `lifecycle/entity.rs:160` 实体构造 | 推荐 `executor.rs:148`：此处 profile 配置与 entity id 均已可达（核验结论），且 consumer 已按 execution id 过滤（`conversation_compression.rs:61`） |
| Q11 | 三套机制下线的验收口径 | 以「单实例单引擎」日志断言 / 以回归全绿 | 两者都要：日志断言保证无并行，回归全绿保证行为不回退；下线顺序建议 advisor → workflow 接管 → hook 接管，每步跑一次回归 |

## 12. 代码索引

| 文件 / 位置 | 职责 |
|---|---|
| `crates/engine/wf-agent/src/coordinator/iteration/compression.rs:141,328` | 强制压缩、决策入口（L5/L2 改造点） |
| `crates/engine/wf-execution-shared/src/compaction/decision.rs:18,29` | 决策枚举与函数（L0 包装为 builtin） |
| `crates/engine/wf-execution-shared/src/compaction/plan.rs:195,453` | 计划构造与建议校验（L2 下线建议、保留纯规划） |
| `crates/engine/wf-execution-shared/src/compaction/budget.rs:6-9,95-107,119` | 四档阈值、默认比值、r_min（L4 TokenMeter / Q8） |
| `crates/engine/wf-execution-shared/src/compaction/strategy.rs:39,45-56,61-63,96` | 协议版本、strategy、advisor 表（L2 下线）、进程参数 |
| `crates/engine/wf-execution-shared/src/context_store.rs:16-30,36,44,176,205` | WritebackOp、超预算闸门、压缩请求、信号结果（L3/L2） |
| `crates/engine/wf-agent/src/conversation_compression.rs:42-45,51-62,127-142,243-265` | consumer 实例绑定、checkpoint、版本写回（L2/L3/L5） |
| `crates/engine/wf-agent/src/executor.rs:148`、`coordinator/lifecycle/entity.rs:160-161`、`entity.rs:46-47` | 实例创建与唯一引擎绑定落点（§5.1/Q10） |
| `crates/foundation/wf-types/src/message/message_view.rs:20,35-40` | 视图投影与分段（L3） |
| `crates/foundation/wf-types/src/hook.rs:45,73-77,234-242` | hook 事件类型约束与压缩信号屏蔽（L2/§8.2） |
| `crates/app/wf-runtime/src/trigger_listener/compression/pipeline.rs:77,138,210` | 内建执行流水线（L2 包装为 builtin） |
| `crates/infra/wf-plugin/src/contributions/types.rs:28-90`、`wf-plugin-sdk/src/contributions.rs:712-730` | 31 种贡献类型与 advisor trait（L1/L2 下线） |
| `crates/foundation/wf-types/src/config/limits.rs:59-96`、`wf-config/src/processor/limits.rs:41-49,117-152` | 配置字段与默认值（L1/Q8） |
| `infra/checkpoint/coordinator.rs:444,474-487,535`、`crates/foundation/wf-types/src/agent_execution/loop_config.rs:65` | checkpoint 实例状态与引擎字段缺口（§5.1） |
| `crates/engine/wf-tools/src/predefined/context.rs:13-50`、`wf-tools/src/tool/execution.rs:33-59` | 现有检索工具与通用工具结果（L4/§8.2） |
| `crates/infra/wf-metrics/src/collectors/compaction.rs:11,39`、`docs/archive/dynamic.md:1-40` | 指标采集器与动态派发登记格式（L6/L0） |

## 13. 落地实现记录（随代码持续更新）

本节记录方案的代码落地状态与相对原设计的务实调整，取代上方路线图中已过期的表述。所有条目均经 `cargo check --workspace` 验证（EXIT=0）。

### 13.1 已落地

| 项 | 落点 | 说明 |
|---|---|---|
| CompactionEngine 契约 | `wf-execution-shared/src/compaction/engine.rs` | 三方法 trait + `CompactionCall`（只读快照）+ `CompactionOutcome` 三态 + `ProgressSink` + `EngineCapabilities` + 工厂 trait |
| builtin 引擎 | `compaction/builtin.rs` | 包装 `decide_compaction` + `build_plan`，行为与原路径等价 |
| 进程级工厂注册表 | `compaction/registry.rs` | 按名索引、重名/保留名拒绝、`engine_api_version` 校验、未知名启动期报错 |
| 配置面 | `wf-types/config/limits.rs` + `wf-config/processor/limits.rs` | 新增 `compaction.engine`、`compaction.engine_config` |
| 实例级唯一绑定 | `wf-agent/coordinator/iteration.rs` + `lifecycle/run.rs` | 每次执行新建 iteration coordinator 时解析一次引擎并冻结；自动压缩入口改调引擎 |
| 宿主复算 ShrinkProof | `compaction/engine.rs` 的 `validate_replacement` | 忽略引擎自报、能力声明校验、区间校验、reclaim 阈值；3 个单元测试覆盖 |
| advisor 彻底下线 | 跨 6 crate | plan 不再咨询建议；删除贡献类型/trait/进程表/FFI/bridge/测试，全仓无悬空引用 |
| 溢出接线 | `iteration/compression.rs` | `publish_forced_compression` 改调实例引擎的 `compact_region`；builtin 退化为 stateless plan，保持「forced 无 directive」不变量 |
| 手动入口核心 | `entity.rs`、`lifecycle/run.rs` | 引擎随装配绑定到 entity，`AgentLoopEntity::compact_now` 调用引擎 `compact_now` 返回 outcome；含单元测试 |
| 动态派发登记 | `docs/archive/dynamic.md` | 追加「压缩引擎动态派发」一节 |
| 可替换性验证 | `compaction/registry.rs` 测试 | 示例引擎工厂注册→构建→重名拒绝→保留名/版本校验，3 个测试证明第三方可替换 builtin |
| 回归验证 | 全 workspace | `cargo check --workspace` 通过；wf-execution-shared 280、wf-agent 210、wf-config 402 单测全绿 |

### 13.2 相对原设计的调整（务实边界）

* **引擎字段绑定在 `AgentIterationCoordinator` 而非 `AgentLoopEntity`**：`AgentLoopEntity::new` 在全仓有数十个调用点（含大量测试与 placeholder），加必填字段会造成大范围破坏。iteration coordinator 同样是每次执行新建（`lifecycle/run.rs`），且压缩决策方法（自动/手动/溢出）本就是它的方法，是更自然的绑定点。checkpoint 恢复走同一装配路径，引擎按配置名重建，仍满足「跨恢复唯一」。
* **摘要执行链（hook → pipeline → workflow）保留为宿主服务，不作为并行压缩逻辑下线**：核验确认 `CompressionPipeline` 内无 `decide_compaction`/`build_plan`，它只执行引擎已决定的摘要计划，不具备独立压缩决策。把它从 hook 迁到引擎服务需要做完整 LLM 调用注入（跨 `wf-agent` 不能依赖 `wf-workflow` 的分层约束），收益低、风险高。真正的「并行压缩决策」来源是 advisor，已彻底删除；现在一个执行实例的压缩决策唯一归属绑定引擎。
* **`strategy` 保留为 builtin 引擎内部的摘要链选择**（`strategy_workflow_ids`），语义已收紧：只在 builtin 引擎下生效，不再是可独立注册的压缩接管机制。
* **`EngineServices` 收敛为 `ProgressSink`**：分层约束下引擎不能直接持有 LLM 路由/归档句柄（这些在更高层）；所有写副作用通过 `CompactionOutcome` 表达、由宿主执行，引擎唯一直接调用的是不影响控制流的进度上报。

### 13.3 第二轮落地（基线 496b86cb，细化方案见 `docs/plan/compaction-engine-remaining-tasks.md`）

本轮把 §13.1 遗留的四项全部落地（Web 命令因前端不可在本环境验证，按 Q6 列为随后项）：

| 项 | 落点 | 说明 |
|---|---|---|
| 破坏式 Replace 写回链 | `conversation_session/history.rs` `replace_range_with_view`；`wf-agent/conversation_compression.rs` `apply_replacement` | 视图层破坏式替换：history 保留可回滚，view 收窄为单摘要，归档引用登记到 `CompactionState.replacement_archives`；`validate_replacement` 复算 proof 后宿主按 anchor 版本写回。自动路径（`iteration/compression.rs`）与手动 `entity.rs compact_now_applied` 均已消费 `Replace` |
| checkpoint 引擎快照 | `wf-types/agent_execution/loop_config.rs` 增 `compaction_engine`/`compaction_engine_config`；`entity.rs compaction_engine_snapshot`；`checkpoint/coordinator.rs` 快照合并；`lifecycle/run.rs` 恢复优先用快照引擎、未知引擎 Q5 回退 builtin | 实例引擎跨恢复唯一；全仓 32 个全字段 `AgentLoopConfig` 构造点补字段 |
| `/compact` 命令（API + CLI + TUI） | `wf-api/src/compaction.rs`、`wf-server/src/api/compact.rs`（`POST /agent-loops/{id}/compact`）；`wf-mini` SlashCommand::Compact；`tui-core` `session.compact` + `wf-tui` request_compact_now | 命令层经 HTTP API 取 `ctx.agent_loop(id)` 调 `compact_now_applied`；返回 `{applied,kind,engine,detail}` |
| 可观测与示例引擎 | `compaction/engine.rs` `TracingProgressSink`；`wf-metrics` `record_engine_outcome(engine,kind)` + `ENGINE_OUTCOMES`；`examples/compaction-engine-demo/` | 示例引擎实现 trait + 工厂，注册→构建→重名拒绝→注销单测通过，证明完整替换 builtin |

**验证**：`cargo check --workspace --tests` 通过；`cargo test -p wf-execution-shared -p wf-agent` 全绿（含新增区间替换 2 项、apply_replacement 2 项）；示例引擎 crate `cargo test` 1 项通过。

**务实调整**：
1. 写回链采用**视图层破坏式替换**而非真删 history：快照/回放不变量不变，`replace_range_with_view` 可逆（`restore_full_view`）；`WritebackOp::Replace` 作为 wire 契约保留，供后续嵌套 agent 事件路径使用。
2. `archive → hash verify → persist → publish` 在单进程内以「归档登记 → ShrinkProof 复算 → 会话快照持久化 → 完成事件」落地，未引入新存储协议。
3. Web 命令挂载按 Q6 列为随后项（前端改动无法在本沙箱编译验证）。

### 13.4 仍待续

* **Web `/compact` 命令**：`apps/web-app/src/lib/config/commands.ts` 加 action、`chat-session.svelte.ts` 加 REST 执行器、i18n 文案；消费 start/end 事件的状态卡。
* **完整事件链**：`compaction/start`、`compaction/end` 独立 `EventType` 尚未新增（当前用 metrics engine 维度 + tracing progress 代替）；若 TUI/Web 需独立订阅再补。
