# wf-agent Hook 设计合理性分析，及 Hook×Trigger 能否实现 Codex Hook 功能

> 分析方法：先 `git pull` 同步 wf-agent（main 已 up-to-date，本次应用了 `packages/storage` 的清理），再通读 `docs/components/hook-注册通知机制设计.md`（2026-08，Rust 侧 Stage 1-4 已完成），并**对照真实代码**核对：
> - `crates/wf-execution-shared/src/hooks/{types,receiver,registry,dispatch,emit,template}.rs`
> - `crates/wf-agent/src/{hook,approval,trigger,coordinator/tool}.rs`
> - 另参考 `docs/architecture/agent/11-trigger-system.md`
>
> 结论先行：**wf-agent 的 hook 设计在其自定的职责边界内是合理且干净的；但它刻意不实现 codex 那类「阻断/改写入参/权限/重试」控制语义——这些被拆到了独立的 approval 机制与 workflow 机制。因此「hook 单点」或「hook+trigger 组合」都复现不了 codex 的 PreToolUse/PermissionRequest 能力，必须算上 approval 才是完整拼图。**

---

## 1. wf-agent Hook 当前设计的真实形态

### 1.1 两层结构
- **静态定义**：`BaseHookDefinition`（`types.rs:8-24`）——纯数据：`hook_type` / `weight` / `condition` / `enabled` / `payload`（模板）/ `receiver`（指向动态接收方名字）。
- **动态行为**：`HookReceiver` trait（`receiver.rs:20-26`）——`name()` + `on_hook(&HookContext) -> HookOutcome`，`Send+Sync` 异步，注册进 `HookRegistry`。

### 1.2 同步屏障 dispatch（`dispatch.rs:56-160`）
引擎在 hook 点调用 `dispatch(...).await`，流程：
1. 对目标 `hook_type` 做 `condition/enabled/weight` 过滤（`filter_and_sort_hooks`）；
2. 模板解析 payload（`template.rs`，失败 warn + null 兜底）；
3. **按序同步 notify**：先 `BaseHookDefinition.receiver` 显式指名的接收方，再 `registry.for_type` 动态注册的（均按 weight 降序）；
4. 聚合 `HookOutcome`（首个 `Intercept` 生效，`aggregate_outcome` 实现）；
5. 发布 `HOOK_TRIGGERED` 审计事件（携带 `payloads` / `receivers`(name/outcome/duration_ms/error) / `duration_ms`）。

### 1.3 安全守卫（`registry.rs`）
- 每个接收方调用包 **3s 超时**（`HookRegistry::new()` 默认 `Duration::from_secs(3)`，`registry.rs:48`），超时→`warn` + 记为 `error` + 视作 `Continue`（`registry.rs:116-143`）；
- 按名去重（`register` 同名将忽略，返回 `false`，`registry.rs:61-75`）；
- `with_timeout` 可覆盖。

### 1.4 关键事实：调用方丢弃了 outcome
`wf-agent/src/hook.rs` 的 `emit_agent_hooks` / `emit_hooks`（`hook.rs:25-81`）调用 `dispatch(...).await` 但**不接收返回值**：
```rust
dispatch(registry_or_default(registry), entity.hooks(), hook_type, &ctx, event_bus).await;
```
`DispatchSummary`（含 `outcome: HookOutcome`）被直接丢弃。`HookOutcome::Intercept` 虽然被计算出来（`dispatch.rs:163-172`），**没有任何引擎消费方使用它**。这与设计文档 §9「控制语义不接线……`HookOutcome::Intercept` 仅作机制预留」一致。

### 1.5 点位全景（来自 `docs/components` 设计文档 §5，已对照代码）
| 点位 | 时机 | 携带数据 |
|---|---|---|
| `BEFORE_AGENT` / `AFTER_AGENT` | 循环开/结束（成败两分支） | execution_id / current_iteration / status |
| `BEFORE_ITERATION` / `AFTER_ITERATION` | 每轮迭代边界 | — |
| `BEFORE_LLM_CALL` / `AFTER_LLM_CALL` | LLM 调用边界 | AFTER 带 finish_reason |
| `BEFORE_TOOL_CALL` / `AFTER_TOOL_CALL` | 工具调用边界（含并行） | `tool_call_id` / `tool_name` / `tool_arguments`（`tool.rs:1076-1089`） |
| `BEFORE_USER_PROMPT` | 输入提交后、start 前 | prompt 摘要 |
| `SUBAGENT_START` / `SUBAGENT_STOP` | 子实体创建 / 子执行返回 | child_execution_id / agent_id / 结果 |

---

## 2. 设计合理性评估

