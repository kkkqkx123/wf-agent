# 通用旁路请求（Aside）与 resources 内置实现设计方案

**Goal:** 把现在散落在 coding 与 harness 里的四份「临时问一次模型、拿一段文本、按某种去向消费」的实现，收敛成插件层的一个原语（`Aside` + 调度行 + 三个去向），让 recap 退化为其中一个特殊实现；并把可按需启用的多套实现放进 `resources/`，新增用途只加资源与配置行，不加协议变体、不加 runtime 生命周期。
**Architecture:** 原语住在 `atomcode-harness` 的插件层：`AsideHubSvc` 是一个注册表缝（同 `tools::mount`），`aside-runner` 行订阅已提交的 `SessionEvent`（`StepEnd` / `TurnEnd`）并按每个 aside 自己声明的节奏发起旁路调用；答案只有三个去向——屏幕注记（`AsideNoteSink` → `HostEvent`）、白名单事实（`session::commit`）、既有决策缝的内容（`Compaction`）。`CodingRuntime` 仍是唯一 runtime owner，kernel 协议不变，压缩触发权仍归 `CompactBeforeRequest`。
**Tech Stack:** Rust（tokio / async-trait / serde）、plexus 插件树、`include_str!` 资源表、cargo nextest。

Baseline: `main` @ `002b7b1c3`（2026-10-22）。本轮只产出文档。前一轮的 recap 传输扩展方案见 [2026-10-26-recap-extension-design.md](2026-10-26-recap-extension-design.md)；本文不改那份方案的结论，只回答「要不要把旁路请求本身做成通用能力」。

## 1. 结论：需要改造，但改造点在插件层

需要，而且值得做。理由是重复已经发生了四次，而不是「将来可能会用到」：

- `recap`、`next-prompt suggestion`、`session title`、`compaction summary` 是同一件事的四个实例——一次低产出、无工具、有超时的旁路模型调用，加上一段清洗，再加一个去向。
- 但每次新增用途都要重写一遍形状：coding 里要新 `mod`、新 `CodingRuntimeControl` 变体、新 `HostCommand`/`HostReply`、新 TUI 渲染块；harness 里要新 trait、新缝、新行。第四种用途（例如「每 N 步看看计划是否过期」）按现状就得把上面那套再抄一遍。

改造边界三条，本方案严格遵守：

1. **原语只负责「问」和「把答案送到某个既有去向」。** 它不拥有回合生命周期、不新增回合终态、不新增压缩触发、不新增 kernel 命令。
2. **recap 是特殊实现，不是原语。** 它的特殊之处是：必须用会话自己的模型（要读本会话账本）、只在 `TurnEnd`、只给屏幕。这些是 `Aside` 的**参数**，不是 `Aside` 的定义。
3. **「多套内置实现」用 resources + 行清单。** 不给每个用途加枚举变体，否则「通用」只是把散落的代码换成一个不断变长的 `match`。

## 2. 现状

### 2.1 四处旁路调用，四份手写实现

| 用途 | 实现位置 | 触发 | 模型槽 | 去向 | 失败降级 |
| --- | --- | --- | --- | --- | --- |
| recap | `coding/src/recap.rs` + `runtime.rs::recap()` + `Control::SideCallProvider` | 前端在 `TurnComplete{Stopped}` 后判定 | 会话模型（`side_call_provider`） | 仅屏幕（`RecapBlock`） | 静默 |
| next-prompt suggestion | `coding/src/next_prompt_suggestion.rs`，回合末 `tokio::spawn` | 同上 | 会话模型（`provider_factory.build`） | `HostEvent::Suggested` | 静默 |
| session title | `harness/src/plugins/session_title.rs::ModelTitle` | 第一条 `UserMessage` 提交 | `LlmUtilitySvc`，无则 `ModelsSvc.current()` | `SessionEvent::Titled` 事实 | 回退到首句截断 |
| compaction summary | `harness/src/plugins/compaction.rs::SummarisingCompaction` | `CompactionAsk`（`Auto`/`Overflow`） | `LlmUtilitySvc` | `CompactionDecision` | 回退到无模型列表摘要 |

