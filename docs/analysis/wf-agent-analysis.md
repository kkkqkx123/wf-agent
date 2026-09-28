# wf-agent 可视化与运行状态分析：补强方向与 n8n 借鉴设计

> 分析范围（按需求）：**图工作流可视化、Agent 可视化、模板编辑、运行状态管理**。
> 不深入内核实现差异（双引擎 / checkpoint / 沙箱等 Rust 内核逻辑不在本文讨论）。
>
> 信息来源：本地克隆 `kkkqkx123/wf-agent`（经 `gh-proxy.com` 代理），以及本地 n8n 仓库源码（`/workspace/n8n`，版本 2.41.0）。所有结论均附 `文件:行` 引用，可直接核对。

---

## 0. 项目概况与结论速览

wf-agent 是 **Rust 模块化 Agent 框架**（Cargo workspace，4 层 crate 架构），前端为 **SvelteKit（`apps/web-app`，Svelte 5 + Tailwind + cytoscape）**。它在同一运行时上统一了「图结构工作流编排」与「自主 LLM Agent 循环」两种执行模型。

本次聚焦的四块现状一句话概括：

| 模块 | 现状 | 关键文件 | 与 n8n 差距 |
|------|------|----------|-------------|
| 图工作流可视化 | **只读查看器**（cytoscape），支持 4 布局 / 过滤 / 大图截断 / 分析叠加 | `display-model.ts`、`GraphCanvas.svelte`、`GraphExplorer.svelte` | **完全没有可视化编辑器**（拖拽/连线/内嵌参数） |
| Agent 可视化 | 对话式流式 UI（气泡/推理块/工具卡）+ 决策图查看 | `chat/*`、`SessionInspector.svelte`、`ToolCallCard.svelte` | 决策图与实时流未联动，无执行路径高亮 |
| 模板编辑 | JSON / Form 双向编辑 + 校验 + 克隆 + 导入导出 + 版本对比 | `templates/+page.svelte`、`JsonEditor.svelte` | 工作流模板**无图形化编辑**，校验仅客户端 |
| 运行状态管理 | **较完善**：SSE 过滤 + 指数退避重连、80ms 缓冲合并、ExecutionDetail 含关键路径/失败节点 | `sse.ts`、`stream.ts`、`stream-run.svelte.ts`、`models.ts` | 执行态**未上图**（画布无节点级着色/回放） |

**核心结论**：wf-agent 的「运行时状态管理」是一条腿已经站稳，而「图/编辑器」这条腿还停留在只读查看阶段。最大、最值得优先补强的是**图工作流可视化编辑器**——这也是 n8n 最核心的产品护城河。

---

## 1. 现状盘点（基于真实代码）

### 1.1 图工作流可视化

**视觉映射层（单一真相源）** —— `apps/web-app/src/lib/graph/display-model.ts`
- 三套图预设：`'workflow' | 'decision' | 'execution'`（`display-model.ts:8`）。
- 节点形状 `nodeShape()`（`display-model.ts:55`）：终端节点→椭圆、工具/LLM→圆角矩形/六边形、决策/分支→菱形。
- 边虚线判定 `isDashedEdge()`（`display-model.ts:74`）：conditional / error_route / branch 走虚线。
- 状态色 `statusHex()`（`display-model.ts:96`）：success/running/danger/warning/info/neutral 六档。
- **关键设计理念**（注释第 1-5 行）：「backend only supplies pure topology; coordinates and visual encoding always live here」——后端只给纯拓扑，坐标与视觉编码全在前端。这条原则本身很清晰，是后续补编辑能力的好地基。

**画布渲染器** —— `apps/web-app/src/lib/components/domain/GraphCanvas.svelte`
- 基于 **cytoscape**（动态导入 `GraphCanvas.svelte:285`），支持 `layered/columns/force/grid` 四种布局（`GraphCanvas.svelte:148`）。
- 交互仅**查看级**：点选、双击展开邻居、框选、缩放、导出 PNG（`GraphCanvas.svelte:207-276`）。事件回调 `onselect/onexpand/onboxselect` 全是「查看/展开」语义，**没有任何拖拽建边、拖拽移动、手柄连接**（`:358-377`）。
- 性能保护：`GRAPH_NODE_CAP=800` 大图按 kind 轮询采样截断（`display-model.ts:156,203`）；`zoomedOut` 跨阈值才隐藏边标签（`GraphCanvas.svelte:55,112`）。

