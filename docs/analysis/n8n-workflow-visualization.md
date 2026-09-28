# n8n 工作流可视化功能架构分析

> 分析对象：n8n 仓库 `packages/frontend/editor-ui`（`n8n-editor-ui`，版本 `2.41.0`）
> 仓库来源：经 `git clone https://gh-proxy.com/https://github.com/n8n-io/n8n.git` 代理拉取至 `/workspace/n8n`（29641 个文件）
> 聚焦范围：**工作流的可视化编辑**——节点画布、节点/连线渲染、交互、执行态着色、自动布局、分组与协同编辑。
> 所有行号均针对 `packages/frontend/editor-ui/src/...` 下真实源码，可直接核对。

---

## 0. 速览

| 维度 | 结论 |
|------|------|
| 画布引擎 | **Vue Flow**（`@vue-flow/core`，另用 `@vue-flow/minimap` 小地图） |
| 前端框架 | Vue 3 + Vite + Pinia + Tailwind/SCSS |
| 画布代码位置 | `src/features/workflows/canvas/`（V2 画布，代码注释明确标注 "Canvas V2 Only"） |
| 核心目录 | `Canvas.vue`（渲染层）、`components/WorkflowCanvas.vue`（编排层）、`composables/`、`components/elements/` |
| 状态源泉 | `workflowDocumentStore` + `useWorkflowDocumentRenderData()` 产出的 `CanvasRenderData` |
| 自动布局 | `@dagrejs/dagre` 有向图布局（`useCanvasLayout.ts`） |
| 节点渲染类型 | `Default` / `StickyNote` / `AddNodes` / `ChoicePrompt` / `Agent`（`CanvasNodeRenderType`） |
| 协同编辑 | `features/collaboration` + WebSocket push（`pushConnectionStore`），含写锁 |

---

## 1. 可视化在 n8n 中的位置

n8n 的可视化编辑器是一套**纯前端 SPA**，由 `n8n`(cli) 通过 Express 在 `/` 托管（`dist/`）。其代码按 **feature-based** 组织（`src/features/*`），工作流可视化集中在：

```
src/features/workflows/
├── canvas/                 ← 画布核心（本文重点）
│   ├── Canvas.vue          ← 渲染/交互层（宿主 <VueFlow>）
│   ├── components/
│   │   ├── WorkflowCanvas.vue   ← 编排层（取数+映射+分组/执行态）
│   │   └── elements/            ← 可视原子（nodes/edges/handles/...）
│   ├── composables/        ← 映射、布局、遍历、分组、视口等逻辑
│   ├── canvas.types.ts     ← 可视化类型体系
│   └── canvas.utils.ts     ← 渲染数据工具
├── components/             ← 工作流级别 UI（激活弹窗、发布、分享等）
├── composables/           ← 工作流级 composables（如节点连接校验）
├── readyToRun/ templates/ workflowDiff/ workflowHistory/  ← 周边特性
```

与之协作的是 `src/app/stores/workflowDocument*`（文档状态 + 渲染数据）及 `src/features/collaboration`（协同）。

---

## 2. 架构分层与数据流

### 2.1 三层结构

```
┌─ WorkflowCanvas.vue (编排层 / "智能画布") ───────────────────────┐
│  · 从 workflowDocumentStore 读取 nodes/connections/groups        │
│  · useWorkflowDocumentRenderData → renderData (执行态/校验/标签)  │
│  · useCanvasMapping → 映射为 VueFlow 的 nodes/edges              │
│  · 管理分组视图、执行态、协同态、viewport 自适应                 │
└───────────────────────────┬──────────────────────────────────────┘
                            │ 传入 :nodes / :connections / :render-data
┌─ Canvas.vue (渲染交互层) ──▼─────────────────────────────────────┐
│  <VueFlow> 宿主：节点/边/背景/手柄/小地图的实际渲染               │
│  · 平移/缩放/选择/框选、快捷键体系、上下文菜单、拖拽连线         │
│  · 通过大量 emit 把交互回传给外层（update:node:position 等）     │
└───────────────────────────┬──────────────────────────────────────┘
                            │ #node-canvas-node / #edge-canvas-edge 插槽
┌─ elements/* (可视原子) ────▼─────────────────────────────────────┐
│  nodes/CanvasNode + render-types/*   edges/CanvasEdge            │
│  handles/CanvasHandleRenderer        background/  groups/         │
│  selection/   buttons/(运行/停止/清除/聊天)                      │
└──────────────────────────────────────────────────────────────────┘
```