四者的共同形状完全一致：`reasoning_effort: Low`、`temperature: 0.2`、`ToolChoice::None`、无 `max_tokens`、超时兜底、只收 `TextDelta`。差异只有四个维度：**何时问、用哪一档模型、答完去哪、失败怎么降级**。这正是原语该参数化的部分。

`coding/src/next_prompt_suggestion.rs` 的文档注释已经把这件事说破了一半：`sample_side_call` 是「the shape every side call of the runtime takes」。问题是这个形状只存在于 coding 的私有模块里，harness 的两次旁路（标题、摘要）各自又写了一遍流式收集。

### 2.2 已经具备、不要重造的零件

- **日志投影**：`SessionLog::derive_messages()` 是「日志变成提示词」的唯一入口（`session.rs:164`）。旁路请求要的上下文就是它，不需要 runtime 快照。
- **两档模型**：`LlmSvc`（`"llm"`）与 `LlmUtilitySvc`（`"llm-utility"`）。coding 树里 `llm-injected` 提供的正是 `side_call_provider` 返回的那个 provider，所以「会话模型」在插件层可直接解析，不必再走 `SideCallProvider` 控制命令。
- **每 N 步的观察点**：`SessionEventCommitted` + `SessionEvent::StepEnd { turn, step, tool_calls }`。`todo_reminder` 已经用这个入口做 `after_steps`（默认 3）判定，是现成范例。
- **每请求前的观察点**：`Waterfall<AgentRequest>`；`CompactBeforeRequest` 已经在里面按 token 压力判断，并用 `first_try(session, turn, slow)` 保证每回合每档只试一次。
- **决策缝**：`Compaction`（`describe` / `calls_model` / `compact`）与 `CompactBeforeRequest` 是压缩触发的唯一 owner。
- **装配机制**：`plugins::catalog()`、`on_harness::plugins()`、`Layer` 的 `patch`/`swap`/`disable`/`insert`、`CODING_DEFAULTS`/`CODING_ROWS`，以及 bundle + profile 的文件化装配。
- **资源表先例**：`atomcode-review/src/rules.rs` 用 `const RULE_DOCS: &[(&str, &str)] = &[("go", include_str!("../rules/go.md")), …]` 把几十份提示词编进二进制。这就是本项目已有的「多套按需资源」形态。
- **推送先例**：`CodingRuntimeEvent::NextPromptSuggested` → CLI 宿主 → `HostEvent::Suggested` → 屏幕。旁路结果要上屏，走的是同一条路。

### 2.3 现有缝为什么不够（AGENTS.md 的前置核对）

AGENTS.md 要求：涉及 turn completion / compaction 时，先证明现有 seam 缺失，再决定是否新增，且不得新增重叠 hook 或第二压缩状态机。核对结果：

- `LlmUtilitySvc` 只回答「用哪个模型」，不回答「何时问、问什么、答完去哪」。它不是请求缝。
- `Compaction` 是决策缝：`calls_model` 决定「这次压缩要不要调模型」，`compact` 决定「怎么折叠」。把「临时请求」塞进它，等于给压缩加第二个触发入口。
- `SessionTitle` 是名字缝，不是通用请求缝；它恰好只服务一个用途。
- 现有回合钩子（`TurnFinishing`、`AgentRequest`）都能承载调度，但都不提供「多个旁路用途共享一套请求/清洗/去向」的抽象。

所以新增的是**请求与去向**，不是第二套生命周期，也不与上述任何缝重叠。

## 3. 三个去向，三种语义（不可混同）

| 去向 | 语义 | 现有例子 | 落日志 | 模型可见 | 生命周期 |
| --- | --- | --- | --- | --- | --- |
| `Screen` | 临时视图注记 | recap | 否 | 否 | 切会话/新回合即消失 |
| `Fact` | 一条会话事实 | session title（`Titled`） | 是 | 视变体而定 | 随日志持久化，resume 可重放 |
| `Decision` | 进程内决策结果 | compaction summary | 否 | 否 | 只属于当次请求 |

