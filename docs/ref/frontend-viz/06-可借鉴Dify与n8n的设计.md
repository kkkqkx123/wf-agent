# wf-agent 前端分析 · 可借鉴 Dify 与 n8n 的设计

> 本文是 `docs/ref/` 的核心交付物：基于 W（wf-agent）现状与前两轮的 D（Dify）、N（n8n）对比，给出 **W 应向 D/N 借鉴的具体设计点**。
> 反向（W 已有、D/N 可学）的亮点在各维度文档 `01-05` 已标注，此处不再展开。

## 0. 一句话结论

W 的**工程化底座（受控编辑、纯函数图模块、shadcn 式令牌、执行分析叠加）已经领先 D/N**；真正需要补强的是 **「交互丰富度」与「产品化表达」**——而这两点恰是 D/N 的强项：

- **向 n8n 借**：端口类型校验、节点创建/搜索面板、BaseLayout 插槽化、可重跑的 execution 实体、品牌色锚点。
- **向 Dify 借**：逐节点结构化追踪卡、节点/模板市场一键插入、协作光标、轻透留白、流式 token 可视化。

---

## 1. 执行可视化：补「逐节点追踪」与「可重跑 execution」

### 1.1 借鉴 Dify：逐节点结构化追踪卡
- **Dify 有**：`RunPanel` 三 Tab（RESULT/DETAIL/TRACING）+ 每节点 `NodePanel` 展开输入/过程/输出 JSON、token/耗时、迭代/循环/重试/agent 子日志（`docs/01-执行过程可视化.md`）。
- **W 缺**：`ExecutionInspector` 的 graph Tab 偏「图着色 + 投影」，选中节点卡只显示 kind/status/迭代/耗时（`GraphExplorer.svelte:1011-1106`），**没有可展开、可检索的逐节点输入/输出/中间过程面板**。
- **落地建议**：在 `ExecutionInspector` 的 `tools`/`state` Tab 旁新增「**Node Trace**」Tab，复用 `ToolCallCard`/`KeyValueList` 组件，按 `nodeId` 聚合 SSE 帧与 `getExecutionToolCalls`/`getExecutionContext` 数据，渲染 Dify 式可折叠树。可复用现有 `lib/components/domain/ToolCallCard.svelte`。

### 1.2 借鉴 n8n：可检索/可重跑的 Execution 实体
- **n8n 有**：`WorkflowExecutionsView` + `ExecutionsView` + 卡片左边框状态色 + 详情 + 手动重跑（`docs/01`）。
- **W 缺**：`WorkflowRunsPanel.svelte:58-79` 仅列 id+状态+时间，**无筛选、无参数重跑、无失败原因摘要**。
- **落地建议**：在 `WorkflowRunsPanel` 顶部加 `FilterBar`（复用 `lib/components/ui/FilterBar.svelte`）+ 每行 `重跑` 按钮（调 `executeWorkflow` 同款入口）；失败行直接展示 `failedNodes` 摘要并跳 `executions/[id]`。

### 1.3 借鉴 Dify：流式 token 级可视化
- **Dify 有**：`StreamMarkdown` 流式渲染 LLM 输出（`markstream-svelte` 思路，W 已装此依赖！）。
- **W 缺**：实时仅节点粒度（SSE 缓冲刷新节点状态，`ExecutionInspector.svelte:108-112`），未下钻到 token 流。
- **落地建议**：在 `state`/`overview` Tab 用已装入的 `markstream-svelte` 渲染 `currentNode` 的流式子输出，复用 `lib/components/chat/StreamMarkdown.svelte`。

---

## 2. 工作流编辑：补「类型端口」与「节点创建器」

### 2.1 借鉴 n8n：端口（输入/输出）类型校验
- **n8n 有**：节点有 `Inputs(Bottom/Left)` / `Outputs(Right/Top)` 端口，`isValidConnection` 按端口类型把关（`docs/02`）。
- **W 缺**：`canvas-connect.ts` 只校验「拓扑合法性」（自连/重复/组/隐藏），**不校验端口/类型语义**，会出现「LLM 节点连到触发器输入」这类语义错边。
- **落地建议**：在 `display-model.ts` 增加 `portType(kind)` 与 `connectByPort(source, target, sourcePort, targetPort)`，于 `describeConnectRejection`（`canvas-connect.ts:13`）追加类型分支；`GraphCanvas` 的 hotspot 拖拽需携带端口信息（`GraphCanvas.svelte:315-478`）。

