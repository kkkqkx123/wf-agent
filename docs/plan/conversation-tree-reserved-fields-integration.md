# 对话树预留字段的正式集成方案：trace / ParamsFingerprint.tools / OpRecord

> 对象：`docs/plan/conversation-branch-tree-design.md` 的落地实现（当前仓库 commit `4c21ec3`）。
> 性质：设计方案 + 实施结论。回答三个问题——三个预留字段如何正式集成、`Reply.trace` 是否过度设计、可否用更简单的方式替代。
> 证据约定：所有现状判断均附 `file:line`，以当前代码为准，不采信设计文档中的旧行号。

## 0. 结论摘要

| 字段 | 现状（代码事实） | 是否过度设计 | 本方案决策 |
| --- | --- | --- | --- |
| `Reply.trace` | 有真实写入路径：`adopt.rs:104-108` 把线性历史里的 `Tool` 消息归档进 trace；但**新生成的轮次恒空**（`turn.rs:620` 传 `Vec::new()`） | **不是过度设计**。它是消息内容寻址 + 脏传播 + 上下文回放的唯一载体，删除会破坏不变量 | **正式激活**：新增窄驱动，在正常生成路径填充真实工具轨迹 |
| `ParamsFingerprint.tools` | 所有构造点恒为 `Vec::new()`（`turn.rs:189`、`step.rs:318`、`query.rs:164`）；`model_drift` 判定（`query.rs:135-136`）因 `parameters` 也恒为 `None` 而几乎不触发 | 字段本身不过度，但**缺填充逻辑**使它成为摆设 | **正式填充**：会话级白名单贯穿提交 → 生成 → 快照，让 `model_drift` 真正工作 |
| `OpRecord` | 14 处手术都在写（`surgery.rs:129` 等），但结构缺 `params`（`tree.rs:294-303`），全仓无读取端，`undo` 端点不存在 | **半截子工程**：付了写入成本却换不来可恢复性；设计文档 §6.9「P0 就写避免补不了历史」的意图落空 | **正式实现 undo**：补 `params` + 逆操作派生 + 端点，让写入产生价值 |

**一句话**：不做删除式倒退，把三个字段从"预留半成品"补成"行为自洽的闭环"；`trace` 保留并用窄驱动激活，白名单贯穿全链路，`OpRecord` 补齐参数后落地 undo。

---

## 1. 现状核实

### 1.1 `Reply.trace`：并非死字段，但生成侧从未填充

`Reply.trace: Vec<MessageRef>` 定义于 `tree.rs:125`，注释明确它是"assistant `tool_calls` 后跟 `tool` 结果的有序往返"。它有三项真实职责：

1. **摘要折叠**：`digest.rs:116` 把 trace 中每个 `MessageRef.hash` 折进 `reply_part`，因此工具结果变化会让下游 `base` 变化、进而变脏。
2. **上下文回放**：`projection.rs:68` 把 trace 按原序注入投影，使子节点生成时能"看到"上一轮的工具往返。
3. **线性历史升级**：`adopt.rs:104-108` 把旧线性会话中的 `Tool` 消息归档进 trace，保证升级后的树上下文完整。

问题在于写入侧断裂：`finish_reply`（`surgery.rs:506-548`）是唯一能写 trace 的入口，而唯一调用点 `turn.rs:617-624` 传入 `Vec::new()` 与 `None`。**读取三处、写入两处（一处是迁移），正常生成路径零写入**——这就是"预留但未激活"的精确含义。

### 1.2 `ParamsFingerprint.tools`：恒空导致 `model_drift` 形同虚设

`model_drift` 的判定是 `query.rs:135-136` 的 `reply.params.as_ref() != Some(current)`。`ParamsFingerprint` 有三字段：`profile_id`、`parameters`、`tools`（`tree.rs:108-116`）。

- pending 回复写入 `profile_id`，但 `parameters: None, tools: vec![]`（`turn.rs:186-190`）。
- 快照里的"当前参数"同样是 `parameters: None, tools: vec![]`（`query.rs:161-165`）。
- 步进写入也是 `tools: vec![]`（`step.rs:315-319`）。

于是 `model_drift` 实际只在 profile 切换时触发。设计文档 §7.5 要求白名单"声明与执行两侧都拒"，但白名单**根本没有进入数据模型**，驱动层无白名单可校验。

### 1.3 `OpRecord`：写而不读，缺逆操作参数

`OpRecord`（`tree.rs:294-303`）字段为 `op / target / nodes / version_before / version_after / at`。设计文档 §6.9 承诺的是 `{op, params, version_before, version_after}`——**`params` 从未落地**。