**图浏览器外壳** —— `apps/web-app/src/lib/components/domain/GraphExplorer.svelte`
- 过滤（按 kind 勾选 / 文字搜索）、叠加层（cycles / unreachable / dead-ends 高亮）、选中详情卡、图例、大图「Fold to kind」（`GraphExplorer.svelte:87-288`）。

**工作流详情页** —— `apps/web-app/src/routes/workflows/[id]/+page.svelte`
- **编辑按钮明确禁用**：`icon="pencil"`，`label="Definition editing is not available in this release"`，`disabled`（`workflows/[id]/+page.svelte:426-430`）。
- 有 Graph / Versions / Drafts / Runs 四 Tab；Drafts 仅提供「Preview graph」拓扑查看（`workflows/[id]/+page.svelte:673-697`）。
- 已能做**分析叠加**：`getGraphAnalysis` 返回环检测 / 可达性，渲染为 overlay（`workflows/[id]/+page.svelte:147-172`）；`expandNeighborhood` 调用后端邻居接口（`workflows/[id]/+page.svelte:220-236`）。

### 1.2 Agent 可视化

- 对话式 UI：`chat/+page.svelte` 用 `TranscriptScroller` + `MessageBubble` + `ReasoningBlock` + `ToolCallCard` + `StreamMarkdown` 渲染流式会话（`chat/+page.svelte:296-394`）。
- **实时流驱动**：`StreamRunStore`（`stream-run.svelte.ts:25`）持有 `answer/reasoning/tools/subAgents/usage/iteration`，由 `chat/+page.svelte:156-194` 的回调写入。
- 工具调用卡 `ToolCallCard.svelte` 展示输入/输出/状态/时长；`SessionInspector.svelte` 提供会话侧栏。
- 决策图：Agent 循环详情含 `graph: WorkflowGraph`，用 `decision` 预设渲染（见 `models.ts:251` 与 `AgentLoopDetail`）。但 **chat 页面本身是纯时间线，没有把实时流式步骤映射到决策图节点**。

### 1.3 模板编辑

- 模板库 `templates/+page.svelte`：node / trigger / agent / workflow 四类（`TABS` 行 44-50）。
- **JSON ↔ Form 双向编辑**：`formFromDefinition` / `formToDefinition` 在两种表示间映射（`templates/+page.svelte:197-226`），Form 模式编辑核心字段、其余留 JSON。
- **客户端校验**：`validateTemplateDefinition()` + `jsonErrorLine()` 定位错误行（`templates/+page.svelte:237-253`）；`summaryTemplate` 生成摘要（`templates/+page.svelte:108`）。
- 操作齐全：克隆、导入（JSON 文本）、导出、删除（含 `importEditor` 行号跳转 `templates/+page.svelte:776-799`）。
- 自研 `JsonEditor.svelte`：手写 tokenizer 做语法高亮 + 行号 + 错误行标红（`JsonEditor.svelte:36-103`）。
- **缺口**：workflow 模板只能 JSON/Form 编辑，**没有图形化工作流编辑器**与其联动（因为 1.1 所述编辑器不存在）。

### 1.4 运行状态管理

