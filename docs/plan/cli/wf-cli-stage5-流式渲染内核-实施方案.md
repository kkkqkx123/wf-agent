# wf-cli Stage 5 实施方案：流式渲染内核（通用 F5/F6）

> 状态：已完成（阶段 5A–5E，2026-08-18）；遗留改进（表格 holdback / 换行门控 / 引用链接回退 / 定稿兜底重渲）排期 Stage 6，见 §七
> 上游方案：`docs/plan/cli/wf-cli-分阶段实现方案.md`（Stage 5 任务定义）、`docs/cli/05-opencode-mini模式与无头模式设计.md` §3.2/§3.3/§3.4（streaming markdown / footer.append 微任务合并 / 领域事件→UI commit 映射）、`docs/cli/03-组件设计方案.md` §2.3（超限保护）、§3.2（帧调度合并）、§七（HistoryLine/滚动区）
> 范围：三形态共享的"事件 → 可视内容"数据管线——`reducer.rs`（`Vec<UnifiedEvent>` → `MiniCommit[] + FooterState` 纯函数归约）、`markdown.rs`（pulldown-cmark 增量 top-level block 提交的 streaming markdown）、`render.rs`（`HeadlessRenderer`：reducer + markdown 组合出的无头摘要渲染器，验证数据管线独立于 UI）。**本阶段不引入任何 UI 消费方**（mini footer / TUI 屏幕分别落到 Stage 6/7）。

## 一、现状与缺口

Stage 4 已交付公共 UI 组件层（scrollback/select/keymap/framer/ansi/size，ratatui 0.30 引入点）。Stage 5 需要把 Stage 1 的 `UnifiedEvent` 事件流变成**各形态可直接消费的可视内容**。缺口：

| # | 缺口 | 说明 |
| :- | :--- | :--- |
| G1 | 无事件归约层 | `wf run` 的 `run.rs::SessionRenderer` 是 run 形态私有的渲染逻辑（DeltaBuffer 行级缓冲），mini/TUI 无法复用；缺少"事件 → 提交列表 + footer 状态"的纯函数归约（05 §3.3 `session-data reducer` 对应物） |
| G2 | 无 streaming markdown | mini 的 assistant 文本需要按 top-level block（段落/列表/代码块）增量提交，未完结 block 不固化（05 §3.2 `MarkdownRenderable streaming:true` 对应物）；`pulldown-cmark` 依赖尚未引入 |
| G3 | 无无头摘要渲染器 | 方案要求"reducer + markdown 组合出无头摘要渲染器（只输出文本行）"，验证数据管线独立于 UI；现有 `run.rs` 文本路径与 mini 滚动区产物**不同源** |
| G4 | 事件批次合并/去重无承载 | 连续 `LlmDelta` 需帧内批量合并（05 §3.3 footer.append 微任务语义）、重复事件需幂等去重；目前没有统一实现 |

## 二、外部最佳实践参考（context7 检索结论）

1. **pulldown-cmark（/pulldown-cmark/pulldown-cmark）**：
   - 它是**拉式解析器**（pull parser）：块结构按行增量处理，"each line is processed to alter the document's block structure"——天然支持追加式输入；
   - `Parser::new_offset_iter()` / `into_offset_iter()` 产出 `(Event, Range<usize>)`，每个事件携带**源文档字节范围**——这正是"确定 top-level block 边界 → 计算 committed/streaming 区域"的抓手；
   - CommonMark 规定**未闭合的围栏代码块延伸到文档末尾**（unclosed code block extends to end of document），且关闭围栏必须与开启围栏同字符、长度不小于开启围栏——因此流式代码块可凭"最后一行是否闭合围栏"判定 settle；
   - 段落/标题等块的 `End` 事件在**缓冲末尾也会触发**（EOF 自动闭合），不能仅靠 `End` 判定提交——需要"末尾空行 / 闭合围栏"补充判定流式块是否已完结（见 D2）。