- 写入：`surgery.rs:659-673` 的 `record()` 被 14 个手术动词调用（`:129/:170/:204/:294/:367/:397/:439/:464/:500/:541/:567/:600/:638` 等），环形缓冲上限 200（`tree.rs:15`）。
- 读取：全仓仅 `lib.rs:68` 的类型重导出，**无任何消费点**。`undo` 端点不存在（`wf-server/api/conversation.rs` 路由表 `:24-88` 无 undo）。

结论：每笔手术都付出写放大成本，却换不来任何可恢复性。这是"为未完成步骤预付复杂度"的典型。

---

## 2. `Reply.trace` 是否过度设计：分析与简化方案

### 2.1 为什么它不是过度设计

"过度设计"的指控通常针对 `trace` 作为**独立字段 + 参与摘要折叠**的复杂度。但逐条核对后，这套复杂度有明确、不可替代的语义：

- **弃用成本**：消息不可变、内容寻址（hash 剔除 id/timestamp/usage，`design.md §5.2`）。若把工具往返拼进 `reply.message` 文本，则工具结果的 hash 无法单独折叠，"结果变了下游仍新鲜"的误判无法被检测；回放时也无法把工具消息以正确 role 注入模型（`Assistant.tool_calls` / `Tool`）。
- **不变量要求**：`design.md §7.1` 明确投影顺序是 `context → user → trace → reply.message`，trace 是上下文的一等公民。
- **迁移已依赖它**：`adopt.rs` 已经在写它，删除会丢失旧会话的工具历史。

因此结论是：**字段必要，缺的是激活**。

### 2.2 更简单的方案为何被否决

考虑过三种简化，均不采用：

| 简化方案 | 被否决的原因 |
| --- | --- |
| A. 删除 `trace`，工具结果拼进 `reply.message` 文本 | 破坏内容寻址与脏传播；工具消息无法以正确 role 回放；`adopt.rs` 无法迁移 |
| B. 把 trace 降级为"仅展示、不进 digest" | 工具结果影响模型答案，必须进脏判定；否则"结果变了答案没重算"是正确性缺陷 |
| C. 用 `OpRecord` 式的通用 JSON blob 存工具轨迹 | 丢掉 `MessageRef` 的 hash 语义，摘要折叠要重新解析，反而更复杂 |

**结论**：保留 `Vec<MessageRef>` 是当前数据模型下的最简正确解。真正该做的是补齐窄驱动，让它从恒空变为真实可用。

### 2.3 关键取舍：不碰 `ChatSession` 的工具边界

设计文档 §7.5 已论证：`ChatSession` 结构上无 tools 字段（`chat.rs:20-24`）、`generate_with_tools_once` 强制模型必须返回工具调用（`single_shot.rs:81-86`）且缺"工具后二次生成"。因此**新增独立窄驱动**，而不是给 `ChatSession` 加工具能力——保持线性 chat、one_shot、对话树三者的抽象边界不被抹平。

---

## 3. 正式集成方案

### 3.1 只读工具轮窄驱动（激活 trace + tools）

在 `wf-execution-shared` 新增 `readonly_tool_round.rs`，提供 `generate_readonly_turn`：

```
输入: gateway, registry, request(含 tools 声明), allowed_tools, cancel
流程: 1) 校验 allowed_tools 非空、声明工具 ⊆ allowed_tools
      2) gateway.generate(request) —— 一次生成
      3) 若返回 tool_calls: 校验 ≤4、逐个在白名单内、执行（无 checkpoint）
         若返回纯文本: 合法，直接作为答案（修复 single_shot 的"必须有工具调用"限制）
      4) 把工具往返构造成 Message 序列返回，作为 reply.trace
输出: ReadonlyTurnOutcome { generation, trace: Vec<Message> }
```

与 `generate_with_tools_once` 的差异：允许"零工具调用"的正常文本答案（只读轮是"可以用工具"而非"必须用"），并回传可归档的轨迹消息。约束沿用 `MAX_SINGLE_SHOT_TOOL_CALLS = 4`（`single_shot.rs:20`）。

**接线**：`TurnRequest` 增加 `allowed_tools: Vec<String>`（默认空 = 纯文本，行为不变）。`prepare_request` 在有白名单时把工具声明注入 request；`generate_answer` 分流：有白名单走窄驱动、无白名单走原流式。收敛时把 trace 消息落库为 `MessageRef` 列表并写入 `finish_reply`，同时把生效的 `ParamsFingerprint`（含真实 tools、parameters）写回回复。

### 3.2 `ParamsFingerprint` 全链路填充

