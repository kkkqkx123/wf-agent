# Recap 功能实现分析与扩展设计方案

**Goal:** 让 daemon / webui / 手机端也具备 TUI 已有的 recap（回合结束后的一行「现状回顾」），并补上手动触发入口与配置可见性；不改变「只为屏幕、不入会话日志」的边界。
**Architecture:** `CodingRuntime` 仍是唯一 owner。recap 继续由 `CodingRuntimeHandle::recap()` 经 `SideCallProvider` 发起一次无工具旁路调用，daemon 只新增一个读取路由与一个 handle 适配方法，不新建 runtime、不改生命周期。「什么时候值得回顾」仍属视图判定，由各前端按自己的可见性信号实现。
**Tech Stack:** Rust（axum / tokio）、Preact + TypeScript（webui）、cargo nextest。

Baseline: `main` @ `002b7b1c3`（2026-10-22）。本轮只产出文档；实施按下面的阶段分别提交，每阶段独立跑门。recap 的现有实现是稳定代码，扩展只做加法。

> 后续：把旁路请求本身通用化（recap 退化为其中一个特殊实现）的方案见 [2026-10-27-aside-request-and-resources-design.md](2026-10-27-aside-request-and-resources-design.md)。本文的传输层结论（daemon/webui/手动命令）不受影响。

## 1. 现状：recap 是什么

recap 是回合结束后追加在回答下方的一行暗色注记，形如：

```
※ recap: 正在改 /live 路由的鉴权，两处测试已过；下一步等你确认要不要顺手补 e2e。
```

三条硬约束（后续所有扩展都必须保持）：

1. **只为屏幕**：不写入会话日志，不进入 `SessionSnapshot`，resume 重放时不会出现。
2. **模型不读回**：它不进入下一次请求的 conversation，只是给人看的一句话。
3. **旁路调用**：一次无状态、无工具、有超时的模型调用，失败即静默，绝不阻塞或改变回合终态。

## 2. 现有实现链路

### 2.1 触发判定（TUI 视图逻辑）

`crates/atomcode-tui/src/plugin.rs`：

```rust
const RECAP_AFTER: Duration = Duration::from_secs(120);

fn wants_recap(ran: Option<Duration>, away: bool, reply_rows: usize, screen: u16) -> bool {
    away || ran.is_some_and(|ran| ran >= RECAP_AFTER) || reply_rows > screen as usize
}
```

`maybe_recap()` 的完整门槛：

- `ui.recap` 关闭 → 不做（`moment.recap_on()`，无配置行时默认 true）；
- goal / loop 自主运行（`m.autonomy.is_some()`）→ 不做：连续回合每轮一次付费调用，几乎没人读；
- 当前会话不是 root（子 agent / 成员）→ 不做；
- `wants_recap(ran, away, rows, screen)` 为假 → 不做。

三条触发信号的含义：`away` 是本回合中终端失去过焦点（`Input::Focus(false)` 且 `turn_open`）；`ran` 是本回合时长；`reply_rows` 是把回复按终端宽度折行后的行数，超过一屏即算「说了很多」。

只在 `AgentEvent::TurnComplete { reason: StopReason::Stopped }`（模型自己正常结束）时触发；取消、报错、超限等终态不触发。

生命周期上的丢弃：`TurnStarted`、新提交、切会话、切屏幕都会 `drop_recap()`（ticket 自增 + `task.abort()`），避免迟到的 recap 贴在别的回合下面。

### 2.2 命令通道与旁路调用

```
TUI maybe_recap
  └─ HostCommand::Recap { session }                 (atomcode-host-api)
       └─ HostControl (atomcode-cli/src/host.rs)
            └─ CodingRuntimeHandle::recap()         (atomcode-coding/src/runtime.rs)
                 ├─ Control::SideCallProvider { generation, done }   // oneshot 回执
                 │    └─ side_call_provider(resources)              // 当前会话模型，非 llm-utility
                 ├─ self.snapshot()                                 // Snapshot 命令
                 └─ recap::generate_recap(provider, &snapshot.messages)
                      ├─ recent_stable_transcript(messages)         // JSON Lines 投影 + 脱敏 + 16K 上限
                      ├─ sample_side_call(provider, prompt, 25s, "recap")
                      └─ sanitize_recap(&sample.raw)                // ≤240 字符、单行
            └─ HostReply::Recap { text: Option<String> }
```