- **事件流 SSE**：`openEventStream` 支持 `execution_id` / `agent_loop_id` / `workflow_id` / `since` 过滤，断线**指数退避重连**（1s→10s，`sse.ts:46,89`）。
- **流式读取泵**：`openPostStream` / `openGetStream` 用 fetch + reader 手动解析 SSE 帧，区分 429 与中途断流（`stream.ts:54-191`）。
- **运行态 Store**：`StreamRunStore` 用 80ms tick 把高频 delta 合并进渲染态（`STREAM_FLUSH_MS=80`，`stream-run.svelte.ts:4,67`），避免逐帧解析。
- **数据模型**（`models.ts`）：`Execution`（含 progress / currentNode / tasksTotal / failedNodes / memoryPeakBytes，`models.ts:8-25`）；`ExecutionDetail` 扩展出 `callStack`、`variables`、`analysis`（含 `criticalPath` / `failureNodes` / `slowNodes` / `decisionPoints` / `iterations`，`models.ts:34-47`）；`EventRecord`（`models.ts:415`）。
- 展示组件：`ExecutionCard`、`ExecutionInspector`、`Timeline`、`StatusBadge` 等。
- **缺口**：执行态数据**未回写到图**——`GraphCanvas` 节点的 `status` 字段在 `WorkflowGraph` 映射里存在（`WorkflowGraph.svelte:24`），但运行页执行的是另一种数据流，图上没有「当前节点运行/失败」实时着色与回放。

---

## 2. 功能缺口与待补强项

### 2.1 图工作流可视化（缺口最大）
1. **无可视化编辑器**：节点不能拖拽、不能连线建边、不能内嵌编辑参数。这是与 n8n 最根本的差距。
2. **无分组/折叠**：`display-model.ts` 无任何 group 概念；n8n 有完整 group/collapse 与连接重映射。
3. **执行态未上图**：`status` 字段虽有，但运行时不驱动节点着色；没有「关键路径高亮 / 失败节点红框 / 当前节点脉冲」。
4. **布局为通用图算法**：cytoscape `breadthfirst`/`cose` 不是面向 DAG 工作流的分层+AI 竖向子图布局，大工作流可读性弱。
5. **无画布内节点搜索定位 / minimap / 快捷键**：仅有基础缩放。

### 2.2 Agent 可视化
1. 决策图（decision 预设）是**静态快照**，未与 chat 页实时流式步骤联动。
2. 工具调用 `ToolCallCard` 与决策图节点**未互相关联**（点工具卡不会聚焦对应图节点）。
3. 缺「执行路径回放 / 步进」能力——`ExecutionDetail.iterations` 已存数据，但未可视化。

### 2.3 模板编辑
1. workflow 模板**无图形编辑**，强依赖 JSON 文本，门槛高、易错。
2. 校验是**纯客户端**，缺服务端 schema 强校验与「保存即编译」反馈。
3. 版本对比 `diffWorkflowVersions` 只给「增删节点/边」文本列表（`workflows/[id]/+page.svelte:580-586`），**无图形化 diff**。
4. 模板与画布编辑器割裂：模板库改完不能一键「在画布打开编辑」。

### 2.4 运行状态管理
1. 执行态与拓扑**两张皮**：SSE 数据丰富，但没注入 `GraphCanvas` 做节点级着色。
2. 缺**协作写锁 / 多人编辑**：当前单用户假设。
3. 缺**时间旅行/回放**：`checkpoint` 子系统在后端已存在，前端无对应回放 UI。
4. 缺**节点级指标下钻**：`ExecutionDetail.analysis.slowNodes` 有数据，但未在图上以热力/标注呈现。

---

## 3. 可借鉴 n8n 的设计（附源码设计思路）

> 每一项都给：**n8n 源码怎么做的（file:line）+ 设计思路 + 迁移到 wf-agent 的具体建议**。
> 注意技术栈差异：n8n 用 Vue 3 + **Vue Flow**，wf-agent 用 Svelte 5 + **cytoscape**。思路可复用，库 API 需改写。

### 3.1 单一映射层（renderData 投影 → 画布节点/边）

**n8n 怎么做**
- `useCanvasMapping.ts` 是画布与数据的「最终胶水层」。注释明确：所有按节点投影（字幕、校验错误、执行状态、渲染类型、便签 z-index 等）都先由 `useWorkflowDocumentRenderData` 产成 `CanvasRenderData`，映射层只负责「读 renderData + connections 拼出 CanvasNode / CanvasConnection」（`useCanvasMapping.ts:40-48`）。
- 节点映射 `mappedNodes`（`useCanvasMapping.ts:141-198`）、边映射 `mappedConnections`（`useCanvasMapping.ts:200-210`）。