2. **ratatui（Stage 4 既有结论）**：`HistoryLine::display_lines(width)` 持有源文本按宽度重算（红线 9）；本阶段 markdown 产物只产 `Text`（源文本），宽度重算完全交给 Stage 4 的 HistoryLine，不引入宽度缓存。
3. **仓内既有范式**：`reducer` 对齐 Stage 4 `framer.rs` 的"可注入时钟/纯数据"、`run.rs` 的"可注入缓冲"——reducer/markdown 全部为**无 IO 纯函数**，可注入 `Vec<UnifiedEvent>` 做确定性单测；无 TTY、无文件副作用。

## 三、关键设计决策

| # | 决策 | 理由 |
| :- | :--- | :--- |
| D1 | **`reducer.rs` 为两层**：① 无状态纯函数 `fold(events, execution_id) -> (Vec<MiniCommit>, FooterState)`（供测试/重放）；② 有状态 `SessionReducer`（`new(execution_id)` + `push_batch(&[UnifiedEvent]) -> Vec<MiniCommit>`），`fold` 内部即驱动一次 `SessionReducer`。**同一归约内核**，流式与批量等价 | 05 §3.3 纯函数 reducer；既有 `run.rs` 的"可注入"范式；`fold` 保 证"合成事件序列 → commit 序列"可快照断言 |
| D2 | **`MiniCommit` 携带分组键 `CommitGroup { execution_id, iteration, tool_call_id }`**（`tool_call_id` 为 `Option`），并设 **`ids` 幂等去重**：`push_batch` 内对连续重复事件（同组同类的 `ToolStart/ToolEnd/IterationBoundary`）去重；连续 `LlmDelta` 合并为**一个** `AssistantText` commit（帧内批量，对齐 05 §3.3 footer.append 微任务合并） | commit 分组键 = `execution_id + iteration + tool_call_id`（05 §3.3）；重放/乱序/重复投递不产生重复提交 |
| D3 | **`FooterState` 纯数据**：`phase(Idle/Streaming)` + `iteration` + `active_tools` + `message_count` + `last_error: Option<String>`，由归约过程增量维护 | 05 §3.4 状态栏/阶段图标驱动；footer 渲染（Stage 6）只消费纯数据 |
| D4 | **`markdown.rs` 追加式源文本 + 全量重解析**：`MarkdownStream` 持有累计 `buffer`，每次 `push(delta)` 用 `into_offset_iter` 重解析，取**最后一个 top-level block 的起始字节**为 committed/streaming 分界（前面全部 committed）；**末尾判定**：`buffer` 以空行结尾（`ends_with_blank_line`）或末行是闭合围栏（`last_line_is_fence`）→ 最后一块也提交（`streaming = ""`），否则最后一块为流式块 | pulldown-cmark 是拉式解析器，全量重解析成本低（assistant 文本每帧仅 KB 级）；"未完结 block 不固化"依赖 05 §3.2；redo 语义确定、可快照 |
| D5 | **流式代码块**：未闭合的围栏代码块在 `streaming` 区域逐行显现（不固化），settle（闭合围栏出现 / 空行结束 / `finish()`）后一次性并入 committed；`MarkdownFrame` 提供 `code_lang: Option<&str>` 供后续 Stage 6/7 语法高亮（P2 syntect 在此接缝接入）；**超限保护**：`max_source_bytes`（默认 64KiB）超出后强制截断并强制提交，防止长输出拖垮每帧重解析 | 对齐 03 §2.3 超限保护（512KB/万行/4KiB 单行）+ 05 §3.2 CodeRenderable settle 语义 |
| D6 | **`render.rs::HeadlessRenderer`（无头摘要渲染器）**：组合 `SessionReducer` + `MarkdownStream`，把 `UnifiedEvent` 变为 `HeadlessDelta { stdout: String, diag: Vec<String>, had_output: bool }`——assistant 文本走 markdown 提交（`committed` 渲染为纯文本、流式块只发**完整行**，软换行保留 `\n`），工具生命周期行进 `diag`（stderr），迭代边界触发 flush。**与 mini 滚动区同源**：同一 `SessionReducer` + `MarkdownStream` 产物；本阶段以 `HeadlessRenderer` + 同源测试验证，Stage 6 的 mini 直接消费相同 commit 流 | 方案"lib.rs：reducer + markdown 组合出无头摘要渲染器"；05 §5.2 行粒度流式；三形态一致性（方案 §七"无头与 TUI 同源"） |
| D7 | **`wf run` 实时路径不改动**（保留 Stage 2 `run.rs::SessionRenderer` 的 DeltaBuffer 行级缓冲）；`HeadlessRenderer` 作为公共内核交付，由 `render.rs` 导出并单测/集成测验证 | Stage 2 已验收且无 UI 依赖；Stage 5 目标是**数据管线本身**（独立于 UI），避免破坏既有 8 项 mock e2e；headless 采用行粒度（05 §5.2）本就是正确语义 |
| D8 | 新增 workspace 依赖 `pulldown-cmark`（对齐 AGENTS.md"依赖集中在根 Cargo.toml"）；`markdown.rs` 渲染产物为**源文本 `String`**（不产样式），样式映射留 Stage 6/7（HistoryLine `Role`） | 组件纯数据化红线；宽度重算归 Stage 4 HistoryLine |