### 2.2 数据流（从模型到像素）

```
workflowDocumentStore (nodes, connections, groups, pinData, validation...)
        │  useWorkflowDocumentRenderData(documentId)
        ▼
CanvasRenderData  ── 按节点 id 的 Map<id, ComputedRef<T>>：
   · 端口映射 nodeInputsByNodeId / nodeOutputsByNodeId
   · 执行态 executionStatusByNodeId / executionRunDataByNodeId / running/waiting/...
   · 校验/标签 subtitleByNodeId / validationErrorsByNodeId / renderTypeByNodeId
        │  useCanvasMapping({ nodes, connections, renderData, allGroups, nodeGroupView })
        ▼
CanvasNode[]  +  CanvasConnection[]   （VueFlow 形状）
        │  WorkflowCanvas 透传给 <Canvas>
        ▼
<VueFlow :nodes :edges>  →  elements 模板渲染节点/边
```

> 关键点：`useWorkflowDocumentRenderData` 是**可视化状态的唯一源头**（`useWorkflowDocumentRenderData.ts:24-58` 的注释明确其为 "single object combining ... canvas-shaped outputs"）。所有着色、标签、端口都由它派生，canvas 组件只做"消费"。这种**单向数据流**使执行状态、校验、协作者光标都能以统一方式注入画布。

---

## 3. 渲染层 `Canvas.vue`：`<VueFlow>` 宿主

`Canvas.vue` 是整个画布与 VueFlow 的胶水层（`components/Canvas.vue`）。

- **引入 VueFlow**：`import { ..., VueFlow, useVueFlow } from '@vue-flow/core'`（`:67`），小地图 `import { MiniMap } from '@vue-flow/minimap'`（`:68`）。
- **宿主标签**：模板中 `<VueFlow :id :nodes :edges ...>`（`:1899`），并关闭 VueFlow 自带变更应用、开启网格吸附：
  - `:apply-changes="false"`（`:1905`）—— 节点移动等**不**直接改 VueFlow 内部状态，而是经 `emit('update:nodes:position')` 回到文档 store，保证单一数据源。
  - `snap-to-grid` + `:snap-grid="[GRID_SIZE, GRID_SIZE]"`（`:1910-1911`），`GRID_SIZE` 来自 `nodeViewUtils`。
  - `:min-zoom="0"`、`:max-zoom`（默认 4，实验性 zoom 模式更高）（`:1912-1913`）。
- **自定义元素模板**：通过 VueFlow 的插槽机制注册可视组件：
  - `#node-canvas-node` → `<Node>`（`CanvasNode.vue`，`:1959`）
  - `#node-canvas-node-group` → `<CanvasNodeGroupTitleBar>`（`:1940`）
  - `#edge-canvas-edge` → `<Edge>`（`CanvasEdge.vue`，`:1995`）
- **事件桥接**：`@connect-start/@connect/@connect-end`（连线）、`@node-drag-*`（拖动）、`@node-click`、`@pane-click`、`@selection-*`（框选）、`@drop`（拖放节点）、`@viewport-change`（视口）（`:1920-1938`）。
- **回传事件**：Canvas 通过约 40 个 `emit`（`:119-180`）把交互上报，例如 `update:nodes:position`、`create:connection`、`run:workflow`、`extract-workflow`、`tidy-up`，由外层（工作流视图/store）落实持久化与业务动作。

---

## 4. 编排层 `WorkflowCanvas.vue`：把文档变成画布

`components/WorkflowCanvas.vue` 是"智能"外层（约 344 行 `<script>`）。

