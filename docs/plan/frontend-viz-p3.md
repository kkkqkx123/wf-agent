# 前端可视化 P3 阶段细化方案与落地说明

> 依据 `docs/ref/frontend-viz/`（尤其是 `06-可借鉴Dify与n8n的设计.md` 第 6 节优先级清单）编写。
> P3 共四项：**流式 token 可视化**（源自 Dify §1.3）、**实时协作光标**（源自 Dify §2.3）、
> **密度档与设计令牌文档**（源自 Dify+n8n §4.3/§5.2）、**键盘可达性抛光**（§5.3）。
> 本文先给细化方案，再记录实际落地结果（含文件与行号），最后给出全部决策结论。
>
> 本阶段为纯前端改动，未触碰后端 Rust 代码，因此未执行后端编译。
> 后端事实核对见第 1 节；凡后端不支持的能力，前端只做到“诚实上限”，不伪造数据。

---

## 0. 目标与范围

| 编号 | P3 项              | 来源               | 目标收益               | 本次状态                                                                        |
| ---- | ------------------ | ------------------ | ---------------------- | ------------------------------------------------------------------------------- |
| P3-A | 流式 token 可视化  | Dify §1.3          | 执行输出可读性         | 已落地（历史输出 Markdown 化 + Live 输出卡；真逐 token 流需后端先给节点级增量） |
| P3-B | 实时协作光标       | Dify §2.3          | 多人同画布感知         | 已落地（同源多标签页 presence；跨用户需服务端光标通道）                         |
| P3-C | 密度档 + 令牌文档  | Dify+n8n §4.3/§5.2 | 视觉密度可调、多端对齐 | 已落地（`--density-scale` + `docs/design-tokens.md`）                           |
| P3-D | 快捷键 cheat-sheet | n8n §5.3           | 快捷键可发现性         | 已落地（分组 + `?` 开关）                                                       |

范围外：P0/P1/P2 已交付项；后端新增 token 增量端点、服务端光标广播通道，均不在本阶段。

四项之间无依赖，可并行落地；实际按 A→B→C→D 顺序实施。

---

## 1. 事实基线（后端契约与前端现状，逐条带出处）

### 1.1 token 增量是执行全局的，没有节点归属

`crates/app/wf-api/src/infra/stream.rs:37` 的 `ExecutionStreamEvent::LlmDelta { content }` 只有 `content`，
`ReasoningDelta`（`:69`）、`Usage`（`:71-75`）同样无 `node_id`。
前端 `apps/web-app/src/lib/utils/stream-parser.ts:201-211` 的 `handleExecutionFrame` 把
`llm_delta`/`reasoning_delta` 接成全局 `onDelta`/`onReasoning`，同样无节点归属。

执行检查器的实时通道是另一条：`apps/web-app/src/lib/api/sse.ts:46-106` 的
`openEventStream` 走 `GET /api/v1/events/stream`，帧形 `StreamFrame`（`sse.ts:7-16`）
只有 `BaseEvent`（节点生命周期），**没有文本增量**。
`ExecutionInspector.svelte:423-441` 只在 `graph` Tab 且执行活跃时订阅，做 120ms 节流的状态叠加。

结论：不改后端就拿不到“某节点的 token 流”。P3-A 只做诚实上限（见 D1）。

### 1.2 Markdown 渲染器与唯一真流式用法已就绪

`apps/web-app/src/lib/components/chat/StreamMarkdown.svelte`（20 行）包
`markstream-svelte` 的 `MarkdownRender`（`content` + `final` + `isDark`）。
`apps/web-app/package.json` 与 `apps/web-app-preview/package.json` 均有
`"markstream-svelte": "2.0.13"`。

真流式用法只有 chat 页：
`apps/web-app/src/routes/chat/+page.svelte:507-510`
（`content={chatStream.answer} done={!chatStream.active}`，另有 usage 区 `:511-522`）。
其余两处（`MessageBubble.svelte:86`、`checkpoints/+page.svelte:481`）均为 `done` 态复用。

### 1.3 节点追踪输出是 `unknown`，天然分两类