### 2.1 合理之处
1. **同步屏障语义正确**：`dispatch` 直接 `await` 所有接收方完成（`dispatch.rs:56`），最贴合「边界等待」语义，无 mpsc/oneshot 中间跳——比旧的事件广播（无回执、Lagged 丢信号）严谨。
2. **行为/配置解耦**：静态 `BaseHookDefinition` 只声明「何时/条件/数据」，行为全在 `HookReceiver`（设计文档 §2）。压缩链 `CompressionService`（`name="context_compression"`，weight 1000）就是范例，证明 receiver 能同步接管（一次 await 后 spawn 摘要子 workflow）。
3. **硬防护到位**：3s 超时 + 跳过 + 记错（`registry.rs:116-143`），接收方**永不阻塞或失败引擎**——这点比 codex「hook 失败仍可中止操作」更保守也更安全。
4. **审计比旧版更完整**：`HOOK_TRIGGERED` 事件含 payloads、逐接收方 name/outcome/duration_ms/error（设计文档 §3、测试 `audit_event_published_with_results_summary`），已是 log-only 审计对偶（对齐 dsh 的 `hook/invoked`+`hook/result`）。
5. **注册表贯穿 + 兜底默认值**：`Option<Arc<HookRegistry>>` + `OnceLock` 默认实例（`hook.rs:12-17`），最小嵌入/测试仍可观察到审计事件，行为优雅退化。

### 2.2 风险与不足
1. **`Intercept` 是死机制（最大隐患）**：`HookOutcome::Intercept` 类型存在、`aggregate_outcome` 会聚合它，但 `emit_agent_hooks` 丢弃 `DispatchSummary` → 任何 receiver 返回 `Intercept` 都**静默无效果**。开发者写出「期望阻断」的 receiver 却毫无反应，这是典型的「机制预留但未接线」陷阱——与本项目此前已识别的 `restore_checkpoint` 死代码、`get_hierarchy_depth()→0` stub 属**同一类反模式**（`types.rs:32-40` 注释本身已写明「no engine consumer wires it yet」，等于把隐患写进了代码注释）。
2. **控制语义被割裂，对 codex/dsh 迁移者不友好**：codex 用户心智里「PreToolUse 阻断/改输入」就是 hook；wf-agent 把它塞进 `approval.rs` 的 `ToolApprovalHandler.request_approval`（`approval.rs:65-67`，返回 `approved`/`edited_parameters`/`rejection_reason`）。职责分离（SoC）本身合理，但**两套表面并存**易让使用者误以为 hook 也能拦工具。
3. **trigger 只能事后动作**：`docs/architecture/agent/11-trigger-system.md` 描述 trigger 是 `EVENT/SCHEDULE/CONDITION/MESSAGE → RUN_AGENT/RUN_WORKFLOW/CALLBACK/EMIT_EVENT` 的 match-action，且「在 `AFTER_ITERATION` 之后执行」（文档 §6）。它是**副作用动作**，不返回控制流，**无法在事前门禁**——这决定了它补不了 hook 缺失的前置拦截。
4. **没有 retry / request-error 钩子点**：codex 无显式 retry hook，但 dsh 有 `agent/request-error`（可决定重试）。wf-agent 的 LLM 重试走内部 `retry_budget`（`tool.rs:842` 重试 `under retry budget`），**没有任何 hook/approval/trigger 能介入「是否重试」**。这是相对 dsh 的真实缺口。
5. **部分点位数据偏薄**：`BEFORE_TOOL_CALL` 已带 `tool_name`/`tool_arguments`（够观测），但 `BEFORE_LLM_CALL` 未携带可改写的请求体、`BEFORE_USER_PROMPT` 仅 prompt 摘要——若未来想让 receiver 做「改写请求/注入上下文」，当前 payload 不足以支撑（与 §9「不通过 hook 改写入参/注入上下文」的自我设限一致，但说明边界内能力也有限）。

---

## 3. Hook×Trigger 能否实现 Codex Hook 的功能？

把 codex hook 的能力逐条拆解，映射到 wf-agent 的三个系统（**hook / approval / trigger**）：

| Codex 能力 | wf-agent hook | approval | trigger | hook+trigger 能否达成 | 真实机制 |
|---|:--:|:--:|:--:|:--:|---|
| 全生命周期审计（11 事件点） | ✅ `HOOK_TRIGGERED` | — | — | ✅ | 直接等价 |
| 子 agent 生命周期钩子 | ✅ `SUBAGENT_START/STOP` | — | — | ✅ | 直接等价 |
| 边界副作用（如压缩接管） | ✅ receiver 同步接管 | — | — | ✅ | `CompressionService` |
| **前置拦截工具调用** | ❌（审批后才发） | ✅ `approved=false` | ❌ | **靠 approval** | `tool.rs:463` 先 `approve_tool_calls` 再发 hook |
| **改写入参** | ❌ | ✅ `edited_parameters` | ❌ | **靠 approval** | `approval.rs:23` |
| **权限决策** | ❌（Intercept 未接线） | ✅ | ❌ | **靠 approval** | `ToolApprovalHandler` |
| **停止/反转决策**（Stop） | ❌ | — | ❌ | ❌ | 无对应 |
| **重试决策**（request-error） | ❌ | ❌ | ❌ | ❌ | 内部 retry_budget，无钩子 |
| 上下文注入（下次采样可见） | ⚠️ 仅 receiver 副作用改共享状态 | — | ⚠️ 事后动作 | ⚠️ 间接 | 非一等公民 |
| 预/后压缩（Pre/PostCompact） | ⚠️ `CONTEXT_COMPRESSION_REQUESTED` 接收方 | — | — | ⚠️ 部分 | 不新增 PRE/POST_COMPACT 类型 |
| 事后自动化（事件→跑 agent/workflow） | ❌（仅通知） | — | ✅ | ✅ 靠 trigger | `RUN_AGENT/RUN_WORKFLOW` |