- **取数**：`useWorkflowDocumentRenderData(documentId)` 产出 `renderData`（`WorkflowCanvas.vue:89`），并在 documentId 变化时重建 effectScope（`:83-94`，注释强调该 composable 有副作用，必须"一次每文档"，不可放进 reactive computed）。
- **映射**：`useCanvasMapping({ nodes, connections, renderData, allGroups, nodeGroupView })`（`:136-149`）得到 `mappedWorkflowNodes` 与 `mappedConnections`。
- **分组视图**：`useCanvasNodeGroupView` / `useCanvasNodeGroupDescriptionVisibility` 管理折叠、描述可见性（`:105-116`），并 `provide` 给子组件（`:208-215`）。
- **执行中节流**：`mappedNodesThrottled = throttledRef(mappedNodes, 200)`（`:228`）——执行态高频更新时降频，避免重渲卡顿。
- **视口自适应**：节点初始化后 `fitView`（`onNodesInitialized`，`:218-226`）；`ensureNodesAreVisible` 通过 `getRectOfNodes` + `fitBounds` 把新增/选中节点带入视野（`:234-297`）。

---

## 5. 节点可视化（`CanvasNode` 与 render-types）

### 5.1 渲染类型体系（`canvas.types.ts:58-64`）

```ts
export const enum CanvasNodeRenderType {
  Default = 'default',
  StickyNote = 'n8n-nodes-base.stickyNote',
  AddNodes = 'n8n-nodes-internal.addNodes',
  ChoicePrompt = 'n8n-nodes-internal.choicePrompt',
  Agent = 'n8n-nodes-base.messageAnAgent',
}
```

对应 `components/elements/nodes/render-types/` 下各自的 `.vue`：`CanvasNodeDefault`、`CanvasNodeStickyNote`、`CanvasNodeAddNodes`、`CanvasNodeChoicePrompt`、`CanvasNodeAgent`。`CanvasNode.vue` 是统一外壳，根据 `data.render.type` 选择内部渲染器。

### 5.2 节点外壳构成（`CanvasNode.vue`）

一个节点的可视结构由四部分组成（模板 `:378-463`）：

1. **输出/输入手柄**（`CanvasHandleRenderer`，`:389-417`）：`mappedOutputs`/`mappedInputs` 由端口映射函数生成（`:152-247`）。每个端口带 `handleId`（由 `createCanvasConnectionHandleString({mode,type,index})` 编码，`:193`）、连接数、连接中状态，并按 `Position`（左入/右出/上/下非主流）与索引计算偏移（`:204-211`）。
2. **工具栏**（`CanvasNodeToolbar`，`:423`）：删除/停用/运行/激活/右键/聚焦/加 AI/加聊天；hover 或 focus-within 时淡入（`:466-480`）。
3. **主渲染器**（`CanvasNodeRenderer`，`:441`）：节点图标、标题、副标题、状态图标。
4. **触发器标识**（`CanvasNodeTrigger`，`:451`）：仅 `render.options.trigger` 为真的 `Default` 节点显示（`:452-454`），用于悬停时在其附近显示 "execute" 按钮（见 `WorkflowCanvas.vue:678` 的 `hoveredTriggerNode`）。

### 5.3 节点状态着色（视觉语言）

`classes` computed（`CanvasNode.vue:109-118`）根据 `data` 切换 CSS 类：

| 类名 | 触发条件 | 语义 |
|------|----------|------|
| `selected` | `props.selected` | 选中 |
| `hovered` | `props.hovered` | 悬停高亮 |
| `highlighted` | setupPanel 高亮 | 引导/聚焦高亮 |
| `running` | `execution.running \|\| waitingForNext` | 执行中 |
| `waiting` | `execution.waiting \|\| status==='waiting'` | 等待（如 Wait 节点） |
| `disabled` | `data.disabled` | 已停用（变灰） |

此外 `dirtiness`（`CanvasNodeDirtiness`，`canvas.types.ts:68-73`：参数变更/连线变更/pin 变更/上游脏）由 renderData 注入，用于"运行后改了参数"的警示标记。

---