把三者合成一个 API 是这类设计最常见的错误：屏幕注记一旦落日志，resume 就会重放一条没人说过的话；决策结果一旦落日志，模型下一轮会读到一段它没有参与的推理。

本方案的规则：

- `Screen` 永远不写日志、不进下一请求、不参与滚动历史的计算。
- `Fact` 只允许**白名单变体**。当前白名单只有 `SessionEvent::Titled` 和 `SessionEvent::Injected { origin: InjectionOrigin::Reminder }`。`AssistantMessage` 永远不在白名单里（理由见 §4.6）。
- `Decision` 不新增 API：由既有缝（`Compaction`）自己持有，`Aside` 只提供「问出来的文本」。

## 4. 设计

### 4.1 新原语：`Aside` trait + `AsideHubSvc`

`crates/atomcode-harness/src/seams.rs`:

```rust
plexus_service!(
    AsideHubSvc => dyn AsideHub, "aside", Seam,
    "临时旁路请求的调度台；每个旁路用途是一个可装卸的行"
);
```

不是「一个缝一个 provider」，而是注册表：同一个缝里要能同时存在多个用途（recap、todo 提案、上下文提示），逐个启停。这与 `tools::mount(ctx, vec![…])` 的形状一致——工具也不是互相替换，而是共存在一个注册表里。

```rust
pub enum ModelSlot { Conversation, Utility }
pub enum Cadence { TurnEnd, EverySteps(u32), Manual }

pub enum AsideMoment {
    TurnEnd { turn: u64 },
    Step { turn: u64, step: u32 },
    Manual { turn: u64 },
}

pub enum AsideOutput {
    /// 这次没有值得说的。等价于「没发生」。不显示、不提交、不报错。
    Silent,
    /// 只给屏幕：临时注记，切会话即消失。
    Note { label: &'static str, text: String },
    /// 落日志的事实，必须通过白名单校验。
    Facts(Vec<SessionEvent>),
}

#[async_trait]
pub trait Aside: Send + Sync {
    fn name(&self) -> &'static str;
    fn describe(&self) -> String;
    fn cadence(&self) -> Cadence;
    fn model(&self) -> ModelSlot;
    fn timeout(&self) -> Duration;

    /// 从日志投影出提示词。`None` = 这次不值得问：不发起请求，不花一次调用。
    /// 这是与「问了但没答出东西」不同的状态，前者连请求都不该发。
    fn prompt(&self, log: &SessionLog, at: AsideMoment) -> Option<Vec<Message>>;

    /// 清洗并决定去向。返回 `Silent` 是正常结果，不是错误。
    fn interpret(&self, raw: &str, log: &SessionLog) -> AsideOutput;
}
```

设计要点：

- **`prompt` 返回 `Option`，而不是让模型自己判断「有没有东西可说」。** 现有 recap 的做法是先问、再在 `sanitize_recap` 里把 `<none>` 变成 `None`，代价是明知没什么可问也要付一次调用。通用原语把这个判断前移：投影不出稳定上下文（例如最后一条不是稳定 assistant）、或距上次成功太近，就直接不发请求。
- **`interpret` 同步、无 IO。** 它只做清洗与去向分类；真正的「提交事实」由调度行执行，这样白名单校验只有一处。
- **`model` 是必需参数，不是默认值。** recap 必须用会话模型（要读本会话账本、凭据归属也应是本会话的）；标题与摘要默认用 utility 模型。默认值会把这个区别悄悄抹掉，重演「recap 换到了另一个厂商的模型」的事故。

### 4.2 调度行 `aside-runner`

`crates/atomcode-harness/src/plugins/aside.rs`，一个 `Plugin`：

