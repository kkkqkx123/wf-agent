# 工具路由、可见性与审批

> 范围：`crates/engine/wf-agent/src/tool_router.rs`、`tool_snapshot.rs`、`visibility.rs`、`decision_policy.rs`、`approval.rs`、`coordinator/tool.rs` 及 `coordinator/tool/` 子树。

## 1. 工具路由：`ToolRouter`

每次 agent 迭代装配"本轮可见工具集"的唯一收敛点（`src/tool_router.rs:1-8`）。持有四层工具桶（`tool_router.rs:29-42`）：

| 桶 | 语义 |
|---|------|
| `visible` | 本轮模型可见、直接渲染进 LLM 请求的工具 |
| `discoverable` | 仅注入元数据到 prompt，调用须经由 `general` 代理 |
| `gated` | 已注册但未初始暴露，须通过 TOOL_VISIBILITY 解锁激活后可见 |
| `hidden` | 注册了但永不暴露 |

- 装配计算在共享 crate `wf_tools::resolve_tool_exposure` 中完成，`From<ExposureResolution>` 只做转换，保证 agent-loop 元数据注入与 schema 使用同一套过滤决策、不会漂移（`tool_router.rs:5-8, 44-53`）。
- **KV-cache 友好设计**：可见 schema 只由白名单 + exposure 层 + 正式激活推导；运行期 `block` 动作不改 schema，仅在执行期拦截，使 LLM 请求前缀跨轮字节级稳定（`tool_router.rs:10-14`）。
- 暴露层级（由强到弱）：hidden 黑名单 > 每次运行的 exposure 覆盖 > discoverable 配置列表 > 元数据默认 Direct（`tool_router.rs:16-20`）。

## 2. 快照与冻结：`ToolSnapshot`

在 loop 入口对一次运行的工具成员做"冻结快照"，供每轮重算对比（`src/tool_snapshot.rs:4-12`）：

- 存储四层排序后的工具声明 + 每层的稳定指纹（指纹覆盖名称、描述、参数 schema 的声明内容，`tool_snapshot.rs:45-48`），并记录 `general_enabled`。"同名 schema 变更"也算漂移。
- `diff_snapshot`（`tool_snapshot.rs:134-162`）对比冻结快照与实时解析结果，产出四层各自的 added/removed、同名但定义变更的 `modified_definitions`、`general_changed`——在无外部版本号的情况下区分"有意的 loop 边界变更"与"后台注册表漂移"。
- `render_tool_change_text`（`tool_snapshot.rs:181-270`）把 diff 渲染成 `[Tool Change]` 纯文本通知（被 block 的工具带 "do not retry" 提示）。

## 3. 工具可见性：`visibility.rs`

TOOL_VISIBILITY 工作流节点向共享变量表写入 `__tool_blocked_<name>` / `__tool_activated_<name>` 布尔标记（`src/visibility.rs:18-22`）：

- `VariableBackedVisibilityStore`（`visibility.rs:27-49`）实现 `ToolVisibilityStore` trait，`is_tool_visible` = 未被 blocked 变量标记。block 是纯运行期拦截——模型可见 schema 不变（KV-cache 友好），且直接调用与 `general` 内层调用共用同一执行期拦截点（`visibility.rs:1-8`）。
- `collect_blocked_tools` / `collect_activated_tools`（`visibility.rs:68-97`）遍历变量表收集集合；activated 集合由 AGENT_LOOP 节点读取用于初始化 `ToolDiscoveryState`。

## 4. 审批

**共享契约**（定义于 `wf-execution-shared/src/approval.rs`，由 `src/approval.rs:4-6` re-export）：

- `ToolApprovalHandler` trait：唯一方法 `async fn request_approval(&ToolApprovalRequest) -> ToolApprovalResult`（`approval.rs:84-86`）。
- `ToolApprovalRequest`：tool_call_id、tool_name、arguments、风险级别、工具描述、批量信息（batch_id/tool_index/total_tools）、pending_queue（同批待审列表）（`approval.rs:23-34`）。
- `ToolApprovalResult`：approved、edited_parameters（审批人可改参）、rejection_reason（`approval.rs:37-40`）。
- `RejectionMessageBuilder`（`approval.rs:10-87`）：拒绝消息模板——全局默认模板、按工具模板覆盖、可选工具变更提示注入。