## 四、模块落点

```
crates/wf-cli/src/
├── reducer.rs    ← 新增：CommitGroup / MiniCommit / FooterState / Phase / SessionReducer / fold()
├── markdown.rs   ← 新增：MarkdownStream + MarkdownFrame{committed, streaming, code_lang} + 末尾空行/围栏判定 + render_plain_text()
├── render.rs     ← 新增：HeadlessRenderer（reducer+markdown 组合，产出 HeadlessDelta{stdout,diag,had_output}）
├── lib.rs        ← 接线：pub mod reducer/markdown/render + pub use 核心类型
Cargo.toml（root）     ← workspace.dependencies 新增 pulldown-cmark
crates/wf-cli/Cargo.toml ← 依赖 pulldown-cmark
```

## 五、分阶段任务与验收

### 阶段 5A：依赖引入与 reducer 骨架（G1/G4 基础）

- [x] root `Cargo.toml` 新增 `pulldown-cmark`；wf-cli 增依赖。
- [x] `reducer.rs`：`CommitGroup{execution_id,iteration,tool_call_id}`、`MiniCommit` 枚举（`User` / `AssistantText` / `ToolStart` / `ToolEnd` / `IterationBoundary` / `Completed` / `Failed` / `Interrupted`）、`Phase{Idle,Streaming}`、`FooterState`。
- [x] `SessionReducer`：`new(execution_id)`、`push_batch(&[UnifiedEvent]) -> Vec<MiniCommit>`（连续 `LlmDelta` 合并、重复事件去重、group 键派生、`FooterState` 增量维护）；纯函数 `fold(&[UnifiedEvent], execution_id) -> (Vec<MiniCommit>, FooterState)`（内部驱动一次 SessionReducer）。

**验收**：`cargo check -p wf-cli` 通过；`fold` 对合成事件序列（含乱序/重复 part）返回确定 commit 序列。

### 阶段 5B：reducer 归约语义（F5 主体）

- [x] 事件映射对齐 05 §3.4：`IterationStarted` → `iteration++` + footer；`TextDelta` → 帧内合并为 `AssistantText`（跨 batch 按 iteration 累积）；`ToolStart/ToolEnd` → 同组 `tool_call_id` 配对、`active_tools` 增减、`ToolEnd{success,duration_ms}`；`Completed` → footer 回落 Idle + `Completed{iterations}`；`Failed/Interrupted` → `last_error` + commit。
- [x] `ids` 幂等去重：同 batch 内连续重复的 `ToolStart/ToolEnd/IterationBoundary`（同 group + 同内容）跳过；`TextDelta` 空串跳过。
- [x] 单测：合成事件序列（正常流 / 乱序 / 重复投递 / 空 delta / 工具配对）→ commit 序列快照（内存断言，无文件副作用）；`FooterState` 在各阶段的值断言（iteration/active_tools/phase/error）。