- `uses(&["aside", "session", "llm", "llm-utility"])`——模型槽是运行时解析的，patch 换模型立即生效（同 `ModelTitle` 的注释：「Resolved per call, so a patch that swaps the utility model … applies to the next title」）。
- `apply` 里订阅 `SessionEventCommitted`，只处理 `StepEnd` 与 `TurnEnd`；`Manual` 不经日志，由 §4.3 的请求入口触发。
- 计数按 `(aside_name, session_id)` 存 `Mutex<HashMap<…>>`：
  - `EverySteps(n)`：只在 `step % n == 0` 时问；`TurnStart` 清零。
  - `TurnEnd`：每次回合结束问一次，但同一 (session, turn) 只问一次。
  - 同一 aside 同一会话最多一个 in-flight：在途时再次触发**直接跳过**，不排队（排队只会让迟到的答案更没意义）。
- 只对 root 会话运行，子 agent / team 成员跳过（与 recap 现有门槛一致）。
- 只 `tokio::spawn`，从不 `await`：调度行绝不阻塞模型请求，也绝不改变回合终态。这是与 `CompactBeforeRequest`（会 await 一次压缩）最关键的区别——旁路请求没有资格让主流程等它。
- 结果回来时校验 `(generation, session, turn)`；会话已切、回合已过、行已卸载则丢弃。沿用 TUI `recaps` ticket 的同一做法。
- `AsideOutput` 的分发：
  - `Silent` → 什么也不做。
  - `Note` → `AsideNoteSinkSvc`（§4.3）。
  - `Facts` → 白名单校验通过后 `crate::session::commit`。写法与 `todo_reminder` 一致：**锁内判定、锁外提交**，避免重入 `SessionEventCommitted` 死锁。

### 4.3 屏幕去向：`AsideNoteSink` + `HostEvent::AsideNote`

屏幕注记必须能从 harness 内部走到前端。现成的路是「runtime 事件 → 宿主 → 前端事件」：

```
aside-runner (harness 内)
  └─ AsideNoteSinkSvc            // 宿主注入的缝，只做 fn note(AsideNote)
       └─ CodingRuntimeEvent::AsideNote { session_id, label, text }
            └─ HostEvent::AsideNote { session, label, text }        (CLI 宿主)
                 └─ TUI: AsideBlock(label, text)                    (RecapBlock 泛化)
```

- `AsideNoteSinkSvc` 是宿主按 `InjectProvider` 的同一方法注册的（携带 per-mount 状态，`CodingRuntime` 建 App 时 `swap` 进去）。
- 没有 sink（headless、库用户、测试树）时直接丢弃：屏幕去向本来就是可选的，这也让原语在无前端装配里仍然可用（只走 `Facts`）。
- `HostEvent::AsideNote` 与现有 `HostEvent::Suggested` 同型：**不是日志事实、不是对话内容**，前端在会话切换后必须丢弃。
- daemon / webui 不在第一版映射这个事件。理由同 recap 扩展方案 §5.1：先请求-响应，等确有多端共享需求再进 `LiveWireEvent`。

### 4.4 resources：多套内置实现的落点

目录形态（与 `atomcode-review/rules/` 同构，提示词是文件、装配是行）：

```
crates/atomcode-harness/resources/asides/
  recap/
    aside.toml
    prompt.md
  todo-steward/
    aside.toml
    prompt.md
  context-hint/
    aside.toml
    prompt.md
```

`aside.toml` 只描述**事实**，不描述行为：

```toml
id = "recap"
cadence = { turn_end = true }      # 或 { every_steps = 4 } / { manual = true }
model = "conversation"             # 或 "utility"
timeout_secs = 25
output = "note"                    # note | fact
[limits]
max_chars = 240
```

`crates/atomcode-harness/src/resources.rs`：