**设计思路**：把「语义投影」与「几何映射」两阶段分离。语义（执行状态、校验、脏标记）在外层 store 算好，映射层是纯函数。这样编辑、执行、校验三套数据都能独立更新，画布只消费最终投影。

**wf-agent 借鉴**
- 把 `display-model.ts` 升级为**两阶段**：(a) `projectRenderData(workflow, execution?, validation?)` 算出每节点的 `status/issue/dirty`；(b) `mapToDisplay(nodes, edges, renderData)` 产出 `DisplayNode/DisplayEdge`。当前 `nodeShape/statusHex` 是「即时函数」，可保留为第二阶段工具。
- 运行时把 `ExecutionDetail`（`models.ts:34`）的 `criticalPath/failureNodes/currentNode` 喂进投影，自动着色——解决 2.1.3。

### 3.2 渲染类型多态（render-type 分发）

**n8n 怎么做**
- `CanvasNodeRenderType` 枚举五态：`Default / StickyNote / AddNodes / ChoicePrompt / Agent`（`canvas.types.ts:58-64`）。节点数据里只存一个 `render.type`，真正的视觉差异由对应渲染器分发。
- 节点尺寸也按渲染类型算：`computeNodeDisplaySize` 对 Default，Agent 用固定 `AGENT_NODE_SIZE`（`useCanvasMapping.ts:116-128`）。

**设计思路**：用「数据里只放类型 + 渲染器按类型多态」替代「函数里 if/else 堆形状」。新增一种节点可视化（如 wf-agent 未来要的 StickyNote / Agent 卡）只需加一个渲染类型与对应组件。

**wf-agent 借鉴**
- 把 `display-model.ts` 的 `nodeShape()` 字符串分支（`display-model.ts:55-71`）重构为 `nodeRenderType(kind, preset)` → 返回枚举，再让 `GraphCanvas` 按类型选 cytoscape 样式类。为未来的「便签 / Agent 卡 / 触发器」预留扩展点，不必改核心映射。

### 3.3 单一数据源 + 交互回写（编辑模式地基）

**n8n 怎么做**
- `Canvas.vue` 渲染 `<VueFlow>` 时**关闭 `applyChanges`**（`setInteractive` / 受控模式），所有节点增删改、连线、移动都不让库自己改内部状态，而是回写到 workflow store，再由映射层重算——保证单一真相源（本项目上一轮分析中 `Canvas.vue` 已确认该模式）。
- 约 40 个交互事件通过 eventBus 回传（`canvas.types.ts:267-292`：`fitView`、`tidyUp`、`nodes:select`、`create:sticky` 等）。

**设计思路**：画布库只负责「画 + 发出意图事件」，所有状态变更归 store。这样撤销/重做、协同、执行态注入都不会和库的本地状态打架。

**wf-agent 借鉴**
- 若要补 **3.1 编辑器**（优先级最高），必须先在 `GraphCanvas.svelte` 引入同样的「受控模式」：节点位置/边来自 Svelte store，cytoscape 的 `drag`/`connect` 事件只 `emit` 意图（`onmove`/`onconnect`），由 store 写回并重新 `syncElements`（`GraphCanvas.svelte:175-200` 已有 syncElements 框架，正好复用）。
- 建议新建 `workflowEditor.svelte.ts`（类 `StreamRunStore` 的模式）持有可编辑图状态 + 历史栈。

### 3.4 执行态着色与状态优先级

**n8n 怎么做**
- 边的状态优先级常量：`['running','pinned','error','success']`（`useCanvasMapping.ts:38`），`getConnectionStatus` 按此优先级取最高（`useCanvasMapping.ts:212-241`）。
- 非 main 连接（AI model/memory/tool）是「被动」的，仅当目标节点也运行时才算执行过（`:226-230`）——避免 AI 配置线乱闪。
- 节点快照 `getNodeExecutionSnapshot` 聚合 running/waiting/error/dirty/iterations（`useCanvasMapping.ts:73-98`）。

**设计思路**：多状态叠加时要有**明确优先级**，否则红/绿/蓝互相覆盖让用户困惑。边与节点都先聚合成单状态再染色。

