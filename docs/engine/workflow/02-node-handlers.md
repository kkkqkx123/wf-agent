# wf-workflow 节点处理器

> 范围：`src/handler/` 子树及 `src/handler.rs`、`src/lib.rs` 工厂函数。
> 所有 `file:line` 相对 `crates/engine/wf-workflow/src/`。

## 1. 注册表与工厂

- `HandlerRegistry`（`handler.rs:51-61`）：按 `StaticNodeType` 分发处理器，附带两个可选共享注册表——`agent_loop_registry`（AGENT_LOOP 嵌套循环注册）与 `decision_registry`（决策配置档案）。缺注册表时节点级解析为结构化错误（`resolve_handler_registry`，`handler.rs:41-49`）。
- 标准处理器集在 `register_defaults_with_capture`（`handler.rs:140-182`）中注册：START/END、ROUTE、DECISION、VARIABLE、LOOP_START/LOOP_END、FORK/JOIN、SYNC、SUBGRAPH、LLM（含文件 checkpoint）、CONTEXT_PROCESSOR、SCRIPT、INTERACTIVE_SCRIPT、AGENT_LOOP、TOOL_VISIBILITY、EMBED_START/EMBED_END、USER_INTERACTION、START_FROM_MESSAGE/CONTINUE_FROM_MESSAGE。
- 工厂函数（`lib.rs:128-153`）：`create_default_handlers`（网关 + 沙箱）、`create_default_handlers_with_file_checkpoint`（附加文件 checkpoint 管理器）、`create_default_handlers_with`（规范构造器，将共享 agent 循环注册表接到 AGENT_LOOP 处理器，嵌套循环经其容量闸门注册）。

## 2. 控制流节点

### START / END（`handler/start_end.rs`）

幂等直通。重入（重试/恢复）时经 `__completed_{node}` 标记与 `loop_state::should_honor_completed_marker` 短路返回输入（`start_end.rs:32-40`、`71-75`）；首次进入置标记后放行（`start_end.rs:41-42`、`76-77`）。

### ROUTE（`handler/route.rs`）

按配置顺序对执行变量快照求值 `conditions[].expression`，第一个为真的表达式其 `target_node_id` 成为唯一后继（`route.rs:43-68`）；求值错误记日志并跳过该边；全不匹配时回退 `default_target_node_id`（`route.rs:71-78`）。

### DECISION（`handler/decision.rs`）

`DecisionHandler`（`decision.rs:49-51`）：校验 1–400 个问题（`decision.rs:81-86`）；连接方式支持共享注册表的 `decision_profile_id` 或内联 `base_url/model/protocol`（`decision.rs:189-271`）；组装 state/assets/questions（`decision.rs:287-529`）后调用外部决策服务，将 `{model, answers}` 写入 `output_variable`，发布 `decision_node_completed` 事件并记录 token 用量（`decision.rs:129-180`）。

## 3. 循环节点

### LOOP_START / LOOP_END（`handler/loop_handler.rs` + `src/loop_state.rs`）

- LOOP_START（`loop_handler.rs:33-168`）：读取 `loop_id` 与 `max_iterations`（默认 100）；运行期注入的 `__loop_max_iterations_cap`（否则 `MAX_ITERATIONS_CAP = 10000`，`loop_state.rs:23,31`）优先，超上限配置直接报错（`loop_handler.rs:52-62`）。首次进入解析可迭代对象（数组/对象/数字/字符串，`loop_handler.rs:267-307`）、导入 `variable_inputs` 并留快照、压入 `LoopState` 栈。`break_condition` 在起始节点对称生效，退出并路由到 LOOP_END（`loop_handler.rs:127-147`）。
- LOOP_END（`loop_handler.rs:188-261`）：求值继续/终止条件，终止时 `exit_loop` 并输出 `SHOULD_CONTINUE=false`，否则清 `iteration_started` 路回 `loop_start_node_id`。break 条件求值失败是错误而非"不退出"（`loop_handler.rs:359-370`）。
- 循环状态栈（`loop_state.rs:26`）：保留键 `__loop_state_stack` 下的 JSON 栈，嵌套循环以 push/pop 隔离（`loop_state.rs:93-119`），栈结构损坏即报错；栈参与 checkpoint。`LoopState`（`loop_state.rs:35-65`）记录 loop_id、可迭代对象、索引、迭代数、循环变量、导入变量快照（退出时还原或删除）与每轮 `iteration_nodes`（保证 checkpoint 恢复正确性）。`current_item`（`loop_state.rs:208-233`）产出数组元素/键值对/索引/字符。

## 4. 并行节点

### FORK / JOIN（`handler/fork_join/`）