```rust
/// Built-in aside resources, compiled in. `<home>/asides/<id>/` overrides by id.
const ASIDE_DOCS: &[(&str, &str, &str)] = &[
    ("recap", include_str!("../resources/asides/recap/aside.toml"),
              include_str!("../resources/asides/recap/prompt.md")),
    ("todo-steward", include_str!("../resources/asides/todo-steward/aside.toml"),
                     include_str!("../resources/asides/todo-steward/prompt.md")),
    // 新增一套实现 = 加一行 + 加一个目录 + 加一个 `[[insert]] aside-<id>` 行
];
```

- 每个内置实现默认 `disabled`。装配里写 `[[insert]] name = "aside-recap" disabled`，启用就是删掉 `disabled`（或 `[[patch]]`）。这就是「按需使用」。
- 部署/用户可在 `$ATOMCODE_HOME/asides/<id>/` 放同名资源覆盖内置（与 `Profiles::with_home` 同构）。覆盖只换提示词与数值上限，仍要过同一套 `AsideRow` 反序列化与白名单校验；不因为来自磁盘就放宽。
- **提示词不进 `config.toml`。** 配置树是可打印的数据（`dump_runtime` 原样渲染），提示词进配置会被当成「用户可改行为」，也会让 `describe_self` 的配置说明变成一份产品文案。resources 是代码资产，不是配置。
- 资源只在被行点名时加载；一行都不写时，`resources` 只是二进制里的几段常量，没有运行时成本（与 `catalog` 注释「a plugin that no row names is inert」同一立场）。

### 4.5 与现有实现的关系：recap 降级为特殊实现

分两步，第一步不改变任何行为：

- **Phase A（原语先落地，recap 不动）**：`Aside` + hub + runner + sink + resources + `aside-recap`（`cadence = Manual`）。`CodingRuntimeHandle::recap()` 保持现状，现有测试一条不改。此时 `aside-recap` 只用于对照：同一个 prompt 走两条路，输出应逐字相同。
- **Phase B（迁移）**：把 `recap.rs` 的 `INSTRUCTIONS`、`MAX_RECAP_CHARS`、`sanitize_recap` 搬进 `resources/asides/recap/`，`aside-recap` 改为 `cadence = TurnEnd`、`model = Conversation`、`output = Note`。TUI 的 `wants_recap` 判定**留在 TUI**（终端焦点、回合时长、屏幕行数只有前端知道），触发改为「请求 run」而不是「请求文本」。`HostCommand::Recap` 保留一个版本周期，内部委托给 hub；之后删除。
- 迁移判据不是「新测试通过」，而是**旧测试原样通过**：`the_recap_is_written_by_the_conversation_model`（请求数 +1、prompt 含 "Write a recap"、未新建 provider）与 `recap.rs` 的三个清洗单测，在 aside 实现下不改断言也成立。任何一条需要改断言，说明语义漂了。

### 4.6 每 N 步更新 todo：可行路径与不做什么

先把权威关系说清：**todo 的唯一权威是对话里的 `todowrite` 调用**——`reduce_todos` 从消息折叠，TUI 面板、内核消息投影、ACP 读的都是同一次折叠。`.todos` 旁挂明确只是缓存（`TodoSidecar` 的注释写明 `message_count` 是 stale 标记，读者发现对不上就丢弃、回退到 transcript 派生）。所以「旁路对话直接改 todo」不是加一个 aside 就能完成的，它必须先回答「一个非模型行为如何成为 todo 事实」。

三条路，各自的真实代价：