### 3.1 三个核心结论
1. **codex 的「控制流 hook」（阻断/改写/权限）在 wf-agent 中根本不由 hook 承担，也不由 trigger 承担，而是由独立的 `approval` 机制承担**。所以「hook+trigger 组合」复现不了这部分——必须引入 approval 才是完整拼图。这是设计文档 §9 的明确取舍（「阻断…不通过 hook 体系实现（approval 与 workflow 机制承担）」）。
2. **wf-agent 反而在「事后自动化」上超过 codex**：trigger 能从事件直接 `RUN_AGENT`/`RUN_WORKFLOW`（在 `AFTER_ITERATION` 后触发子 agent），codex 没有这种一等公民的事件→agent 触发器。
3. **retry / request-error 是三系统共同的真实缺口**：codex 不强、dsh 有，wf-agent 完全没有可介入点。

### 3.2 所以，直接回答用户问题
- **「hook 设计是否合理」**：在其自定边界内**合理且干净**（同步屏障 + receiver 解耦 + 超时硬防护 + 审计对偶）。主要风险是 `Intercept` 死机制与「控制语义割裂」带来的认知负担。
- **「hook+trigger 能否实现 codex hook 功能」**：**部分能，且需要拆解看**：
  - 观察/审计、子 agent 生命周期、边界副作用（压缩）、事件→agent 自动化 → ✅ 已被 hook + trigger 覆盖（甚至更强）；
  - 前置拦截、改写入参、权限决策 → ❌ hook+trigger 都做不到，必须靠 **approval**；
  - 重试决策 → ❌ 三个系统都没覆盖。
  - 即：**wf-agent 用「hook（通知+边界行为）+ approval（前置门禁）+ trigger（事后自动化）」三系统分布实现了 codex 单点 hook 的功能，但没有任何单一组合（含 hook+trigger）能完全等价**。

---

## 4. 改进建议

### P0（消除隐患）
- **处理 `Intercept` 死机制**：二选一——(a) 在 `emit_agent_hooks` 接收 `DispatchSummary` 并把 `Intercept` 接到 coordinator 控制流（至少 `BEFORE_TOOL_CALL`/`BEFORE_LLM_CALL` 可据此中止），让 hook 真正获得与 codex PreToolUse 对等的前置拦截力；或 (b) 若坚持不改控制流，则**删除 `Intercept` 变体、只留 `Continue`**，并在 `HookReceiver` 文档中明示「hook 不做控制」，避免误导。当前「保留类型但不接线」是脚枪。

### P1（补能力缺口）
- **补 `request-error` / retry 钩子点**：在 LLM 调用错误分支（`tool.rs` 重试前）加 `LLM_REQUEST_ERROR` hook 点（或让 approval 也覆盖「是否重试」），对齐 dsh 的 `agent/request-error`。
- **明确「三系统模型」文档**：在 `docs/components/hook-*.md` 与 trigger 文档交叉写明「前置门禁用 approval、边界通知/行为用 hook、事后自动化用 trigger」，消除 codex 迁移者的心智错位。

### P2（可选增强）
- **富化 `BEFORE_LLM_CALL` payload**：若未来要让 receiver 支持「改写请求/注入上下文」这类 codex 式语义，需携带可改写的请求体；否则维持现状并文档化「hook 不承载逻辑」。
- **考虑配置兼容桥接**：与 dsh 主动桥接 CC/Codex `hooks.json` 的思路一致，wf-agent 若想复用既有生态，可在 approval+hook 之上做一层 `hooks.json` 适配器（低成本迁移资产）。

---

## 5. 一句话总结

> **wf-agent 的 hook 是「同步屏障 + receiver 行为 + 审计」的干净通知层，设计本身合理；但它与 trigger 都只覆盖 codex hook 的「观察/生命期/边界副作用/事后自动化」一半，codex 的「阻断/改写/权限」被有意拆到了 approval 机制。因此「hook+trigger」复现不了 codex 前置门禁能力——要等价必须算上 approval，且当前 `Intercept` 死机制与「无 retry 钩子」是两处应当尽快收口的真实短板。**