`apps/web-app/src/lib/types/models.ts:53-54` 的 `NodeTrace.input/output` 为 `unknown`，
`NodeTraceCard.svelte:128-143` 统一走 `JsonViewer`。
字符串输出（LLM 文本）被 JSON 引号包裹，可读性差——这正是 P3-A 要修的点。

### 1.4 lease 是 HTTP 轮询互斥锁，WS 无光标消息，前端无 WS 客户端

前端锁三件套：`services/workflow-locks.ts:27-84`（`lock/acquire/heartbeat/release`）、
`stores/workflow-lock.svelte.ts`（`HEARTBEAT 30s / POLL 20s`，404 则 `supported=false` 放行）、
`routes/workflows/[id]/+page.svelte:101-153`（`watch` + 失锁只读提示）。

后端 `crates/app/wf-server/src` 与 `openapi.json` 无 `/lock` 路由；
`crates/app/wf-server/src/ws.rs:20` 写明 `Presence deferred`，
server→client 只有执行/通知类消息，client→server 只有订阅/ping。
`apps/web-app/src` 无任何 `WebSocket` 客户端。

结论：Dify 式“经 WebSocket 广播光标”无服务端载体。P3-B 用 `BroadcastChannel`
做同源 presence（见 D2）。

### 1.5 密度只有字号档，无间距变量

`stores/preferences.svelte.ts:40-44` 的 `DENSITY_SCALE`（0.94/1/1.06）经
`theme.svelte.ts:27-30` 的 `applyFontScale` 写 `--font-scale`；
`apps/ui/src/styles/tokens.css:135-142` 只有六档字号用它，`--radius: 0.5rem`（`:58`）固定。
`app.html:25-28` 首屏内联恢复字号档，`routes/+layout.svelte:28-31` 响应式应用，
`routes/settings/+page.svelte:242-256` 文案称“只缩放字号，不改变布局单位”。

`app.css:7-73` 的 `@theme inline` 把全部令牌映射为 Tailwind 颜色/圆角/字号/间距，
但无 `--brand`（全仓零命中），无间距密度变量。

### 1.6 快捷键帮助已打通，只缺分组与开关

`lib/graph/canvas-shortcuts.ts:10-21` 的 `CANVAS_SHORTCUT_HELP`（10 行）、
`lib/components/layout/HelpModal.svelte:17-25`（全局 3 行 + 画布 10 行扁平渲染）、
`lib/components/layout/AppShell.svelte:36-44`（`F1 → toggleHelp` + `preventDefault`）。
帮助弹窗可用，P3-D 只做抛光。

### 1.7 改动前的问题清单

| 问题                                            | 出处（改动前）                      | 后果                             |
| ----------------------------------------------- | ----------------------------------- | -------------------------------- |
| 节点输出一律 JSON 查看器，LLM 文本带引号难读    | `NodeTraceCard.svelte:128-143`      | Dify 式“流式 token 可视化”无落点 |
| overview 的 Current position 只有节点名，无输出 | `ExecutionInspector.svelte:744-751` | 运行中看不到当前节点在写什么     |
| 画布无远端光标层                                | `GraphCanvas.svelte:971-1098`       | 多人同编无感知                   |
| 密度只缩放字号，间距/圆角不变                   | `tokens.css:58`、settings 文案      | comfortable 档“不够松”           |
| 设计令牌无速查表                                | `docs/design-tokens.md` 不存在      | 多端对齐靠翻源码                 |
| 帮助弹窗 13 行扁平无分组，无键盘开关            | `HelpModal.svelte:54-65`            | 快捷键不可发现                   |

---

## 2. 差距分析（对照 Dify / n8n）