- FORK（`fork_join/fork.rs:403-651`）：配置（`fork.rs:32-107`）含 `fork_paths`、`failure_strategy`（默认 fail_fast / continue_on_error / fail_on_threshold）、串行或并行 `fork_strategy`、分支/总超时、是否阻塞等待。执行时结构化推导 JOIN 节点（`fork_join/graph.rs:89-148`）、预生成分支执行 id、注册进 `ForkRegistry` 与父子层级（`fork.rs:426-472`），再串行（`fork.rs:185-231`）或并行（`fork.rs:291-384`）跑分支。
- 分支执行（`fork_join/branch.rs:69-407`）：`BranchRunContext` 驱动切片出的分支子图（`graph.rs:10-80`，处理嵌套 fork）。
- JOIN（`fork_join/join.rs:36-209`）：优先从活跃 `ForkRegistry` 收集分支结果，回退到 checkpoint 恢复场景的 `__fork_outputs_<fork_id>` 记录变量（带显式降级事件，`join.rs:81-121`）；应用分支超时、拆分成功/失败、合并输出并报告冲突（`join.rs:294-305`）。合并在 `fork_join/merge.rs`：分支变量聚合（`merge.rs:125-159`）、消息历史聚合（`merge.rs:171-265`）、数据输出主分支优先（`merge.rs:274-325`）。
- 屏障类型（`src/barrier.rs`）：`BranchResult`（`barrier.rs:8-32`）、`FailureStrategy::evaluate`（`barrier.rs:136-169`）决定整体结果、`ForkOutcome`（`barrier.rs:125-133`）。

### SYNC（`handler/sync.rs`）

跨分支读取：`execute_inner`（`sync.rs:41-445`）读取 `source_path_id` 与 `wait_for_completion`/`timeout`；先经层级管理器定位发起 fork 的注册表（回退图查找与确定性跨注册表扫描，歧义路径 id 冲突显式暴露，`sync.rs:69-118`）；等待源分支落定（超时映射为类型化 `TransportTimeout`），读取活跃分支变量/消息上下文或回退 fork 输出记录，再应用变量映射（`set_variable_path`）与消息同步。

## 5. 子图与嵌入

- SUBGRAPH（`handler/subgraph.rs:54-401`）：创建完全独立的子执行实体（独立 execution_id/变量表），拒绝层级深度溢出；只继承 wall-clock 预算、节点超时与 checkpoint 布线，step/暂停/导航/循环上限重置为引擎默认（`subgraph.rs:143-157`）；支持 `ignore_child_error` 容忍子失败、`ChildCancelOnDrop` 联动取消。
- EMBED_START / EMBED_END（`handler/embed_boundary.rs`）：EMBED_GRAPH 预处理展开产物，纯直通。
- 预处理（`src/preprocess.rs`）：`PreprocessedGraph`（`preprocess.rs:29-38`）为展开后的扁平执行图；解析内联 `graph_definition` 或注册的 `embed_id`（`preprocess.rs:53-71`），拒绝嵌入图携带变量/触发器/VARIABLE 节点。

## 6. LLM 与 Agent 节点

### LLM（`handler/llm.rs` + `handler/llm/`）

- 配置（`llm/config.rs:28-179`）：归一化 profile_id、生成参数、工具协议、执行超时、交互预算（拒绝 0）、流式标志与死循环检测。
- 消息组装（`llm/messages.rs:172-216`）：系统提示（`messages.rs:99-128`）、声明的命名消息上下文、注入消息、工具结果与作用域违规标记（`messages.rs:60-88`）；未被消费的锚点剥离并告警。
- 主循环（`llm.rs:132-449`）：token 追踪器持久化在变量中（`llm/token_budget.rs:18-49`），多轮工具循环受 `max_interactions` 与每请求 `max_tool_calls_per_request` 约束；每轮做压缩 settle 等待（`token_budget.rs:236-341`）与预检预算检查（`token_budget.rs:440-504`）；工具执行（`llm/tool_exec.rs:129-413`）含待处理调用队列与被拦截工具的审批流；流式分支（`llm/stream.rs:43-203`）跳出循环；作用域违规连续轮数封顶（`MAX_CONSECUTIVE_SCOPE_VIOLATION_ROUNDS`，`llm.rs:36`）。

### AGENT_LOOP（`handler/agent_loop.rs:67-301`）

将 `wf-agent` 的 agent 循环作为图节点运行：组装可缓存系统头 + 易变尾（`wf_execution_shared::agent_prompt`，`agent_loop.rs:84-120`）；绑定新 `agent_loop_id` 并构建 `AgentLoopConfig`（工具池、hook、迭代与 token 上限、文件作用域，`agent_loop.rs:172-213`）；入站会话历史按目标工具暴露面归一化（`agent_loop.rs:145-157`）；有工具注册表时先过 fail-fast 的 `AgentLoopValidator` 门禁（`agent_loop.rs:220-252`）；再阻塞或流式执行（`agent_loop/stream.rs`），导出会话与元数据（迭代数、loop id、消息数）。嵌套循环经共享 `AgentLoopRegistry` 容量闸门注册。