- `submit_turn` 写入 pending 回复时，把 `allowed_tools` 排序后写入 `tools`，把会话 generation 参数写入 `parameters`。
- `snapshot` 构造 current_params 时用同一口径（profile + 真实白名单 + 参数），使 `model_drift` 在工具集变化时正确触发。
- `step` 重算时沿用相同 params，避免"重算后 drift 误报"。

### 3.3 `OpRecord` + undo 正式实现：**快照式撤销**（简化方案）

#### 为什么放弃逐手术逆操作

设计文档 §6.9 的原始设想是"记录每个手术的逆向操作参数，undo 时派生逆操作"。落地核查后发现该方案有硬伤：

- `splice_out` 把子节点提升到父节点时**重新分配了 ordinal**（`surgery.rs:155-160`），原 ordinal 未记录，逆操作无法精确复原；
- `detach` / `attach` 跨主树与 graft 文档搬运节点，逆操作要同时协调两个文档的落库顺序；
- 14 种手术要写 14 套逆逻辑，每套都要重新走一遍摘要折叠，维护成本与出错面都很大。

这正是"能否用更简单的方式解决"的正面回答：**树文档本身可完整序列化**（`ConversationTree` derive `Serialize/Deserialize`，`nodes: BTreeMap`），消息体在独立存储且不可变，所以"整树快照"既是充分状态，又天然免于内容重建。

#### 数据结构

给 `OpRecord` 增加 `before: Option<serde_json::Value>`（手术前的整树快照），`#[serde(default, skip_serializing_if = "Option::is_none")]` 保证旧文档兼容；`CONVERSATION_TREE_SCHEMA_VERSION` 升至 2。保留 `params: Option<serde_json::Value>` 作为手术语义标签（如 `{"node": ..., "parent": ...}`），便于 UI 展示"撤销了什么"。

| 字段 | 用途 |
| --- | --- |
| `before` | 手术前的完整 `ConversationTree` JSON，undo 的唯一真相源 |
| `params` | 人类可读的手术参数摘要，仅用于展示 |
| `version_before` / `version_after` | 乐观并发校验 |

代价：每条快照是整份树文档（消息只存 `MessageRef = id + hash`，不含正文），对上限 `MAX_TREE_NODES` 的对话树是可控的；200 条环形缓冲（`tree.rs:15`）封顶。

#### undo 算法（单一函数，不依赖手术类型）

```
undo(session, if_version):
  1. 写锁 + require_chat_session + if_version 校验
  2. 载入树，取 tree.ops.last()；无则 404
  3. 反序列化 record.before -> ConversationTree
  4. 校验快照.schema_version == 当前（否则拒绝并提示迁移）
  5. 用快照替换 tree 的 nodes/active_leaf/pool/next_ordinal，
     但保留 tree.id/session_id/kind/profile_id/created_at 不变量
  6. version = 快照.version（回退版本号），重建 DigestIndex
  7. 弹出最后一条 op，落库 + 发 tree_changed / dirty_changed
```

因为快照携带了手术前的摘要相关全部字段，`base` 折叠、`dirty`、脏徽标全部自动复原——与 `conversation.rs:22-25` 的不变量"staleness 由内容摘要派生，撤销无需清理"完全吻合。

#### undo 端点

`POST /api/v1/chat/sessions/{id}/undo`，body 可选 `if_version`。带乐观并发校验；无可撤销记录返回 404。撤销本身不记新 op（避免 undo 栈自指）；如果需要"再撤销"，由下一次真实手术产生新记录。

**取舍**：只做"撤销最近一次手术"（对齐 F14 的"一键回退"语义）。不做任意深度的历史回放——快照链可以做到，但每次撤销只能前进一格已覆盖绝大多数误操作场景，且保持实现单一函数、易测。

---

## 4. 实施清单

| 步骤 | 改动 | crate | 风险 |
| --- | --- | --- | --- |
| 1 | 新增 `readonly_tool_round.rs` + 单测 | wf-execution-shared | 低（新文件） |
| 2 | `OpRecord.params` + schema v2 | wf-types | 低（加字段，serde default） |
| 3 | `record()` 携带 params，各手术补参数 | wf-execution-shared | 中（14 处调用） |
| 4 | turn/step/query 填充 ParamsFingerprint + trace 落库 | wf-api | 中 |
| 5 | undo 领域函数 + 路由 + OpenAPI | wf-api / wf-server | 中 |
| 6 | 文档同步（本文件为设计源，实施后补状态） | docs | 低 |

## 5. 与其他分析的关系