| 能力         | Dify                               | n8n                    | 改动前 wf-agent    | 本次做法                                                                   |
| ------------ | ---------------------------------- | ---------------------- | ------------------ | -------------------------------------------------------------------------- |
| token 流渲染 | `StreamMarkdown` 流式渲染 LLM 输出 | 无对应能力             | 仅 chat 页真流式   | 取 Dify 形态：历史输出 Markdown 化 + Live 输出卡；真增量待后端节点级 delta |
| 协作感知     | 画布协作光标                       | 锁 + 评论              | lease 互斥锁       | 画布叠加远端光标层；传输先用同源 `BroadcastChannel`，留服务端注入缝        |
| 密度         | 轻透留白                           | 紧凑 IDE 风 + 设计令牌 | 三档只缩放字号     | 加 `--density-scale` 联动圆角，文档写清两档语义                            |
| 令牌文档     | `dify-ui` 独立包 + 文档            | design-system 令牌体系 | 令牌完备无速查表   | 补 `docs/design-tokens.md`                                                 |
| 快捷键发现   | 面板内提示                         | cheat-sheet 弹窗       | HelpModal 扁平列表 | 分组 + `?` 开关                                                            |

两处刻意不照抄：Dify 的尝试级子日志树（后端已折叠，P1 已记）、n8n 的品牌橙红锚点
（P2 范畴，不属 P3）。

---

## 3. 决策记录

| #   | 议题                | 决策                                                                | 依据                                                                                 |
| --- | ------------------- | ------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| D1  | 真逐 token 流做不做 | **不做**，只做历史输出 Markdown 化 + Live 输出卡                    | 1.1：增量无节点归属，SSE 无文本帧；伪造“某节点流”等于编数据                          |
| D2  | 协作光标传输层      | **`BroadcastChannel` 同源多标签页**，`presence` prop 留服务端注入缝 | 1.4：WS 无光标消息、无前端客户端、锁端点服务端缺失；BroadcastChannel 是零后端真功能  |
| D3  | 光标坐标系          | **相对画布包裹层的 CSS px**，不经过 cytoscape 世界坐标              | 避开缩放/平移逆变换；hotspot 同为 CSS 叠加（`GraphCanvas.svelte:987-999`），模式一致 |
| D4  | 密度新增变量语义    | 新增 **`--density-scale`**（0.92/1/1.08），只联动 `--radius`        | 字号仍由 `--font-scale` 管，两变量各司其职；全量间距变量化超出 P3                    |
| D5  | 令牌文档放哪里      | **`docs/design-tokens.md`**，与源码同值、手写用途列                 | 06 §4.3 点名此路径；由 `tokens.css` + `app.css` 翻译，不引入生成步骤                 |
| D6  | `?` 开关是否全局    | **全局**（AppShell），输入框内不触发                                | 与 F1 同级；`isTyping` 保护沿用 canvas-shortcuts 既有语义                            |
| D7  | preview 工程        | **不同步改代码**，只同步文档                                        | preview 是旧镜像（P1 D9），同步脚本只镜像代码；`docs/*.md` 不在脚本范围              |

---

## 4. P3-A：流式 token 可视化（诚实上限）

### 4.1 新增 `lib/utils/markdown.ts`