### CONTEXT_PROCESSOR（`handler/context_processor.rs:146-257`）

按优先级三段：(1) 超长内容的服务端 fold——服务失败一律转为跳过报告而非工作流失败；(2) `operation_config` 消息上下文操作经 `message_ops::apply` 应用并重新注册；(3) `variable_operation`：Aggregate / Transform / BatchUpdate。

## 7. 脚本节点

- SCRIPT（`handler/script.rs:228-461`）：flow-config 委派（`script.rs:468-619`）、内联/模板/注册脚本解析（带变量层模板渲染，`script.rs:257-317`）、沙箱配置构建（畸形配置 fail-closed，`script.rs:903-916`）、允许写范围校验（`script.rs:168-175`）、输出映射（`output_mapping.rs:7-33`）与文件 checkpoint 变更捕获（`script.rs:181-211`）。
- INTERACTIVE_SCRIPT（`handler/interactive_script.rs:128-582`）：持久会话（`execute_session`）与单发执行（`execute_single_shot`）双路径；消费 `__interaction_input__` 预设（见 `interaction_keys.rs`）；支持 shell 语言，可借助 LLM 网关做解释器式运行。

## 8. 交互与消息节点

- USER_INTERACTION（`handler/user_interaction.rs:98-293`）：`prompt` 走工作流变量层解析；同步路径消费预置的 `__interaction_response__`；异步路径注册进交互注册表、发布 `FollowupQuestionRequested` + `WorkflowExecutionPaused`、置 `__interaction_waiting__` 并带超时（默认 30 000 ms）等待应答；随后应用操作（`apply_operation`，`user_interaction.rs:306-377`）并写响应变量。超时/取消清理注册表并路由到类型化错误类别。
- START_FROM_MESSAGE / CONTINUE_FROM_MESSAGE（`handler/message_node.rs`）：START_FROM_MESSAGE（`message_node.rs:236-247`）做完成标记幂等、把触发消息输入映射进变量（`message_node.rs:115-157`）并可选应用触发动作（如 pause/stop 路由）；CONTINUE_FROM_MESSAGE（`message_node.rs:269-296`）导出 `variable_outputs`/`message_outputs` 并输出导出的消息数组（压缩子工作流的输出契约）。
- 交互保留变量协议（`handler/interaction_keys.rs:1-30`）：`__interaction_response__` / `__interaction_waiting__` / `__interaction_input__` / `__interaction_output__` 留在变量表中（不在活体注册表），保证预设跨 checkpoint 恢复存活；take 语义——成功消费、失败保留以重试。

## 9. 其他节点

- TOOL_VISIBILITY（`handler/tool_visibility.rs:72-154`）：`block` 置 `__tool_blocked_<tool>` 标记（仅运行期拦截，schema 不变以保 KV-cache），`unblock` 清标记并置 `__tool_activated_<tool>` 记激活指标；节点本身不写消息，下一个 AGENT_LOOP 重算合并通知。
- RAG 自定义节点（`handler/rag.rs`）：`RAG_QUERY`（查询，配置 `rag.rs:37-86`，含 top_k、分数阈值、查询变体、句窗口、自动合并、融合模式 `rrf/concat/weighted`、关键词混合）与 `RAG_INGEST`（摄取，`rag.rs:105-133`）；经 `register_rag_handlers`（`rag.rs:613-619`）单独注册，不在默认集内；节点状态只写命中/摘要变量，向量不落节点状态。
- 触发器上下文与动作（`handler/trigger/`）：`TriggerContext`（`trigger/context.rs:28-68`）携带执行/节点 id、共享变量表、事件与类型化信号总线、注册表、取消令牌、继承的父预算；`TriggerCoordinator::execute`（`trigger/coordinator.rs:24-151`）静态分发动作——stop/pause/resume、跳过节点、设置变量、消息上下文增删过滤、通知、子工作流与脚本动作。
- 变量映射助手（`handler/variable_mapping.rs`）：`inherit_all_variables`（父→子全量继承，剔除引擎内部键，用于触发器子工作流；SUBGRAPH 保持隔离）、`apply_variable_inputs`/`apply_variable_outputs`（必填缺失失败、可缺省回退）、`set_variable_path` 点路径写入（SYNC 复用）。

## 10. 跨节点通用模式

- **完成标记幂等**：START/END 与消息节点共用的 `__completed_{node}` + `should_honor_completed_marker`，保证重试/恢复不重复执行。
- **fork 输出回退**：JOIN/SYNC 在 checkpoint 恢复后从记录变量回退读取分支结果，并发布显式降级事件（`join.rs:81-121`、`sync.rs:103`）。
- **降级即事件**：回退路径不静默，发布 `data_degraded` 可观测事件（见 [06-checkpoint-persistence.md](06-checkpoint-persistence.md)）。
- **预算边界继承**：子图/触发器子执行只继承 wall-clock 预算类限制，防护类上限重置。