要点：

- `SideCallProvider` 带 generation 校验；停机路径 fail-closed 返回 `Err`，不会落到空 provider。
- `side_call_provider` 优先取 harness 当前 provider，否则用 factory 现建；注释明确「绝不用 side-call 模型（`llm-utility`）」——recap 要读的是本会话自己的账本。
- `sample_side_call`：`reasoning_effort: Low`、`temperature: 0.2`、无工具、无 `max_tokens`、25s 超时；只收 `TextDelta`。
- `recent_stable_transcript` 要求最后一条是稳定 assistant（无 `tool_calls`、非空），从尾部按 group 回填到 `MAX_CONTEXT_CHARS = 16_384`，必须含 user；tool arguments 经 `project_tool_arguments` 脱敏（api_key/token/password → `<redacted>`，content/command/patch → `<omitted N bytes>`，path/url 保留并截断）。

### 2.3 输出清洗

`crates/atomcode-coding/src/recap.rs`：

- 去掉 ``` 围栏与 `*`；
- 去掉模型自带的 `recap:` / `回顾:` 前缀；
- `<none>` → `None`（没有值得说的就不显示）；
- 折成一行、去掉包裹引号；
- 超过 `MAX_RECAP_CHARS = 240` 按句末截断，否则补 `…`。

### 2.4 展示

`Host::show_recap` → `stream.writer("recap").emit(RecapBlock(text))`。`RecapBlock` 的 `kind()` 为 `"recap"`，`always_open()` 为 true，渲染为 `※ recap:` 前缀 + muted 样式按宽度折行，续行对齐前缀（`crates/atomcode-tui/src/content.rs`）。文案来自 i18n：`Msg::RecapLabel` → `※ recap:` / `※ 回顾:`。

### 2.5 竞态

`recaps: Arc<AtomicU64>` 作为 ticket，`recap_task: Mutex<Option<JoinHandle<()>>>` 保存在途任务。结果回来时 ticket 已变则丢。这是「迟到的旁路结果不能贴到新回合」的唯一机制，扩展端必须等价的做。

### 2.6 配置与测试

- 配置：`UiConfig.recap`（`config.toml` 的 `[ui] recap`，默认 true），设置目录 id `ui.recap`，`ApplyPolicy::ImmediateUi`，`/config` 面板可改。
- 现有测试：
  - `crates/atomcode-coding/src/recap.rs`：`a_recap_is_one_clean_line`、`nothing_to_say_is_no_recap`、`a_long_one_is_cut_at_a_sentence_end`；
  - `crates/atomcode-coding/tests/runtime_criteria.rs::the_recap_is_written_by_the_conversation_model`：断言请求数 +1、prompt 含 "Write a recap"、未新建 provider；
  - `crates/atomcode-tui/src/plugin.rs` 的 `recap_tests`（`wants_recap` 三信号）、`content.rs` 的 `recap_block_tests`；
  - `crates/atomcode-tui/tests/e2e.rs::a_long_answer_ends_with_a_recap_and_a_short_one_does_not`（`RecapHost` 拦截 `HostCommand::Recap` 直接给固定文本）；
  - `atomcode-host-api` 的 `HostCommand::Recap` / `HostReply::Recap` 序列化往返。

## 3. 缺口清单

| 入口 | 现状 | 缺口 |
| --- | --- | --- |
| TUI | 完整：自动触发 + 渲染 + 配置 + 竞态 | 无手动触发 |
| daemon | 无任何 recap 路由 | `POST /live/recap` 缺失；`LiveViewHub` 无对外取 handle 的方法 |
| webui | 无 recap | 无请求、无渲染、无触发判定、`/config` 不暴露 `ui.recap` |
| 手机端 / 远程 | 走同一 `/live` 通道 | 同 webui |
| ACP | 无 | 见 §5.5：**不补**（理由） |

关键点：`crates/atomcode-coding` 已经提供了 `CodingRuntimeHandle::recap()`、`HostCommand::Recap`、`HostReply::Recap`，**扩展不需要改动 coding 的旁路调用、提示词、清洗逻辑，也不需要新增 `DriverCommand`**。缺的只是传输与视图层。

## 4. 设计约束（扩展不得破坏）

- 不新增第二 runtime owner：daemon 只通过 `LiveViewHub` 已有的 `bound_handle()`（含 generation 校验）取 `CodingRuntimeHandle`，与 `cancel` / `respond` 同一条路径。
- 不把「值不值得回顾」下沉到 kernel 或 coding：这是每个前端各自的屏幕信号（终端焦点 / 页面可见性 / 行数）。
- recap 仍然不写日志、不进快照、不广播给模型；前端拿到的文本是**临时视图状态**，切会话、重载、fork 后必须消失。
- 失败语义统一：超时、provider 不可用、`<none>` 一律不显示，不回错误气泡，不影响回合终态。

## 5. 扩展方案

### 5.1 Phase 1（P0）daemon 新增 `POST /live/recap`

**Hub 侧**：在 `crates/atomcode-daemon/src/live_hub.rs` 的 `impl LiveViewHub` 里加一个与会话生命周期无关的读取方法，复用私有的 `bound_handle()`：

```rust
/// 用当前绑定的 runtime 生成一次 recap。屏幕专属：不发布事件、不写日志。
pub async fn recap(&self) -> Result<Option<String>, HubError> {
    let (_, handle) = self.bound_handle()?;      // generation / snapshot_error 已校验
    handle.recap().await.map_err(|e| HubError::RuntimeRejected(e.to_string()))
}
```

不放 `state` 锁跨 await：`bound_handle()` 返回后立即释放锁，与 `cancel()` / `respond()` 的写法一致。

**native_live 侧**：加薄封装（`hub()` 是模块私有的静态）：

```rust
pub async fn recap() -> Result<Option<String>, HubError> {
    hub().recap().await
}
```

**路由与 handler**：`crates/atomcode-daemon/src/live_api.rs`，参照 `live_compact` 的极简风格，但返回正文而不是 `accepted`——recap 是同步读取，请求方需要这句话本身：

```rust
/// POST /live/recap —— 为共享实时运行时生成一次「屏幕回顾」。
/// 仅用于显示：不写会话日志、不进快照。无绑定 runtime 或生成失败时 ok=false。
pub(crate) async fn live_recap(State(_state): State<AppState>) -> impl IntoResponse {
    match crate::native_live::recap().await {
        Ok(Some(text)) => Json(serde_json::json!({ "ok": true, "text": text })),
        Ok(None) => Json(serde_json::json!({ "ok": true, "text": null })),
        Err(error) => Json(serde_json::json!({ "ok": false, "error": error.to_string() })),
    }
}
```

`crates/atomcode-daemon/src/lib.rs` 受保护路由表加一行（紧邻 `/live/compact`）：

```rust
.route("/live/recap", post(live_api::live_recap))
```

**契约细节：**

- 响应恒为 200，用 `ok` 区分「没有可回顾的内容」（`ok:true, text:null`）与「没有 runtime / provider 不可用」（`ok:false`）。HTTP 层不把 `Unbound` 当 5xx，否则前端难以区分「这台 daemon 没绑共享运行时」和「网络坏了」。
- **不额外加超时**：`sample_side_call` 内部 25s 超时就是上界；HTTP 层再加一层更短的超时会切断一次本可成功的调用。
- **不按 `turn_in_progress` 拒绝**：recap 只是读当前快照，允许在任意时刻请求；代价由 25s 超时与前端「每回合最多一次」共同约束。前端在收到新回合的 `state.running=true` 时主动放弃在途请求（见 5.2），这比服务端拒绝更准确，因为服务端不知道请求方的屏幕状态。
- 内嵌 TUI（`/webui`）场景下 `bound_handle()` 同样可用；TUI 自己的自动 recap 继续走 `HostCommand::Recap`，两条入口互不影响，也不会重复触发（TUI 的判定在本地）。

**不做广播**：新增 `LiveViewEvent::Recap` → `LiveWireEvent::Recap` 需要动 hub 的事件序列、replay 缓冲与 `to_wire` 映射，收益是「多标签页共享同一行」。但「离开过」本来就是每个视图各自的信号，同一条 recap 贴到另一个一直盯着屏幕的标签页上是噪声。V1 采用**请求-响应**：谁问谁显示。若产品后续要求共享，再按 §5.6 的可选方案补事件。

### 5.2 Phase 2（P0）webui 请求、判定与渲染

**api.ts**：与 `postLiveCompact` 同型，新增

```ts
/** 生成一次屏幕回顾。无内容或失败都返回 null —— 调用方直接不显示。 */
export async function postLiveRecap(signal?: AbortSignal): Promise<string | null> {
  const resp = await apiFetch('/live/recap', { method: 'POST', headers: authHeaders(), signal });
  if (!resp.ok) return null;
  const body = await resp.json() as { ok?: boolean; text?: string | null };
  return body.ok === true && typeof body.text === 'string' && body.text ? body.text : null;
}
```

**触发判定**（`webui/src/components/Chat.tsx`）：与 TUI 的三信号同构，只用浏览器能拿到的信号。

- 回合开始（提交成功 / 收到 `state.running=true`）时记录 `turnBeganRef = Date.now()`，`awayThisTurnRef = document.visibilityState === 'hidden'`，并注册 `visibilitychange`：`hidden` ⇒ `awayThisTurnRef = true`。
- `case 'done'` 且 `event.stop_reason` 为空或 `'stopped'`（正常自结束）且非 goal/loop 自主回合 ⇒ 计算：
  - `elapsed >= 120_000`，或
  - `awayThisTurnRef`，或
  - 最后一条 assistant 消息容器 `scrollHeight > clientHeight`（「超过一屏」）。
- 三者皆假则不请求。`ui.recap` 为 false 时不请求（见 5.3）。
- 每个回合一个 `recapTicketRef`（自增）：请求返回时 ticket 已变则丢弃。切会话、fetch 历史、新建会话、`state.running=true` 都自增 ticket 并 `AbortController.abort()`，与 TUI 的 `drop_recap()` 等价。
- 只在 sync 模式（有 `/live` 绑定）启用；非 sync 的 `/chat` 流没有共享 runtime，`/live/recap` 会返回 `ok:false`，此时静默跳过。

**渲染**：新增一个**临时**消息项（例如 `{ kind:'recap', text }`），样式复用现有的 muted/系统提示，前缀 `※ recap:`（文案键复用 `ui.recap` 相关 i18n，webui 侧新增 `chat.recapLabel`）。它只存在于前端 state：

- 不进 `messages` 的历史来源、不发 `appendSessionMessages`、不参与滚动锚点的历史计算；
- 会话切换 / 重新加载 / fork / 页面刷新即消失（与 TUI 的「resume 不显示」一致）。

### 5.3 Phase 3（P1）`ui.recap` 在 webui 可见

现状：`/config` 只回 provider / notifications 等字段，`ConfigInfo` 与 `api_config::get_config` 都没有 `ui.*`；daemon 也没有 settings 写入口。

- **读取（必做）**：`api_config::get_config` 增加 `ui: { recap: config.ui.recap }`；`ConfigInfo` 增加 `ui?: { recap: boolean }`，前端缺省按 true 处理（与 `moment.recap_on()` 的默认一致，兼容旧 daemon）。
- **写入（可选，另立评审）**：若需要在 webui 里给开关，新增 `PATCH /config/setting`，复用 CLI 的写法（`atomcode_config::settings::SETTINGS` 找 `ui.recap` → `spec.patch(&mut DocumentMut, value)` → 写回文件），不序列化整份配置、保住注释与顺序。这是一条新的写盘面，应独立提交并配套权限/校验测试；V1 允许先只读。

### 5.4 Phase 4（P1）手动触发 `/recap`

自动触发的门槛意味着「短且盯着的回合」永远不会给 recap，但人有时就是想问一句「刚才做到哪了」。

- **TUI**：`SessionCommands`（`crates/atomcode-tui/src/commands.rs`）加 `"recap"` 分支，走与 `/cost` 相同的 `control.call(HostCommand::Recap { session: root })`；拿到 `Some(text)` 后调用 `host.show_recap(text)` 并返回 `Outcome::Quiet`——直接落 `RecapBlock`，与自动路径同一样式。`None` 返回 `Outcome::Quiet`（无声即答案）。命令表在 `Command::said("recap", ...)` 处登记一行说明。
- **webui**：`FRONTEND_COMMANDS`（`webui/src/lib/slashCommands.ts`）加 `recap`，在 `Chat.tsx` 像 `/compact` 一样特判 sync 分支：调用 `postLiveRecap()`，成功则插入上面那个临时 `recap` 项，`null` 则不显示或给一条「没有可回顾的内容」的 notice（文案二选一，建议前者，避免噪声）。
- 手动路径同样受 `ui.recap` 约束吗？**不受**：配置项管的是自动注记；人明确输入的命令照做。这一点要与 TUI 的 `/config` 文案区分开，避免「关了还手动能触发」被当成 bug。

### 5.5 ACP：不补

ACP 的 `SessionNotification` 只有消息内容语义，没有「宿主装饰行」这一档。把 recap 塞成 `agent_message_chunk` 会同时违反 §1 的第 1、2 条：客户端会把它当 assistant 消息写进自己的记录，模型下一轮也可能读到。除非将来 ACP 有 `_meta` 级别的展示通道，否则不实现。

### 5.6 可选：服务端广播（暂不排期）

若确定需要多标签页共享同一条 recap：`LiveViewEvent` 加 `Recap(String)`，`LiveWireEvent` 加 `Recap { text }`，`to_wire` 做映射，`Chat.tsx` 的 `onLiveEvent` 处理；请求方与服务端需要去重（同一回合只广播一次），并且要接受「另一个标签页也收到了一条它没等过的回顾」这一副作用。

### 5.7 非目标

- 不在 resume / 打开历史会话时生成 recap：历史是 authoritative 的事件日志，recap 不落盘，重放时无法复原，也不该临时花一次调用去补写屏幕。
- 不为失败 / 取消 / 超限终态生成 recap：`StopReason::Stopped` 之外的状态，屏幕已经有明确的失败或取消提示，再加一句回顾会稀释它。
- 不改 `MAX_RECAP_CHARS`、`SAMPLE_TIMEOUT`、`INSTRUCTIONS`、脱敏规则：这些是已调过的产品行为，扩展只搬运输。

## 6. 测试与验证

**coding（回归，不改实现）**
- 已有 `the_recap_is_written_by_the_conversation_model` 必须继续通过，证明新入口没有绕开 `SideCallProvider`。

**daemon**
- `live_hub`：无绑定时 `recap()` 返回 `Err(Unbound)`；generation 变化返回 `RuntimeGenerationChanged`；正常路径调用到 `CodingRuntimeHandle::recap()`（用现有 `RealControl` / `FakeControl` 模式）。
- `live_api`：`live_recap` 的三种响应（`text` / `null` / `error`）序列化形状；`/live/recap` 出现在受保护路由表中（鉴权与其他 `/live/*` 一致）。
- 参考现有 `no_console_prints_in_live_path`：`live_path` 内不得用 stdout/stderr 打印。

**webui**
- `postLiveRecap` 对 `ok:false`、非 2xx、空文本都返回 `null`。
- 判定函数抽成纯函数（`shouldRecap({ elapsed, away, overflowsScreen, enabled, autonomous })`）单测，与 `wants_recap` 的用例对齐：短且盯着 → 否；离开 → 是；≥120s → 是；超过一屏 → 是；禁用 → 否。
- 竞态：请求在途时切会话 / 新回合，旧文本不得插入；recap 项不进入历史数组。
- 手动 `/recap`：sync 下插入一项、`null` 下不插入。

**门**
- `cargo nextest run -p atomcode-daemon`、`-p atomcode-coding`、`-p atomcode-tui`；
- `bash gates/compile.sh`；
- `cargo fmt --all -- --check`；
- webui 侧跑现有前端构建/类型检查命令（与 `/compact` 改动同一套）。

## 7. 实施顺序

1. Phase 1 daemon 路由 + hub 方法（可独立合入，先给手机端/脚本一个可调用面）。
2. Phase 2 webui 请求 + 判定 + 渲染（主用户价值）。
3. Phase 3 只读配置；写入口按需另评。
4. Phase 4 手动 `/recap`（TUI 与 webui 各一小段）。
5. Phase 5/6 不做。

Failure semantics: `SideCallProvider` 取不到 provider、25s 超时、`<none>`、`sanitize_recap` 返回 `None`、无绑定 runtime、generation 变化——全部等价为「这一回合没有 recap」，前端不显示任何东西，不产生错误气泡，不写入会话日志，不影响回合终态与后续提交。webui 的在途请求一律可被新回合或会话切换 abort；服务端不因一次 recap 失败改变 runtime 状态。