- 本文是 `conversation-branch-tree-design.md` §7.5、§6.9 的落地细化，纠正其"undo P2 / 只读工具轮第 7 步未做"的现状描述。
- `design-review.md` §4 的架构结论（不下沉、不建 `wf-chat` crate）不受本文影响；本文所有改动都在既有 crate 边界内。
- 架构归位（对话树纯逻辑下沉 `wf-api`）仍是独立待决策项，本文不推进，避免与功能集成耦合。

---

## 6. 实施状态（本次落地结果）

> 状态基准：当前 working tree，工具链 Rust 1.97（仓库 `rust-toolchain.toml` 写 `stable`，
> 但子模块 `simvec` / `checkpoint-base` 已用到 1.97 特性，故以 1.97 验证）。

### 6.1 已完成并通过编译验证

| 项 | 实现 | 文件 |
| --- | --- | --- |
| schema v2 + 快照字段 | `OpRecord.before/params`（serde default 兼容 v1），`SCHEMA_VERSION=2` | `wf-types/.../conversation/tree.rs` |
| 快照自动捕获 | `TreeEditor::new` 在任何手术前捕获整树快照，`record()` 写入每条 op | `wf-execution-shared/.../conversation_tree/surgery.rs` |
| undo 核心 | `TreeEditor::undo_last()`：恢复快照结构字段 + 重建 digest + 回退版本；两个新 `TreeError` 变体 | 同上 + `invariants.rs` |
| undo 领域层 | `conversation::undo_last()`，走会话写锁 + `if_version` 校验 | `wf-api/.../conversation/surgery.rs` |
| undo 路由 | `POST /api/v1/chat/sessions/{id}/undo` + utoipa 注册 | `wf-server/.../api/conversation.rs`、`openapi.rs` |
| 窄驱动 | `readonly_tool_round.rs`：`generate_readonly_turn` 允许零工具调用的正常答案，白名单执行、回传可归档 trace，含 3 个单测 | `wf-execution-shared/src/readonly_tool_round.rs` |
| ParamsFingerprint 统一口径 | `current_fingerprint(ctx, profile, tools)`：turn 写入、step 重算、snapshot 判定共用，填入 profile 的 parameters + generation | `wf-api/.../conversation/query.rs`、`turn.rs`、`step.rs` |
| undo 单测 | 快照恢复节点/digest、空历史报错 | `surgery.rs` 测试模块 |

验证命令（RUSTUP_TOOLCHAIN=1.97.0，均通过）：
- `cargo check -p wf-types -p wf-execution-shared -p wf-api -p wf-server` —— Finished
- `cargo check -p wf-execution-shared --tests` —— Finished
- `cargo test -p wf-execution-shared --lib conversation_tree::surgery` —— **13 passed**（含 2 个新增 undo 用例）
- `cargo test -p wf-execution-shared --lib readonly_tool_round` —— **3 passed**
- `cargo test -p wf-api --lib --features keyword-tantivy,keyword-elastic,rag-vector conversation` —— **7 passed**

> 注：不带 feature 的 `cargo test -p wf-api --lib` 会因 `rag.rs` 引用了被 `#[cfg(feature = ...)]` 门控的函数而编译失败（仓库预存问题，与本次改动无关）；lib 的 `cargo check` 正常。

### 6.2 明确留作后续（未在本次改动，避免高风险流式重写）

1. **窄驱动与生成链的完整接线**：`TurnRequest.allowed_tools` 入口、`prepare_request` 注入工具声明、`generate_answer` 分流、SSE 工具事件、trace 消息落库到 `finish_reply`。这一步要重写流式生成路径并新增 UI 事件，风险与验证成本最高，应在独立迭代中配合端到端测试完成。当前窄驱动是**可独立调用、已单测的执行原语**，接线点已在 §3.1 写明。
2. **会话白名单产品入口**：`tools` 目前由 `current_fingerprint` 结构化填充但入参为空；需产品定义会话级 `allowed_tools` 的配置与下发后才会有非空值。
3. **OpenAPI 快照刷新**：新增 `/undo` 端点后需重新生成 `openapi.json` 与 `apps/web-app/src/lib/api/schema.d.ts`（本次未在离线环境重新 codegen）。

### 6.3 对"是否过度设计"的最终回答

- `Reply.trace`：**不过度**。它是内容寻址脏传播与上下文回放的必要载体，`adopt.rs` 已在真实使用；本次未删除，而是补齐了可执行它的窄驱动。
- 真正过度的是"预留却永不激活"的状态——本次把 undo 从"只写不读"补成闭环，并选择了**快照式**这一更简单、更易测的撤销实现，替代原设计的逐手术逆操作。