**wf-agent 借鉴**
- 在 `display-model.ts` 的 `statusHex()`（`display-model.ts:96`）基础上，定义 `EXEC_STATUS_PRIORITY`，让 `cg`（current/running）>`error`>`warning` 决定单节点色；把 `ExecutionDetail.currentNode` 标为 running 脉冲、`failureNodes` 标红框、`criticalPath` 节点描金边。
- 关键：这套优先级逻辑放在 **3.1 的投影层**，画布只消费结果。

### 3.5 自动布局（DAG 友好的 dagre 层）

**n8n 怎么做**
- `useCanvasLayout.ts` 用 `@dagrejs/dagre`：先 `dagre.graphlib.alg.components` 把图拆成**连通子图**（`useCanvasLayout.ts:898`），子图并排。
- **AI 竖向子图**：每个 AI/配置父节点拉出自子图，TB 方向布局，再把包围盒塞回父图（`createAiSubGraph`，`useCanvasLayout.ts:686-716`）。
- **整组作为单元**：group 折叠时以 chip、展开时以 frame 进入 dagre（`getGroupUnitForTarget`，`useCanvasLayout.ts:289-352`）；便签吸附到其覆盖的节点并网格对齐（`attachCoveringStickies` `:244`，`placeStickies` `:843`）。
- **网格吸附**：`snapToGrid` 按连接手柄而非左上角对齐（`:130-136, 1186-1198`）。

**设计思路**：工作流图不是普通网络图，有「主流向 + AI 子分支」结构。用 dagre 的分层 + 子图 + 整组单元，比通用力导/树布局可读性强得多。

**wf-agent 借鉴**
- 在 `display-model.ts` 增加 `layoutGraph(nodes, edges, preset)`：workflow 预设用 dagre 分层（左→右）替代当前 `breadthfirst`；decision 预设保留「每迭代一列」的 `columnPositions`（`display-model.ts:251`）；execution 预设可复用 workflow 布局再叠加状态。
- Agent 子图：把 `iteration` 相近、tool_call 链视作 AI 竖向子图，复用 n8n 的「父节点 + 配置子图」思路。

### 3.6 分组 / 折叠与连接重映射

**n8n 怎么做**
- `remapCollapsedGroupConnections` 把组内隐藏成员的连接重定向到 group 边界，折叠态一条边代表多条（`useCanvasMapping.ts:202`；逻辑在 `useCanvasMapping.groups`）。
- `CanvasConnectionData.canonicals` 记录被合并的真实端点，状态取最高优先级（`useCanvasMapping.ts:243-256`）。

**设计思路**：折叠不是「删节点」，而是「把内部复杂度收进一个框」，对外连接语义不变。

**wf-agent 借鉴**：当前 `display-model.ts` 无 group。若工作流支持分组（自然语言/批量节点），应在映射层加 `groupView`，`capGraph` 之外再做「折叠态连接重映射」，避免大图截断（2.1.2 的 800 上限）误伤结构信息。

### 3.7 协同编辑写锁（可后置于 P2）

**n8n 怎么做**：`features/collaboration` 经 WebSocket 推送在线协作者，采用 **single-write 写锁**（心跳 + 轮询 + 超时释放），避免两人同时改同一节点。

**wf-agent 借鉴**：wf-agent 已是多前端（Web / CLI / TUI / VS Code）框架，但缺「多人同时编辑同一工作流」保护。可在 `wf-server` SSE 之上加 collab 通道，前端加 single-write 锁 UI（类似 n8n）。属 2.4.2，优先级低于编辑器与执行上图。

---

## 4. 优先行动路线图