## 6. 连线可视化（`CanvasEdge`）

### 6.1 连接类型与外观

连线类型来源于 `n8n-workflow` 的 `NodeConnectionTypes`：

- **Main**（主流数据）：实线 + 箭头（`markerEnd: MarkerType.ArrowClosed`）。
- **AI 工具/记忆/嵌入等非主流连接**：虚线（`stroke-dasharray: '5,6'`，`CanvasEdge.vue:67-70`）。

### 6.2 状态着色（执行反馈）

`getConnectionStatus`（`useCanvasMapping.ts:38-241`）定义**状态优先级** `['running','pinned','error','success']`，取首个命中：

| 状态 | 含义 | 边颜色 |
|------|------|--------|
| `running` | 源节点运行中且无输出 | 运行中高亮 |
| `pinned` | 源有 pin 数据 | `var(--color--secondary)`（`CanvasEdge.vue:113-114`） |
| `error` | 源有 issues | 错误色 |
| `success` | 有输出且目标已执行 | `var(--color--success)`（绿） |

非主流连接为"被动"，仅当目标也执行才计为 success（`:228-230`）。

### 6.3 标签与箭头

- **标签**（`getConnectionLabel`，`useCanvasMapping.ts:277-333`）：显示输出条数（如 "12 items / 12 items total"）或 pin 数据计数；AI 工具连接按目标计数。
- **箭头**：`CanvasArrowHeadMarker.vue` + `markerEnd: url(#custom-arrow-head)`（`Canvas.vue:1999`）。
- **悬停/置顶**：边悬停时 `bring-to-front`（`CanvasEdge.vue:72-76`）并延迟 600ms 收起工具栏（`:42-59`），避免误触。
- **缩放自适应**：`useZoomAdjustedValues` 计算边亮度/描边宽度，配合 `--canvas-zoom-compensation-factor` 保证缩放后视觉一致（`:123-134`）。

---

## 7. 画布交互体系

### 7.1 平移 / 缩放 / 选择

- **平移模式**：空格或鼠标中键（`panningKeyCode`，`Canvas.vue:320-321`），`onKeyDown/onKeyUp` 切换（`panningMouseButton`）（`:337-341`）。
- **选择模式**：Shift（桌面）/ 默认（移动端）（`:322`）；橡皮筋框选由 VueFlow 提供，`CanvasSelectionToolbar` 提供批量操作（`:93`）。
- **网格吸附**：`snap-to-grid` + `GRID_SIZE`（`:1910-1911`）。
- **小地图**：仅在平移时出现，1s 后淡出（`Canvas.vue:1705-1750`，`isMinimapVisible`）。
- **fitView / zoom**：`0` 复位、`1` 适配、滚轮缩放；处理后台标签页 `offsetWidth=0` 导致 fitView 错误的边界情况（`:1769-1780`）。

### 7.2 快捷键体系（节选，`Canvas.vue:512-615`）

| 按键 | 动作 |
|------|------|
| `Ctrl/Cmd + Enter` | 运行工作流 |
| `Ctrl/Cmd + A` | 全选 |
| `Ctrl/Cmd + C / X / D` | 复制 / 剪切 / 复制 |
| `Delete / Backspace` | 删除选中 |
| `F2` / `Space` | 重命名节点 / 组 |
| `N` | 添加节点 |
| `Shift + S` | 添加便签 |
| `Shift + F` | 聚焦面板 |
| `Shift + Alt + T` | **整理布局（Tidy Up）** |
| `Alt + X` | 抽取为子工作流 |
| `Ctrl/Cmd + G` / `Ctrl/Cmd + Shift + G` | 成组 / 解组 |
| `Alt + G` / `Shift + Alt + G` | 展开 / 折叠所有组 |
| 方向键 | 选择上/下/左/右相邻节点 |
| `Shift + 方向键` | 选择上游 / 下游节点 |
| `Z` | 实验性 zoom 模式 |

（只读画布禁用写操作，但保留缩放、分组折叠等视图操作，`:550-553`。）

### 7.3 拖拽、连线、拖放