### 2.2 借鉴 n8n + Dify：可视节点创建/搜索面板
- **n8n 有**：节点面板、双击/上下文创建、搜索过滤；**Dify 有**：节点市场/模板一键插入。
- **W 缺**：仅「双击空白加节点 + hotspot 连线」（`workflows/[id]/+page.svelte` 调 `handleAddNode`），无节点选择面板，`templates` 是注册表而非可视化插入器。
- **落地建议**：复用 `CommandPalette` 的 UI 模式，新增「**Add Node**」抽屉：列出 `configs/node-templates` 的模板，选中后落到双击坐标（`onbackgrounddoubleclick` 已传 position，`GraphCanvas.svelte:96`、`959`）；从 `templates` 页可「插入到当前画布」。

### 2.3 借鉴 Dify：实时协作光标
- **Dify 有**：画布协作光标（`docs/02`）。
- **W 缺**：有 lease 锁但**无多人实时光标**（lease 只是互斥，非协同）。
- **落地建议**：lease 持有者可经 WebSocket 广播光标坐标，`GraphCanvas` 叠加 `<div>` 游标（可参考 hotspot 渲染方式 `GraphCanvas.svelte:969-981`）。优先级低于 2.1/2.2。

---

## 3. 页面布局：补「插槽化」与「沉浸画布」

### 3.1 借鉴 n8n：BaseLayout 具名插槽
- **n8n 有**：`BaseLayout` 用 `header/sidebar/footer/overlays` 具名插槽，可组合出不同页面形态（`docs/03`）。
- **W 缺**：`AppShell.svelte` 写死「侧栏+顶栏+主区」，要「无侧栏沉浸画布 / 三栏对照」需改骨架。
- **落地建议**：把 `AppShell` 改为接受 `header`/`sidebar`/`aside`/`overlays` snippet 的插槽组件（Svelte 5 原生 `<slot>`/`Snippet` 已就绪），`routes/+layout.svelte` 注入默认插槽；执行对照视图可临时传 `aside` 渲染历史/当前双画布。

### 3.2 借鉴 n8n：并排执行对照（dual view）
- **n8n 有**：重叠/并排对照历史与当前执行。
- **W 缺**：单检视器（`SplitView.svelte`）。
- **落地建议**：`SplitView` 增加 `dual` 模式，传两份 `inspector` snippet（如「版本 A 图 / 版本 B 图」），配合 `execution-projection.ts` 的 `buildVersionDiffView`（`execution-projection.ts:283`）天然适配。

### 3.3 借鉴 Dify：画布内极简工具条（少遮挡）
- **Dify 有**：画布内绝对定位 header，不需要时近乎隐形（`docs/03`）。
- **W 缺**：`TopBar` 常驻占 48px（`TopBar.svelte:29`），画布被顶栏+侧栏双重占用。
- **落地建议**：画布页（graph/edit Tab）滚动时收起顶栏（CSS `translateY(-100%)` + 滚动监听），或在画布内嵌极简工具条（`GraphExplorer.svelte:678` 的工具栏本就可内联）。

---

## 4. 组件设计：补「composables 复用」与「可发布 UI 包」

### 4.1 借鉴 n8n：响应式 composable 层
- **n8n 有**：`useCanvas` 等 composables 把生命周期/响应式逻辑从组件剥离、跨页面复用（`docs/04`）。
- **W 缺**：图逻辑是纯函数（`lib/graph/*`），缺「与组件生命周期绑定的 runes composable」，多页面复用投影/选区时易重复 `$effect`。
- **落地建议**：在 `lib/graph/` 增 `useGraphProjection.ts` / `useGraphSelection.ts`（Svelte 5 runes 函数式 composable），内部调 `projectExecutionOverlay`/`capGraph`，供 `ExecutionInspector` 与 `workflows/[id]` 共享。

### 4.2 借鉴 Dify：独立共享组件库
- **Dify 有**：`@langgenius/dify-ui` 独立包，`docs/04`。
- **W 缺**：`lib/components/ui/*` + `variants.ts` 耦合在 web-app 内。
- **W 计划多前端**（Web/TUI/VS Code，`README_zh.md:86`）→ **建议抽成 `@wf-agent/ui` 包**，把 `Button/Badge/Card/Icon/StatusBadge/variants.ts/app.css 令牌` 独立发布，前端多端共用。