**`ToolApprovalGate`**（`coordinator/tool/approval.rs:20-23`）批量审批引擎：

1. 无 handler 且无 options → 全部自动放行快速路径（`approval.rs:53-60`）；
2. 构造审批请求数据，经 `ToolApprovalCoordinator` 策略评估（`approval.rs:62-104`）；
3. **策略拒绝是终态**，绝不升级为人工审批（`approval.rs:106-114`）；
4. `Ask` 类 pending 路由给注册的 handler；等待人工审批期间**暂停超时计时**，不消耗墙钟预算（`approval.rs:169-171`）；
5. handler 缺省时 fail-closed：需人工审批而无 handler → 拒绝（`approval.rs:184-189`）。

## 5. 工具执行协调器：`ToolExecutionCoordinator`

单次执行的完整工具执行协调器（`coordinator/tool.rs:39-74`），builder 式注入工具注册表、事件总线、hook、执行模式（Sequential/Parallel，`tool/types.rs:14-19`）、取消令牌、可见性 store、checkpoint handler、文件作用域、重试预算、`general` invoker、审批配置等（`tool.rs:77-307`）。

- **执行入口** `execute_tool_calls`（`tool.rs:373-382`）→ 顺序路径（`tool.rs:444-545`）/ 并行路径（`tool.rs:547-828`）；并行时单个调用先执行可见性闸门拒绝（gate rejection 与执行失败区分）；`cancel_on_failure` 可中止同批任务。
- **`ToolRunCtx`**（`tool/types.rs:82-116`）：顺序/并行共享的不可变执行上下文。审批快照在建 ctx 时固化，`general` 内层调用复用同一审批闸门——高危工具无法借 discoverable 路径绕过审批（`types.rs:87-92`）。
- **`runner.rs`**：单工具执行的共享流水线 `run_tool`（`runner.rs:31-473`）——超时解析（默认 120s + 30s 安全余量）、进度发射、失败分类、失败/拒绝消息构造、风险级别/描述元信息提取。
- **`general.rs`**：`GeneralToolContext` 实现 `GeneralToolInvoker` trait（`general.rs:238-288`），是 discoverable 工具的代理执行器：
  - 解析请求体中的多条内层调用，每条派生稳定内层 id（`outer#index#tool`），checkpoint 重放命中缓存而非重放副作用（`general.rs:268-275`）；
  - `check_inner_tool_allowed`（`general.rs:52-106`）：拒绝递归调用 `general` 自身、hidden 工具、未激活的 gated 工具、以及不在解析结果可调用集合中的工具——与 schema 装配共用同一 `resolve_tool_exposure`（单一决策源）；
  - `approve_inner`（`general.rs:113-142`）：内层调用重跑同一审批闸门；
  - 每个 item 独立产出结果/`{"error": reason}`，互不吞没；首次通过 general 发现的工具记录 discovery 状态并发事件。

## 6. Decision 决策模式：`decision_policy.rs`

服务 `action_policy == Decision` 的 agent 模式——不用 function-calling，而是把"下一步选哪个工具"作为一次 LLM 选择题决策（`src/decision_policy.rs`）：

- `resolve_action_policy`：无配置默认 `Chat`（`decision_policy.rs:3-7`）；`decision_policy_for`：Chat 模式下请求决策策略报错、Decision 模式缺 policy 也报错（`decision_policy.rs:9-22`）。
- `build_tool_choice_request`：把观察文本作为 state，工具列表 + `done` 选项构造 `DecisionRequest`（`decision_policy.rs:24-62`）。
- `select_tool_choice`：解析决策响应；`done` 映射为 `None`（结束循环），无效选择报验证错（`decision_policy.rs:64-98`）。