- **节点拖动**：`@node-drag-start/drag/stop` → `groupDrag` 处理（含组成员联动、折叠组"推动"效果），停下后 `commitManualNodePositions` 回写 store（`:825-836`）。
- **连线**：`@connect-start/connect/connect-end` → 通过 `create:connection*` 事件上报，由外层建立连接（`:1295-1318`）。
- **拖放创建**：`@drop` 计算投影坐标 `project(...)` 后 `emit('drag-and-drop')`（`:1702-1706`）。
- **右键菜单**：`useContextMenu` 统一处理节点/组/选择/空白的右键菜单（`:1508-1569`）。

---

## 8. 分组（Groups）

分组是可视化层的一等公民，由 `useCanvasNodeGroup*` 系列 composables 支撑：

- **折叠/展开**：组以 `canvas-node-group` 类型节点（`group:${id}`，`canvas.types.ts:165-166`）表示；折叠时成员 `hidden: true`（`useCanvasMapping.ts:195`），仅显示标题条（`CanvasNodeGroupTitleBar.vue`）。
- **整体选择**：组作为单一单元选择，标题条与成员选区同步（`useCanvasNodeGroupSelection`，`Canvas.vue:802-809`）；选择框需包裹整个组帧（`:811-823`）。
- **拖拽联动**：拖动组或展开组会用"推力"（`push`）把重叠节点推开，拖动结束 `commitPushedPositionsForSourceGroups` 把被推位置固化进文档（`:776-787`）。
- **布局影响**：自动布局时整组作为 dagre 的一个 box（见 §9）。

---

## 9. 自动布局（Tidy Up / 整理）

`composables/useCanvasLayout.ts` 基于 **`@dagrejs/dagre`** 有向图布局算法（`:1` import）。

- **入口**：`layout(target: 'selection'|'all', options)`（`:1038`），由快捷键 `Shift+Alt+T`、按钮、右键菜单、命令栏触发（`CanvasLayoutSource` 含 `keyboard-shortcut`/`context-menu`/`command-bar`/`import-workflow-data`/`builder-update`，`:41-47`）。
- **布局策略**（`placeNodes`，`:885`）：
  1. 用 `dagre.graphlib.alg.components` 把工作流拆分为**连通子图**，子图左右并列（`:898`）。
  2. **AI 子图**：识别 AI 父节点（`isAiParentNode`），把其配置子节点（`getAllConnectedAiConfigNodes`，`:819`）单独做 **Top-Bottom** 布局后折叠回父节点 box（`:686-716, :907-932`）。
  3. **分组单元**：整组作为单个 box 进入 dagre，组内容先在组内独立布局（`layoutGroupContent`，`:359`），帧再包住结果（`:288-352`）。
  4. **便签吸附**：覆盖某组的便签随组移动并重定位到底部对齐（`placeStickies`，`:843`；`attachCoveringStickies`，`:244`）。
- **网格对齐**：结果按连接手柄（非左上角）`snapToGrid`（`:1186-1198`），保证连线轴对齐。
- **边界处理**：大量 `BoundingBox`/`isCoveredBy`/`intersects` 工具处理重叠、覆盖与冲突检测（`:718-817`），保证布局后无节点重叠。

> 该实现高度工程化——处理折叠组 chip、展开组 frame、AI 竖向子图、便签覆盖、sticky 快照等大量边界情况，是画布可视化中复杂度最高的模块之一。

---

## 10. 执行态可视化

执行态通过 `CanvasRenderData` 注入，而非单独轮询：

- **节点级**：`getNodeExecutionSnapshot`（`useCanvasMapping.ts:73-98`）聚合 running/waiting/hasExecutionError/hasValidationError/iterations/dirty，驱动 `CanvasNode` 的 `running`/`waiting` 类与状态图标。
- **边级**：§6.2 的状态优先级驱动边颜色与标签条数。
- **组级**：`aggregateGroupExecution(group.nodeIds, getNodeExecutionSnapshot)`（`WorkflowCanvas.vue:158`）把组内节点状态聚合为组的状态（`GroupExecutionStatus`，`canvas.types.ts:187-193`：waiting/running/error/issues/warning/success），`groupExpansionMode: 'errored'` 时自动展开含错误的组（`:151-172`）。
- **高频节流**：执行中 `mappedNodesThrottled`（200ms）防止每 tick 重渲（`:228-229`）。