### 4.3 借鉴 n8n：设计令牌文档化
- **n8n 有**：design-system 令牌体系明确（`docs/05`）。
- **W 缺**：`app.css` 令牌完备但无速查表。
- **落地建议**：补 `docs/design-tokens.md`，列出 `--background/--card/--running/--spacing-*/--radius` 及用途，对齐多端。

---

## 5. UI 风格：补「品牌色锚点」与「留白轻盈」

### 5.1 借鉴 n8n：一处高饱和品牌色
- **n8n 有**：橙红品牌色用于主 CTA/Logo，辨识度高（`docs/05`）。
- **W 缺**：几乎全中性冷灰蓝（hue 214–252），`running` 靛紫是唯一的「彩色」，缺品牌锚点。
- **落地建议**：在 `app.css` 增 `--brand` 令牌（如 `#f97316` 级饱和色），用于 logo 方块（`Sidebar.svelte:109` 的 `bg-primary` 方块）、主 Run 按钮（`workflows/[id]/+page.svelte:605` 的 Run）、空状态图标，提升识别。

### 5.2 借鉴 Dify：更大留白与轻透
- **Dify 有**：轻透圆角、宽松留白（`docs/05`）。
- **W 缺**：偏紧凑 IDE 风（micro 字号 0.6875rem、行 `py-1.5`），低密度屏略拥挤。
- **落地建议**：在 `preferences` 增加「舒适/紧凑」密度档，映射到 `--font-scale` 与间距变量（已有 `--font-scale` 机制，`theme.svelte.ts:27`，仅需扩展间距变量）。

### 5.3 借鉴两者：键盘可达性一致性
- **W 已强**（⌘K 命令面板、F1 帮助、画布快捷键，`canvas-shortcuts.ts`）。保持即可；可借鉴 n8n 把快捷键提示做成**可发现的可视化 cheat-sheet 弹窗**（W 的 HelpModal 已存在，直接复用渲染 `CANVAS_SHORTCUT_HELP`）。

---

## 6. 优先级清单（建议实施顺序）

| 优先级 | 借鉴项 | 来源 | 收益 | 改动文件 |
| --- | --- | --- | --- | --- |
| P0 | 节点创建/搜索面板 | n8n + Dify | 编辑可用性跃升 | `WorkflowEditPanel`、`CommandPalette`、新增 AddNode 抽屉 |
| P0 | 端口类型校验 | n8n | 杜绝语义错边 | `display-model.ts`、`canvas-connect.ts`、`GraphCanvas.svelte` |
| P1 | 逐节点追踪卡 | Dify | 执行可调试性 | `ExecutionInspector.svelte` + `ToolCallCard` |
| P1 | Execution 筛选/重跑 | n8n | 运维闭环 | `WorkflowRunsPanel.svelte` |
| P1 | `@wf-agent/ui` 抽包 | Dify | 多前端复用 | `lib/components/ui/*`、`variants.ts`、`app.css` |
| P2 | BaseLayout 插槽化 | n8n | 布局灵活度 | `AppShell.svelte`、`+layout.svelte` |
| P2 | 并排执行对照 | n8n | 调试对比 | `SplitView.svelte` + `execution-projection` |
| P2 | 品牌色锚点 | n8n | 辨识度 | `app.css` `--brand` |
| P3 | 协作光标 / 流式 token | Dify | 协同+体验 | `GraphCanvas`、`StreamMarkdown` |
| P3 | 密度档 / 设计令牌文档 | Dify+n8n | 多端对齐 | `preferences`、`docs/design-tokens.md` |

---

## 7. 反向提醒（W 已领先、勿回退）

- **勿丢弃** 规范化状态色调排名 `rankTone`（`display-model.ts:406`）、慢节点热度分档、关键路径/决策点图叠加、版本拓扑 diff——这些是 W 相对 D/N 的差异化优势。
- **勿丢弃** 受控编辑 + 整图快照原子撤销 + lease 乐观锁——比 D/N 更稳的并发模型。
- **勿丢弃** 纯函数图模块 + 单测、`frost` 毛玻璃、字号缩放变量——工程化资产。

> 借鉴原则：**吸收 D/N 的「交互密度与产品表达」，保留 W 的「工程严谨与分析深度」**，二者互补而非替换。