**验收**：reducer 纯函数单测全绿（≥12 例）；commit 序列快照稳定。

### 阶段 5C：markdown.rs streaming markdown（F6 主体）

- [x] `MarkdownStream`：`push(&str) -> MarkdownFrame`、`finish() -> MarkdownFrame`；`MarkdownFrame{ committed: String, streaming: String, code_lang: Option<String> }`。
- [x] 块边界：`into_offset_iter` 求最后一个 top-level block 起始字节；`ends_with_blank_line` + `last_line_is_fence` 判定末块完结；committed = 已完结块源文本、streaming = 在途块源文本。
- [x] 流式代码块：`code_lang` 从开启围栏信息语言提取；未闭合围栏留在 streaming；settle 后并入 committed。
- [x] `render_plain_text(&str) -> String`：pulldown 渲染为纯文本（行内标记剥离、软换行 → `\n`、代码块原样行），供无头与 mini 同源输出。
- [x] 超限保护：`max_source_bytes`（默认 64KiB）超限 → 强制截断并提交（不 panic）。
- [x] 单测/快照：增量提交（段落未完结不出现、空行后固化）；列表/标题/代码块；未闭合围栏逐行显现 → settle 后固化；软换行保留；超限截断。

**验收**：markdown 增量提交快照全绿（≥10 例）；未完结 block 不固化、完结后固化。

### 阶段 5D：render.rs 无头摘要渲染器（lib.rs 组合 + 同源验证）

- [x] `HeadlessRenderer::new(execution_id)` + `on_event(&UnifiedEvent) -> HeadlessDelta` + `finish() -> HeadlessDelta`；assistant 文本经 markdown（committed 全量 + 流式块完整行）、工具行进 `diag`、`had_output` 汇总。
- [x] `lib.rs` 注册 `pub mod reducer / markdown / render` 并 `pub use` 核心类型。
- [x] **同源测试**：同一合成事件序列分别驱动 ① `HeadlessRenderer`（stdout 文本行）与 ② `SessionReducer`→`MiniCommit`→HistoryLine 文本，断言文本一致——证明"无头文本输出与 mini 滚动区内容同源（同一 reducer 产物）"。
- [x] 端到端冒烟（mock LLM）：`agent_execution::stream` 或 `run_session` 产物喂给 `HeadlessRenderer`，验证真实事件流下的 stdout/diag 分流（作为 Stage 5 的集成测试）。

**验收**：`cargo test -p wf-cli` 全绿（含同源测试）；`HeadlessRenderer` 文本输出与 commit 流一致。

### 阶段 5E：勾选与收尾

- [x] 勾选总方案 Stage 5 任务项；补完成记录（见下）。
- [x] 生成 patch（排除构建产物：target/ 不入 patch；Cargo.lock 不动；新增 docs/plan/cli 方案 + src/reducer.rs + src/markdown.rs + src/render.rs + 根 Cargo.toml / wf-cli Cargo.toml 增量）。

**验收**：本方案全部任务勾选；`cargo test -p wf-cli` 全绿；patch 校验 `grep -c 'target/'` = 0。

## 五·一、与方案的偏差（实施期决策）

| # | 偏差 | 原因 |
| :- | :--- | :--- |
| P1 | `wf run` 实时路径**不重写**：Stage 5 交付 `HeadlessRenderer` 公共内核 + 同源测试，`run.rs` 仍用 Stage 2 DeltaBuffer | Stage 5 范围是数据管线本身；headless 行粒度流式（05 §5.2）本就是既有验收语义；避免破坏 8 项 mock e2e；mini/TUI 从 Stage 6 起消费同一 commit 流 |
| P2 | `MarkdownStream` 采用"追加式源文本 + 全量重解析"，而非纯增量 parse 状态机 | pulldown-cmark 无公开增量事件 API；assistant 文本量级下全量重解析成本可忽略，且块边界/完结判定因此可确定性快照 |
| P3 | 流式块完结判定用"末尾空行 / 末行闭合围栏"启发式，而非事件流精确状态 | pulldown-cmark 在 EOF 也会补发 `End`，无法纯靠事件区分"在途块"与"已完结末尾块"；启发式覆盖 LLM 输出的常规形态，单测固定行为 |
| P4 | `HeadlessRenderer` 位于独立 `render.rs`（方案模块图未列名，归入 lib.rs 组合） | 组合器逻辑独立可测；lib.rs 保持"声明 + 组合 + 导出"薄层 |