---

## 11. 协同编辑（Collaboration）

`features/collaboration/collaboration/` 提供多人同时编辑同一工作流的可视化。

- **在线协作者跟踪**：`useCollaborationStore`（`collaboration.store.ts:28`）注释明确 "tracking active users ... who is collaboratively viewing/editing"。通过 `pushConnectionStore`（WebSocket）接收其他人的在线/活跃状态（`collaboration.store.ts:53`）。
- **写锁（single-write mode）**：含心跳 `WRITE_LOCK_HEARTBEAT_INTERVAL=30s`、锁状态轮询 `20s`、无活动超时 `20s`（`:39-43`），防止并发写入冲突。
- **UI**：`CollaborationPane.vue` 展示协作者（头像/光标/选区）；编辑器画布本身通过 `nodeIdToGroupId`、选区同步等与其他协作者状态联动（协同光标/选区渲染散落在 canvas 交互与 selection composables 中）。

---

## 12. 设计要点与评价

1. **单一数据源 + 单向数据流**：文档状态（`workflowDocumentStore`）是唯一真相，VueFlow 的 `apply-changes=false`，所有变更经 `emit → store` 回写；`useWorkflowDocumentRenderData` 统一派生"可视化状态"，使执行/校验/协同能以一致方式注入。
2. **分层清晰**：编排层（`WorkflowCanvas`）与渲染层（`Canvas`/`elements`）分离，映射逻辑（`useCanvasMapping`）与布局逻辑（`useCanvasLayout`）皆为**纯函数式 composable**，可独立测试（配套大量 `.test.ts`）。
3. **类型驱动的渲染多态**：`CanvasNodeRenderType` 枚举 + 插槽机制，让 Default/StickyNote/Agent 等节点共享外壳、各自渲染，扩展新节点类型成本低。
4. **性能意识**：执行态 `throttledRef` 降频、按节点的 `effectScope` 懒求值渲染数据（避免全量重算）、分组折叠隐藏成员降低 DOM 量。
5. **可视化语言统一**：节点/边/组共用一套状态类（selected/hovered/running/waiting/disabled）与 CSS 变量（`--canvas-edge--color` 等），主题与深浅色（`light-dark()`）一致。
6. **工程复杂度担当**：`useCanvasLayout` 是画布里最复杂的算法模块，需正确处理组、AI 子图、便签、折叠态等组合，是"整理布局"功能可靠性的核心。

---

### 附：关键文件与行号索引

| 关注点 | 文件:行 |
|--------|---------|
| `<VueFlow>` 宿主 / 事件桥接 | `features/workflows/canvas/components/Canvas.vue:1899, 1920-1938` |
| 编排层取数+映射+分组 | `features/workflows/canvas/components/WorkflowCanvas.vue:89, 136-149, 208-215` |
| 文档→画布映射 | `features/workflows/canvas/composables/useCanvasMapping.ts:38, 141, 200, 277` |
| 节点类型体系 | `features/workflows/canvas/canvas.types.ts:58-64, 129-161, 230-239` |
| 节点外壳与端口 | `features/workflows/canvas/components/elements/nodes/CanvasNode.vue:109-118, 152-247, 378-463` |
| 边状态/着色/标签 | `features/workflows/canvas/components/elements/edges/CanvasEdge.vue:67-134` |
| 自动布局（dagre） | `features/workflows/canvas/composables/useCanvasLayout.ts:1, 885, 1038` |
| 渲染数据来源 | `app/stores/workflowDocument/useWorkflowDocumentRenderData.ts:24-58` |
| 协同编辑 | `features/collaboration/collaboration/collaboration.store.ts:28-43` |
| 快捷键体系 | `features/workflows/canvas/components/Canvas.vue:512-615` |