`extractMarkdownText(value: unknown): string | null`：只认字符串；空串、JSON 貌
（去空白后首字符 `{`/`[`）、单行无标点短串返回 `null`；含换行或 Markdown 标记
（`#`/`*`/`` ` ``/`-`/`>`/`[`/序号列表）返回原文。其余一律 `null`，
调用方回落 `JsonViewer`。纯函数，单测覆盖。

### 4.2 `NodeTraceCard` 输出双渲染

Output 区：`extractMarkdownText(trace.output)` 命中 → `StreamMarkdown content done`；
未命中 → 原 `JsonViewer` 不变。Input 区不动（结构化参数）。
LLM 推理步骤（`reasoning.content`）同样走 `StreamMarkdown`，与卡片内
“Reasoning”语义对齐 Dify tracing 面板。

### 4.3 `ExecutionInspector` overview 新增 Live 输出卡

Current position 卡之后新增 `Live output` 卡：

- 数据源：已加载的 `nodeTraces` 中 `nodeId === execution.currentNode` 的首条；
  文本经 `extractMarkdownText` 提取，`StreamMarkdown` 渲染；
- Live 徽标：`isLive` 为真时显示 pulsing `running` 点 + “Live”；
- 跳转：`Open trace` 按钮复用 `openNodeTrace(currentNode)`；
- 加载语义：`isLive` 且 `seenTrace` 未命中时，overview 同样触发一次
  `getExecutionNodeTraces`（复用同一 guard 变量，不新增状态）；
  历史执行且 traces 未加载时显示引导文案，不自动请求。

---

## 5. P3-B：协作光标（同源 presence + 画布叠加层）

### 5.1 新增 `lib/stores/presence.svelte.ts`

`PresenceStore`：`join(workflowId)` 开 `BroadcastChannel('wf-presence-<id>')`，
`move(x, y)` 100ms 节流广播 `{clientId, name, color, x, y, at}`，
`peers` 为 5s 过期的远端表；`leave()` 关闭。`clientId` 随机，`name` 取
`lock ownerName` 语义（本标签页标识），`color` 由 id 哈希取 6 色盘。
无服务端依赖；`BroadcastChannel` 不可用时静默停用（单机零影响）。

### 5.2 `GraphCanvas` 叠加远端光标

新增 `presence?: PresenceCursor[]` + `oncursormove?: (pos) => void`：
包裹层 `onpointermove` 上报相对坐标（调用方节流），
`presence` 渲染为 `pointer-events-none z-20` 的箭头 + 名字旗，
模式与 hotspot 按钮（CSS 绝对定位）一致。

### 5.3 `GraphExplorer` 透传 + 工作流编辑页接入

`GraphExplorer` 新增同名可选 props 透传给 `GraphCanvas`。
`routes/workflows/[id]/+page.svelte` 的 Edit Tab：
`join(workflowId)` / `leave` 生命周期 + `oncursormove → presence.move`，
`presence={presence.peers}` 传入。只读 Graph（执行检查器内）不传，保持纯净。

---

## 6. P3-C：密度档 + 设计令牌文档

### 6.1 新增 `--density-scale`

`apps/ui/src/styles/tokens.css`：`:root` 加 `--density-scale: 1`，
`--radius` 改为 `calc(0.5rem * var(--density-scale))`。
`preferences.svelte.ts` 加 `DENSITY_SPACING_SCALE`（compact 0.92 / default 1 /
comfortable 1.08）+ `spacingScale` getter；`theme.svelte.ts` 加
`applyDensityScale`；`+layout.svelte` 与 `app.html` 同步应用两变量；
settings 页 Type density 卡更新描述与双档读数。

### 6.2 新建 `docs/design-tokens.md`

速查表：颜色（background/card/running/… 明暗两值）、圆角、字号六档、
`--font-scale`/`--density-scale` 语义、`app.css @theme inline` 映射关系、
Tailwind 用法示例。不含完整代码，以表格 + 用途列为主。

---

## 7. P3-D：快捷键 cheat-sheet 抛光

`HelpModal.svelte`：13 行扁平列表改为 Global / Canvas 两组分区标题；
全局组新增 `?` 行。`AppShell.svelte`：`?`（非输入框内）触发 `toggleHelp`，
与 F1 同行为。`CANVAS_SHORTCUT_HELP` 本体不动（画布词汇唯一来源）。

---

## 8. 代码落点（实际行号）

### 8.1 P3-A：流式 token 可视化

| 文件                                                               | 行号                                                                                                                               | 动作                                                                                              |
| ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `apps/web-app/src/lib/utils/markdown.ts`                           | 全文新增                                                                                                                           | `extractMarkdownText`：字符串 Markdown 启发式                                                     |
| `apps/web-app/src/lib/utils/markdown.test.ts`                      | 新增（7 例）                                                                                                                       | 非字符串/空串/JSON 貌/纯短串判否；多行/标题/行内码/链接/列表/引用判是                             |
| `apps/web-app/src/lib/components/domain/NodeTraceCard.svelte`      | `:10,16,49,148`、`:235`                                                                                                            | Output 双渲染（Markdown 命中走 `StreamMarkdown`，否则 `JsonViewer`）；推理正文改 `StreamMarkdown` |
| `apps/web-app/src/lib/components/domain/ExecutionInspector.svelte` | `:19-20` 引入；`:567` `loadNodeTraces`；`:631,636` 复用加载；`:685-693` `liveTrace`/`liveMarkdown`；`:775` 起 overview Live 输出卡 | trace 加载抽函数供 trace Tab 与 overview（isLive）复用；Live 徽标 + `Open trace` 跳转             |

### 8.2 P3-B：协作光标

| 文件                                                              | 行号                                                                                                                        | 动作                                                                                                      |
| ----------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `apps/web-app/src/lib/graph/canvas-model.ts`                      | 末尾                                                                                                                        | 新增 `PresenceCursor`（画布包裹层相对 CSS px）                                                            |
| `apps/web-app/src/lib/stores/presence.svelte.ts`                  | 全文新增                                                                                                                    | `colorForClient` / `livePeers` 纯函数 + `PresenceStore`（BroadcastChannel 同源广播，100ms 节流，5s 过期） |
| `apps/web-app/src/lib/stores/presence.test.ts`                    | 新增（3 例）                                                                                                                | 颜色稳定性/调色盘范围；自过滤 + 过期过滤                                                                  |
| `apps/web-app/src/lib/components/domain/GraphCanvas.svelte`       | `:43` 类型引入；`:90-92` props；`:131-132` 解构；`:170-179` `reportCursorMove`；`:997` 包裹层上报；`:1060` 起远端光标叠加层 | presence 渲染（箭头 + 名字旗，`pointer-events-none z-20`）                                                |
| `apps/web-app/src/lib/components/domain/GraphExplorer.svelte`     | `:20` 类型；`:116-118` props；`:168-169` 解构；`:972-973` 透传                                                              | 透传给 `GraphCanvas`                                                                                      |
| `apps/web-app/src/lib/components/domain/WorkflowEditPanel.svelte` | `:7-10` 类型；`:41-43` props；`:65-66` 解构；`:152-153` 透传                                                                | 透传给 `GraphExplorer`                                                                                    |
| `apps/web-app/src/routes/workflows/[id]/+page.svelte`             | presence store 引入；Edit Tab `join`/`leave` effect；`presence={presence.peers}` + `oncursormove → presence.move`           | 仅编辑页接入，只读 Graph（执行检查器）不传                                                                |

### 8.3 P3-C：密度档 + 令牌文档

| 文件                                                | 动作                                                                                   |
| --------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `apps/ui/src/styles/tokens.css`                     | `:root` 加 `--density-scale: 1`，`--radius` 改为 `calc(0.5rem * var(--density-scale))` |
| `apps/web-app/src/lib/stores/preferences.svelte.ts` | `DENSITY_SPACING_SCALE`（0.92/1/1.08）+ `spacingScale` getter                          |
| `apps/web-app/src/lib/stores/theme.svelte.ts`       | 新增 `applyDensityScale`                                                               |
| `apps/web-app/src/routes/+layout.svelte`            | effect 内同步应用双变量                                                                |
| `apps/web-app/src/app.html`                         | 首屏内联恢复双变量（防闪）                                                             |
| `apps/web-app/src/routes/settings/+page.svelte`     | Type density 卡：文案改为字号 + 圆角，读数 `Type N.NN× · corners N.NN×`                |
| `docs/design-tokens.md`                             | 新建：颜色/圆角/字号速查 + 密度双变量语义 + Tailwind 用法                              |

### 8.4 P3-D：快捷键抛光

| 文件                                                      | 动作                                                                                       |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `apps/web-app/src/lib/components/layout/HelpModal.svelte` | 13 行扁平列表拆为 Global / Canvas 两组；全局组新增 `F1 or ?` 行                            |
| `apps/web-app/src/lib/components/layout/AppShell.svelte`  | `?`（非输入框内）触发 `toggleHelp`，`isTypingTarget` 保护；`CANVAS_SHORTCUT_HELP` 本体不动 |

---

## 9. 验证情况

| 检查项   | 命令                                                         | 结果                                                                           |
| -------- | ------------------------------------------------------------ | ------------------------------------------------------------------------------ |
| 类型检查 | `apps/web-app` 下 `npm run check`                            | 0 errors / 0 warnings                                                          |
| 单测     | 同上 `npm run test`                                          | 13 文件 / 146 例全部通过（P1 基线 11/136，新增 markdown 7 例 + presence 3 例） |
| 静态检查 | 同上 `npx eslint .`                                          | 无告警（中途修掉 `presence.svelte.ts` 一处 `no-useless-assignment`）           |
| 格式     | 同上 `npm run format:check` + docs 两文件 `prettier --check` | 全部通过（`tokens.css` 本体无需改格式）                                        |

环境说明：`apps/node_modules/@wf-agent/ui` 的 workspace 软链缺失导致初次
`check` 报 339 个模块缺失（非本次改动），补软链（`ui -> ../../ui`，即
`npm install` 会做的事，未联网）后通过。后端未编译：本阶段不改动 Rust 代码。

过程中修掉的两个具体问题：

1. `PresenceStore.join` 里 `let channel: ... | null = null` 触发
   `no-useless-assignment`——改为 try 内直接声明，catch 直接 return。
2. 三个改动文件的 prettier 格式（`ExecutionInspector`、`GraphCanvas`、
   `workflows/[id]/+page`）——`prettier --write` 已对齐。

---

## 10. 风险与回退

| 风险                               | 影响                          | 处置                                            |
| ---------------------------------- | ----------------------------- | ----------------------------------------------- |
| Markdown 启发式误判结构化字符串    | 文本输出被渲染为 Markdown     | 阈值收紧只改 `markdown.ts` 一处；回退即删调用点 |
| overview 自动拉 trace 增加一次请求 | Live 执行多一次 `/nodes` 请求 | 仅 `isLive` 且未加载时触发；回退即删该分支      |
| `BroadcastChannel` 跨标签页回声    | 本页收到自己的光标            | `clientId` 自过滤；不支持环境静默停用           |
| `--radius` 联动影响全部圆角        | 视觉微调                      | 只改 `tokens.css` 两行即回退                    |
| `?` 与输入法冲突                   | 中文输入法打出 `?` 时弹帮助   | 输入框内不触发（`isTyping` 同语义）             |

---

## 11. 遗留与后续

1. **真逐节点 token 流**：需后端 `LlmDelta`/`ReasoningDelta` 携带 `node_id`
   （或新增节点级增量端点），前端 `stream-parser` 扩展按节点分流后再接入
   `ExecutionInspector`；在那之前 P3-A 即上限。
2. **跨用户协作**：`ws.rs` 的 `Presence deferred` 落地后，把 presence store 的
   `BroadcastChannel` 换成 WS 通道即可，`GraphCanvas` 的 `presence` prop 不用变。
3. **服务端锁端点缺失**：`workflow-locks` 的 404 放行逻辑保持不变，
   与本阶段无关。
4. **间距变量化**：`--density-scale` 目前只联动圆角；若要 gap/padding 全随密度，
   需把布局单位逐个改为 `calc(... * var(--density-scale))`，属独立议题。

---

## 12. 附录：参考文档与后端出处索引

| 主题                  | 出处                                                                                   |
| --------------------- | -------------------------------------------------------------------------------------- |
| 优先级清单（P3 来源） | `docs/ref/frontend-viz/06-可借鉴Dify与n8n的设计.md` 第 6 节                            |
| 流式 token 建议       | 同上 §1.3                                                                              |
| 协作光标建议          | 同上 §2.3                                                                              |
| 令牌文档 / 密度档建议 | 同上 §4.3 / §5.2                                                                       |
| 快捷键建议            | 同上 §5.3                                                                              |
| 增量无节点归属        | `crates/app/wf-api/src/infra/stream.rs:37,69,71`                                       |
| SSE 无文本帧          | `apps/web-app/src/lib/api/sse.ts:7-16`                                                 |
| WS Presence deferred  | `crates/app/wf-server/src/ws.rs:20`                                                    |
| 锁端点服务端缺失      | `crates/app/wf-server/src` + `openapi.json` 无 `/lock` 命中                            |
| 密度机制              | `stores/preferences.svelte.ts:40-44`、`stores/theme.svelte.ts:27-30`、`app.html:25-28` |
| 令牌源                | `apps/ui/src/styles/tokens.css`、`apps/web-app/src/app.css:7-73`                       |
| 快捷键源              | `lib/graph/canvas-shortcuts.ts:10-21`、`HelpModal.svelte`、`AppShell.svelte:36-44`     |