## 六、风险与边界

| 风险 | 缓解 |
| :--- | :--- |
| 每帧全量重解析的性能 | `max_source_bytes` 64KiB 超限保护 + 截断；assistant 单帧 KB 级，重解析 <1ms；万级事件性能基准留 Stage 8 |
| 末尾块完结判定误判 | `ends_with_blank_line` / `last_line_is_fence` 用单测固定常见形态（段落/标题/列表/围栏代码块/纯文本流）；异常形态最坏表现为"多等一个 delta 才固化"，不会丢内容 |
| 软换行语义 | `render_plain_text` 软换行保留 `\n`，保证无头与 mini 一致、且与既有 DeltaBuffer 行为兼容 |
| 与 Stage 4 HistoryLine 衔接 | markdown 只产源文本；宽度 reflow / 样式 / Role 全部由 Stage 4 `HistoryLine` + Stage 6/7 承担（组件纯数据化红线） |
| 语法高亮（P2 syntect） | `MarkdownFrame.code_lang` 预留语言标识接缝；超限保护与 03 §2.3 对齐 |

## 七、已知改进项（codex 对照分析定位，排期 Stage 6）

对照 `docs/analysis/` 下 codex 流式渲染分析（`ai-output.md`）与 `wf-agent-learnings.md` §4，本阶段交付的 `MarkdownStream` 存在四项正确性/体验缺口，mini（Stage 6）是第一个 UI 消费方，**排期 Stage 6B 一并补齐**（`wf-cli-stage6-mini模式-实施方案.md` D14）：

| # | 改进项 | 缺口 | codex 参照（ai-output.md） |
| :- | :--- | :--- | :--- |
| I1 | **表格 holdback** | `boundary()`（markdown.rs:152）只有顶层块边界 + fence/空行沉降启发式，无表格检测——表头行会被提前提交，流式表格列宽变化产生错位/闪烁 | §4.2/§5.1 `TableHoldbackScanner` + `FenceTracker` |
| I2 | **换行门控** | 沉降启发式未严格"未以换行结尾不提交"——半行可能被提交（如段落最后一行不完整时按块边界提前提交） | §2.2/§2.5 换行门控硬约束 |
| I3 | **引用式链接定义全量回退** | 检测到 `[ref]: url` 定义后未全量重渲（此类结构可回溯影响任意块） | §4.1 recompute 全量回退 |
| I4 | **定稿兜底重渲测试** | `finish()` 全量 commit 但无"完整源码渲染 vs 增量提交"比对兜底；缺 `assert_streamed_equals_full` 语义测试锁定"流式过程 == 完整结果" | §2.5/§5.2 `assert_streamed_equals_full` |

**状态（2026-08-22）**：I1/I2/I3 已随 stage6 6B 于 `cbffeb8` 落地（`markdown.rs` `boundary()` 三 gate：`unclosed_table_start`/`rfind('\n')` 门控/`has_reference_definition`，均带单测）；I4 已随 stage6 6B 补缺落地（`MarkdownStream::final_plain_text()` 定稿兜底 + `assert_streamed_equals_full` 测试：LCG 随机 char 边界切分 × 表格/fence/引用链接/空行/混合文档 × 8 seeds，断言"提交+流式前缀渲染 == 全量渲染前缀"与"终态等价"）。

另有 P1 项：**两档提交动画**（`AdaptiveChunkingPolicy` 的 Smooth/CatchUp 带滞回分块策略，ai-output.md §4.4）作为独立纯逻辑对象（可注入队列深度/队龄单测），随 mini 流式呈现层落地评估。