1. **提案（可实现，推荐先做）**：`aside-todo-steward` 每 N 步问一次「当前计划是否已经过期 / 是否该重排」，输出两样东西——给模型的 `<system-reminder>`（`Injected { origin: Reminder }`，含建议的新列表）与给人的 `Note`。写入仍由主 agent 的 `todowrite` 完成。代价：主 agent 仍是写者，不完全满足「不再由主 agent 完成」；收益：不动协议、不动折叠、可随时关掉。
2. **合成模型调用（不推荐）**：把结果写成 `AssistantMessage { tool_calls: [todowrite] }` + `ToolResultLogged`。所有 fold 立刻生效，但日志会声称主模型调用过它从未调用的工具。`assert_model_visible_is_logged` 只保证「可见即可追溯」，不保证「所记即所发生」；这条会同时破坏 replay 的诚实与「模型看到的都是它做过的」这一直觉。
3. **窄事实 + 折叠加宽（唯一忠实做法，另立 ADR）**：新增 `SessionEvent::TodosWritten { turn, todos, origin }`，`reduce_todos` 的输入从 `(name, args)` 扩为 `TodoWrite` 流；消费点包括 `capabilities::tools::todo`、TUI 面板、ACP 命令、daemon 投影、`derive_current_todos`。这是 kernel 公共事件格式变更，必须单独立项：写 ADR、列全 fold 点、处理 resume 兼容（旧日志没有该变体照常读）与 `active_todo_calls` 的「用户中断后作废」语义。不能夹在 aside 原语里顺手做。

本方案只做 1，把 3 写成明确的后继项与前置条件。这样「每 N 步判断」的调度能力立刻可用，「不再由主 agent 完成」则作为一个需要协议评审的独立决定，不被原语绑架。

### 4.7 压缩：不新增判断

- **触发权不迁。** 「要不要压缩」仍由 `CompactBeforeRequest` 按 `last_prompt_tokens` 与窗口比例决定，并用 `first_try` 限制每回合每档一次。把这套判断改成 LLM 旁路调用，就是第二压缩状态机——AGENTS.md 明确禁止，且会带来「模型说该压缩、但 token 预算还很宽」这类无法收敛的分歧。
- **唯一合法用法是复用「问」这一半。** `SummarisingCompaction::ask` 已经是旁路请求的形状（`LlmUtilitySvc` + 超时 + 只收 `TextDelta`）；Phase C 可以让它复用原语的请求实现，触发与决策仍归 `Compaction`/`CompactBeforeRequest`。
- 如果产品坚持要「语义上的压缩提示」，只能产出 `Note`（例如「上下文已用 75%，建议 `/compact`」），**不得**回调 `compact()`、不得改写 `CompactionAsk`。提示是给人看的，决策是给状态机用的。

### 4.8 备选方案与否决理由

| 备选 | 否决理由 |
| --- | --- |
| 给 `AgentCommand` 加 `Aside { … }` | kernel 不该知道「临时请求」这种产品概念；回复还得挤进 25 个 `AgentEvent` 的语义里，且每个前端都要实现新的请求-响应配对 |
| 给 `HostCommand` 每个用途加一个变体 | 前端被迫知道用途枚举；新增用途要同时改 host-api、cli、tui 三个 crate 与协议序列化 |
| 把 recap 扩成「万能 recap」 | 模型槽（会话 vs utility）、上限、去向三者不同，挤进一个实现只会互相污染；这正是现在四处重复的根源 |
| 所有旁路统一走 `LlmUtilitySvc` | recap 必须读本会话账本，换模型会改变内容，也把凭据归属挪到了另一个槽；`side_call_provider` 的注释专门写了「Never the side-call model」 |
| 把提示词搬进 `config.toml` | 配置树可打印、可被 `dump_runtime` 原样渲染；提示词进配置等于把产品行为交给用户，也让「配置只描述装配」这条边界失效 |

## 5. 约束与失败语义（对照 AGENTS.md）

- **不新增 runtime owner。** 调度行只订阅已提交事实、只读日志、只经 `AsideHubSvc` 注册；`CodingRuntime` 仍是唯一生命周期所有者。
- **不新增 kernel/coding 协议面。** 不加 `AgentCommand`、不加 `DriverCommand`；Phase B 的 `HostCommand::Recap` 是收窄（委托），不是新增。
- **不新增第二压缩状态机、不新增回合末补丁。** 调度行只在 `StepEnd` / `TurnEnd` 这两个已经提交的事实上做计数，不参与 `TurnFinishing` 的等待，也不改变 `StopReason`。
- **事实白名单是硬边界。** `AsideOutput::Facts` 只接受 `Titled` 与 `Injected{Reminder}`；其他变体（尤其 `AssistantMessage`、`Compacted`、`TurnEnd`）一律拒绝并记一条 `tracing::warn`，不提交。
- **失败一律等价于 `Silent`：** 模型槽缺失、超时、空答案、`interpret` 判为无内容、generation/session/turn 已过期——不显示、不提交、不报错气泡、不影响回合终态与后续提交。
- **并发：** 每个 `(aside, session)` 一个 in-flight；新回合、切会话、行卸载都使在途结果作废。不排队、不重试。
- **计数状态随行卸载清空**（`Context::effect` 的 undo），不在会话之间泄漏。