| 优先级 | 事项 | 借鉴 n8n | 收益 |
|--------|------|----------|------|
| **P0** | 图工作流可视化编辑器：拖拽移动、手柄连线、画布内建/删节点 | 3.3 单一数据源 + 受控模式 | 补齐与 n8n 最根本差距，使 workflow 模板/草稿可图形编辑 |
| **P0** | 执行态上图：节点级 running/error 着色 + 当前节点脉冲 + 关键路径描边 | 3.1 投影层 + 3.4 状态优先级 | 把已完善的运行状态「可视化」到图，闭环 2.1.3/2.4.1 |
| **P1** | 自动布局升级为 dagre（workflow 分层 + AI 竖向子图） | 3.5 | 大工作流可读性，替代 breadthfirst |
| **P1** | Agent 决策图与实时流联动：流式步骤高亮对应图节点、工具卡聚焦节点 | 3.1 + 3.4 | 补 2.2.1/2.2.2，统一「看」与「跑」 |
| **P2** | 模板图形化编辑 + 保存即编译（服务端 schema 校验） | 3.3 + 3.6 | 降门槛、防错，补 2.3.1/2.3.2 |
| **P2** | 版本/草稿图形化 diff（增删节点以图着色呈现） | 3.4 着色 | 补 2.3.3 |
| **P2** | 协作写锁 + 执行回放（时间旅行） | 3.7 | 补 2.4.2/2.4.3，复用已有 checkpoint 后端 |

---

## 5. 关键文件索引

**wf-agent（前端，皆为 `apps/web-app/src/`）**
- 视觉映射：`lib/graph/display-model.ts`（预设 `:8`、形状 `:55`、状态色 `:96`、大图截断 `:156,203`、迭代列布局 `:251`）
- 画布渲染：`lib/components/domain/GraphCanvas.svelte`（cytoscape `:285`、交互 `:358`、sync `:175`）
- 图浏览器：`lib/components/domain/GraphExplorer.svelte`（过滤 `:87`、叠加 `:237`）
- 工作流详情：`routes/workflows/[id]/+page.svelte`（编辑禁用 `:426`、邻居展开 `:220`、分析叠加 `:147`）
- 模板编辑：`routes/templates/+page.svelte`（JSON/Form `:197`、校验 `:237`）、`lib/components/domain/JsonEditor.svelte`
- 运行状态：`lib/api/sse.ts`（`:46,89`）、`lib/api/stream.ts`、`lib/stores/stream-run.svelte.ts`（`:4,25,67`）、`lib/types/models.ts`（Execution `:8`、ExecutionDetail `:34`、WorkflowGraph `:64`）
- Agent 对话：`routes/chat/+page.svelte`、`lib/components/chat/*`、`lib/components/domain/SessionInspector.svelte`、`ToolCallCard.svelte`

**n8n（本地 `/workspace/n8n`，v2.41.0）**
- 映射层：`packages/frontend/editor-ui/src/features/workflows/canvas/composables/useCanvasMapping.ts`（状态优先级 `:38`、节点映射 `:141`、边状态 `:212`、分组重映射 `:202`）
- 类型：`.../canvas/canvas.types.ts`（渲染类型 `:58`、连接模式 `:31`）
- 布局：`.../canvas/composables/useCanvasLayout.ts`（dagre `:1`、连通子图 `:898`、AI 子图 `:686`、整组单元 `:289`、网格吸附 `:130`）
- 编排/渲染：`.../canvas/components/WorkflowCanvas.vue`、`.../canvas/Canvas.vue`（受控模式/单一数据源）
- 协同：`packages/frontend/editor-ui/src/features/collaboration/`（single-write 写锁）

---

## 6. 给开发者的几点提醒（基于代码事实）

1. **不要从零造编辑器轮子**：wf-agent 已有 `syncElements`（`GraphCanvas.svelte:175`）与 `display-model` 单一视觉层，补编辑器时只需在这两个地基上加「受控数据源 + 意图事件」，不要另起一套状态。
2. **先上图、再补编辑**：执行态上图（P0 第二项）几乎只改 `display-model` + 一个把 `ExecutionDetail` 喂进投影的桥，性价比最高，且能立刻让现有 SSE 数据产生可视价值。
3. **cytoscape 与 Vue Flow 能力对齐**：n8n 的「手柄连线 / 节点内嵌参数面板（NDV）」在 cytoscape 里需用 `cy.node` 的 overlay / 自定义事件模拟；建议先实现「拖拽移动 + 框选删除 + 双击空白建节点」，连线手柄作为第二阶段。
4. **模板与画布打通**：模板库 `saveTemplate` 后，应能在 `workflows/[id]` 直接打开渲染——避免 JSON 编辑与画布查看长期割裂（2.3.4）。