## 6. 测试

**单测（harness）**
- cadence 边界：`EverySteps(4)` 在 step 0/4/8 触发、3/5 不触发；`TurnEnd` 同回合只触发一次；非 root 会话不触发。
- `prompt` 返回 `None` 时**不发起请求**（用 scripted provider 断言请求数为 0），这是与旧 recap「先问再判 `<none>`」的差异点。
- in-flight 去重：同一 aside 同一会话在途时第二次触发被跳过，请求数仍为 1。
- `interpret` 返回 `Silent` 时不提交任何事实。
- 白名单：`Facts(vec![AssistantMessage…])` 被拒绝，日志长度不变。
- 资源：`resources::asides()` 每个 id 都能反序列化成合法 `AsideRow`；`catalog()` 里存在对应 `aside-<id>` 行。

**集成（coding 装配）**
- 装 `aside-recap`（`Manual`）后触发一次，恰好 +1 次模型请求，prompt 与旧 recap 逐字相同，日志长度不变。
- 切到 `TurnEnd` 后，现有 `the_recap_is_written_by_the_conversation_model` 不改断言通过；三个清洗单测通过。
- 装 `aside-todo-steward` 后，一个 12 步回合恰好询问 3 次（n=4），日志只多出对应的 `Injected{Reminder}`，`reduce_todos` 折出的列表不变（提案不改权威）。
- 无 `AsideNoteSinkSvc` 时 `Note` 被丢弃，不 panic、不写日志。

**传输**
- `HostEvent::AsideNote` 序列化往返；TUI `AsideBlock` 的续行对齐与 `RecapBlock` 一致。
- 迟到结果：会话切换后回来的 `Note` 不得贴到新会话（ticket 判据）。

**门**
- `cargo nextest run -p atomcode-harness -p atomcode-coding -p atomcode-cli`；
- `bash gates/compile.sh`；
- `cargo fmt --all -- --check`。

## 7. 实施顺序

1. **Phase A（原语）**：`Aside` trait + `AsideHubSvc` + `aside-runner` + `AsideNoteSinkSvc` + `resources::asides()` + `aside-recap`（`Manual`，默认 disabled）+ 单测。此阶段不碰 recap 现有实现。
2. **Phase B（recap 迁移）**：资源化 prompt 与清洗，`aside-recap` 改 `TurnEnd`；TUI 保留判定、触发改为「请求 run」；`HostCommand::Recap` 委托一个版本。用旧测试做验收。
3. **Phase C（两个真实用途）**：`aside-todo-steward`（提案路径）与 `aside-context-hint`（仅 `Note`，读 `Usage` 的压力，永远不触发压缩）。
4. **Phase D（可选、另立 ADR）**：`SessionEvent::TodosWritten` + 折叠加宽，让 todo 真正由旁路写入。没有这份 ADR 就不做。
5. **压缩**：只做「摘要复用请求实现」，不做旁路触发。

Failure semantics: 旁路请求在任何一步失败——没有模型槽、`prompt` 返回 `None`、超时、空答案、清洗后为空、白名单拒绝、generation/session/turn 过期、宿主没有 sink——都收敛为「这次旁路没有发生」：屏幕不显示、日志不增长、请求不阻塞、回合终态不变。原语没有任何一条路径能让主流程等它，也没有任何一条路径能替主流程做决定。
