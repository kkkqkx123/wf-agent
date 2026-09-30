# 前端可视化 P0 阶段细化方案与落地说明

> 依据 `docs/ref/frontend-viz/`（尤其是 `06-可借鉴Dify与n8n的设计.md` 第 6 节优先级清单）编写。
> P0 共两项：**节点创建/搜索面板**（源自 n8n + Dify）、**端口类型校验**（源自 n8n）。
> 本文先给细化方案，再记录实际落地结果（含文件与行号），最后给出全部决策结论。

---

## 0. 目标与范围

| 编号 | P0 项             | 来源       | 目标收益       | 本次状态                 |
| ---- | ----------------- | ---------- | -------------- | ------------------------ |
| P0-A | 节点创建/搜索面板 | n8n + Dify | 编辑可用性跃升 | 已落地                   |
| P0-B | 端口类型校验      | n8n        | 杜绝语义错边   | 已落地（按后端规则收窄） |

范围内只做这两项；P1（逐节点追踪卡、Execution 筛选/重跑、`@wf-agent/ui` 抽包）及 P2/P3 不在本阶段。

本轮额外完成一项**前后端一致性修复**：初版实现里有若干基于猜测的字段名与节点类型字面量，以及后端根本不存在的字段。这些都已改为对齐后端契约，容错读取逻辑全部删除（详见第 2、3 节）。

---

## 1. 事实基线（后端契约，逐条带出处）

### 1.1 节点类型是封闭枚举 + 插件扩展位，只有空值会被拒绝

`crates/foundation/wf-types/src/node/static.rs:13-38` 定义 `StaticNodeType`，序列化为裸 SCREAMING_SNAKE_CASE 字符串；`static.rs:42-65` 的 `ALL` 给出全部 22 个内置名：

```
START  END  EMBED_START  EMBED_END  VARIABLE  FORK  JOIN  SYNC  SUBGRAPH
EMBED_GRAPH  SCRIPT  INTERACTIVE_SCRIPT  LLM  TOOL_VISIBILITY  USER_INTERACTION
ROUTE  CONTEXT_PROCESSOR  LOOP_START  LOOP_END  AGENT_LOOP
START_FROM_MESSAGE  CONTINUE_FROM_MESSAGE
```

第 23 个变体 `Custom(String)`（`static.rs:36-37`）承载插件注册的节点类型。改动前 `static.rs:157-167` 的反序列化对未知值直接报错，导致 `Custom` 永远无法产生；本次修复后（见 1.6）规则是：**内置名大小写不敏感命中，其余非空字符串原样保留为 `Custom`，只有空/全空白被拒**。

草稿保存接口 `crates/app/wf-server/src/api/workflow/drafts.rs:58-61` 用 `Json<wf_api::WorkflowDefinition>` 强类型接收，因此空 `node_type` 会让保存直接失败。

前端发出 `node_type` 的位置是 `apps/web-app/src/lib/graph/edit-store.svelte.ts:255-272` 的 `toDraftDefinition`（`node_type: node.kind`）。

### 1.6 插件节点类型链路（改动前是断的，本次打通）

后端的插件节点类型机制是完整的，但被反序列化与三处配置层校验掐死：

| 环节           | 位置                                            | 改动前                     | 本次                                                    |
| -------------- | ----------------------------------------------- | -------------------------- | ------------------------------------------------------- |
| 类型解析       | `wf-types/src/node/static.rs:157-170`           | 未知值报错                 | 未知非空值 → `Custom(原样)`                             |
| 工作流配置转换 | `wf-config/src/processor/workflow.rs:229-238`   | `Err("unknown node type")` | `from_str_ci` 兜底 → `Custom`；空白报 `blank node_type` |
| 节点配置校验   | `wf-config/src/processor/node_config.rs:31-69`  | 未知类型产出 issue         | 无内置 schema 可查，直接放行                            |
| 节点模板校验   | `wf-config/src/processor/node_template.rs:9-24` | 不在 `ALL` 内即报错        | 只拒绝空白名                                            |
| 引擎节点校验   | `wf-workflow/src/node_validation.rs`            | 断言"unknown node type"    | 断言插件类型可入图                                      |
| API 转换       | `wf-api/src/infra/config.rs`                    | 断言转换失败               | 断言转出 `Custom("LLMM")`                               |

需要指出的是，后两处**原本就是按 `Custom` 可达来写的**，只是从未被触发过：

- `crates/engine/wf-workflow/src/coordinator/workflow/routing.rs:40` 已经是 `other => Ok(StaticNodeType::Custom(other.to_string()))`；
- `crates/app/wf-api/src/infra/context.rs:323-335` 的 `plugin_node_type()` 依赖未知名能反序列化成功才能查插件注册表。

即本次不是新增能力，而是**把既有意图从死代码变成活代码**。注册与否的判定留在运行期：`wf-plugin` 的 `ContributionType::NodeType` / `node_type_registry`（`crates/infra/wf-plugin/src/contributions/manager.rs:30,114`）负责解析 handler；`wf-workflow` 不依赖 `wf-plugin`，因此配置层无法也不应该去查注册表。

### 1.2 端口约束只有四条边界规则

`crates/engine/wf-workflow/src/validation/node_types.rs:14-54` 的 `validate_start_end_topology` 是图规则的权威来源，内容只有四条：

| 节点类型                | 约束       |
| ----------------------- | ---------- |
| `START`                 | 不能有入边 |
| `END`                   | 不能有出边 |
| `START_FROM_MESSAGE`    | 不能有入边 |
| `CONTINUE_FROM_MESSAGE` | 不能有出边 |

`node_types.rs:64-68` 把这四个类型作为边界节点排除在"孤立节点"检查之外。除此之外**后端没有任何端口类型兼容性规则**。

补充事实：后端 `EMBED_START` / `EMBED_END` 不在这四条规则内，属普通节点，可以双向连线。

### 1.3 边的模型没有端口字段

`crates/foundation/wf-types/src/workflow/edge.rs:28-46` 的 `Edge` 只有 `id / source_node_id / target_node_id / type / condition / label / description / weight / error_route / metadata`，**没有 source_port / target_port**。前端保存草稿时也不发送端口（`edit-store.svelte.ts:264-270`）。

### 1.4 节点模板接口的字段是确定的

`crates/app/wf-api/src/template/node_template.rs:80-88` 的 `NodeTemplateSummary` 序列化后只有：

```
id: string   name: string   node_type: string
description?: string（None 时字段省略）   updated_at: number
```

存储模型 `crates/foundation/wf-types/src/storage/node_template.rs:4-11` 的 `NodeTemplateStorageMetadata` 为 `id / name / node_type / description / created_at / updated_at`。**两者都没有 `category` 和 `tags`**。

配置样例 `configs/node-templates/code.template.example.toml` 里出现的 `[metadata] category/tags` 属于运行时类型 `wf_types::workflow::NodeTemplate`（含 `default_config`）的载荷，HTTP 层的 summary 不暴露它们。该样例的 `node_type = "CUSTOM"` 不是合法 `StaticNodeType`。

### 1.5 改动前的代码问题清单

| 问题                                     | 位置                                                   | 后果                                                                                                                       |
| ---------------------------------------- | ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| 新建节点固定 `kind: 'STEP'`              | `routes/workflows/[id]/+page.svelte`（改动前）         | `'STEP'` 不在枚举内，保存草稿必失败                                                                                        |
| 端口校验只做拓扑、不校验边界             | `lib/graph/canvas-connect.ts:18-36`（改动前）          | 可画出 START 入边 / END 出边，要到服务端才报错                                                                             |
| 渲染用 kind 集合含大量后端不存在的字面量 | `lib/graph/display-model.ts:42-61`、`96-114`（改动前） | `trigger` / `note` / `agent` / `subagent` / `webhook` 等永远命中不到；`renderKind` 还带一个只服务于 `error` 的 preset 分支 |
| 无节点选择面板，模板只是注册表           | —                                                      | 对应 `06` §2.2 的可用性缺口                                                                                                |

---

## 2. 差距分析（对照 Dify / n8n）

| 能力          | Dify                   | n8n                                          | 本项目改动前       | 本次补齐                               |
| ------------- | ---------------------- | -------------------------------------------- | ------------------ | -------------------------------------- |
| 端口/类型校验 | 连线按状态着色，弱类型 | `isValidConnection` 按端口类型把关           | 仅拓扑校验         | 边界端口校验（P0-B，镜像后端四条规则） |
| 节点创建入口  | 节点市场/模板一键插入  | NodeCreator（搜索 + 拖拽 + 连线末端 + 替换） | 双击加空 STEP 节点 | Add Node 抽屉（P0-A）                  |
| 创建后落点    | 中点插入/模板插入      | 拖拽落点                                     | 双击坐标           | 双击坐标 / 视口中心                    |
| 模板复用      | 模板市场               | 节点库                                       | 注册表，不可视插入 | 抽屉选模板 + 模板页入队插入            |

---

## 3. 决策记录（原"待拍板开放点"全部拍板）

| #   | 议题                       | 决策                                                                                                                                      | 依据                                                                                |
| --- | -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| 1   | 输入端口是否细分类型       | **不细分**。删除 `PortKind` / `PORT_COMPATIBLE` 与 `flow/data/agent/trigger/note` 语义分类，只保留端口存在性判断                          | 后端只有 1.2 的四条边界规则，没有任何类型兼容约束；细分等于凭空发明规则             |
| 2   | 模板页插入的目标工作流选择 | **保持入队 → 跳 `/workflows` → 用户点开工作流 → 进 Edit 消费**。队列是内存态，刷新即丢                                                    | 引入"最近编辑工作流"要新增后端或跨页持久化，超出 P0；内存队列的丢失语义明确、可解释 |
| 3   | 模板 API 字段              | **对齐后端**：`NodeTemplateSummary` 只保留 `id / name / nodeType / description`；删除 `category` / `tags` 与 `node_type ?? nodeType` 容错 | 后端 summary 根本没有这两个字段（1.4）                                              |
| 4   | 双击空白的行为变更         | **双击空白打开抽屉**，抽屉首项是固定的 "Blank step"（`SCRIPT`）。不新增 Shift 快捷路径                                                    | 与原"双击加空节点"等价能力仍在（多一次回车）；单一路径降低维护面                    |
| 5   | 工具栏入口的默认坐标       | **用画布视口中心**：`GraphCanvas` 新增 `viewportCenter()`，`GraphExplorer` 在无双击坐标时取它                                             | `{x:0,y:0}` 常落在可视区外，用户看不到新节点                                        |
| 6   | 抽屉列表上限               | **保留 `limit: 200`**；`has_more` 为真时显示"仅显示第一页，请搜索过滤"；形状不符的行显示跳过数量                                          | 后端分页字段已给出 `has_more`，不做滚动加载即可诚实表达截断                         |
| 7   | 端口信息是否落库           | **不落库**，并回退 `sourcePort` / `targetPort` 的透传                                                                                     | 后端 `Edge` 无端口字段（1.3），透传只能成为死代码                                   |

**额外决策（一致性修复，原方案未覆盖）**

| #   | 议题                                  | 决策                                                                                                                                            |
| --- | ------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| 8   | 新建节点的默认 kind                   | 由非法的 `'STEP'` 改为 **`SCRIPT`**（`DEFAULT_NODE_TYPE`）；所有写入画布的 kind 都经 `parseStaticNodeType` 校验                                 |
| 9   | 模板 `node_type` 不是内置枚举值怎么办 | **不隐藏、不兜底转换、也不拒绝**：按 `normalizeNodeType` 规范化后正常插入，分组上打 `plugin` 标记并提示"需安装所属插件才可执行"。只有空名被丢弃 |
| 10  | trigger 模板能否插入画布              | **不能**。后端 `StaticNodeType` 无 TRIGGER，模板详情页的插入入口只对 `kind === 'node'` 且 `node_type` 非空时渲染                                |
| 11  | 后端返回行形状不符                    | **跳过并计数**，不做默认值填充。后端字段变更会以"缺失条目"暴露，而不是静默产生错误数据                                                          |

**第二批决策（后端 `Custom` 打通 + `renderKind` 清理）**

| #   | 议题                                            | 决策                                                                                                       | 依据                                                                                                                                                      |
| --- | ----------------------------------------------- | ---------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 12  | 后端 `Custom(String)` 是否该被解析出来          | **该**。反序列化拒绝空值，其余未知名原样进 `Custom`                                                        | `routing.rs:40` 与 `context.rs:323-335` 本就按 `Custom` 可达编写；`wf-plugin` 有完整的 `ContributionType::NodeType` 注册机制。不修 = 插件节点类型是死代码 |
| 13  | 配置层是否要校验"插件类型已注册"                | **不校验**。`wf-config` / `wf-workflow` 都拿不到插件注册表                                                 | `wf-workflow` 的 `Cargo.toml` 不依赖 `wf-plugin`；注册解析属于运行期 handler 查找的职责                                                                   |
| 14  | `wf-config` 里那份重复的 22 臂 `match` 怎么处理 | **删掉**，改为 `from_str_ci(...).unwrap_or_else(Custom)`                                                   | 同一规则原本有三份实现（`static.rs` / `workflow.rs` / `routing.rs`），且旧实现会把插件名转成大写（`git-clone` → `GIT-CLONE`），与原样往返的语义冲突       |
| 15  | `renderKind` 保留哪些渲染角色                   | **只保留 `terminal` / `decision` / `tool` / `step`**，删除 `trigger` / `note` / `agent`；签名去掉 `preset` | 后端没有 trigger/note/agent 节点类型；`preset` 分支只为 `error` 服务，而 `error` 是状态不是类型，状态着色由 `statusHex` 负责                              |
| 16  | `layeredPositions` 的便签停靠逻辑               | **删除**（约 40 行）。它只由 `note` 渲染角色驱动，后者已删除                                               | n8n 的 `StickyNote` 是画布元素（`CanvasNodeRenderType`）而不是节点类型，后端无对应建模；保留即死代码。若要便签，需后端先给出 NOTE 类型或独立画布元素接口  |

---

## 4. P0-B：端口校验方案（收窄为后端规则）

### 4.1 新增 `lib/graph/node-kind.ts`

单一来源，与后端 `StaticNodeType` 一一对应：

- `STATIC_NODE_TYPES`：22 个合法名，顺序与后端 `ALL` 一致
- `parseStaticNodeType(value)`：大小写不敏感精确匹配，未知返回 `null`（与后端 `from_str_ci` 语义相同）
- `DEFAULT_NODE_TYPE = 'SCRIPT'`
- `nodePorts(kind)`：返回 `{ acceptsInput, emitsOutput }`，`START` / `START_FROM_MESSAGE` 无输入，`END` / `CONTINUE_FROM_MESSAGE` 无输出，其余双向
- 未识别的 kind 按普通节点处理（有入有出），这样画布仍能连线，用户可在编辑区修正 kind

### 4.2 校验入口

`display-model.ts` 的 `connectByPort` 只做存在性判断，且**错误文案与后端一致**（`"START node cannot have incoming edges"` 等），让画布提示和服务端草稿报错读起来是同一句话。

调用点仍在 `canvas-connect.ts` 的 `describeConnectRejection` 末尾，**追加在既有拓扑校验之后**，保证 "Edge already exists." 等原提示优先级不变。

### 4.3 端口信息不再透传

`ConnectSpot` / `ConnectDrag` 恢复为无端口字段，`GraphCanvas` 的 `onconnect` 恢复为两参。画布只额外传 `sourceKind` / `targetKind`（取自 `kindById`）供校验使用，不向业务层泄漏端口概念。

---

## 5. P0-A：节点创建/搜索面板方案

### 5.1 Add Node 抽屉

`lib/components/domain/AddNodeDrawer.svelte`，外壳复用 `Dialog.svelte`，交互模式复用 `CommandPalette.svelte`（搜索框 + 分组 + ↑↓/Enter 导航 + hover 同步）。

列表结构：

1. 固定首组 **Blank** —— 只有一项 "Blank step"，落为 `SCRIPT` 节点；
2. 其余按**解析后的节点类型**分组，组名即 `SCRIPT` / `LLM` / …；
3. 模板 `node_type` 解析失败的条目不进入列表，底部显示被隐藏数量；后端返回行形状不符的显示跳过数量；`has_more` 为真时提示截断。

选中后回调 `onselect(nodeType, name)`，传的是**已解析的后端节点类型**，业务层不再做任何类型推断。

### 5.2 触发与落点

| 触发方式                         | 行为                                                          |
| -------------------------------- | ------------------------------------------------------------- |
| 双击空白画布                     | 传入双击坐标，落在该坐标                                      |
| 编辑工具栏「Add node」           | 无坐标，取 `viewportCenter()`（视口中心）                     |
| 模板详情页「Insert as \<KIND\>」 | 入队后跳 `/workflows`，进入目标工作流的 Edit tab 时消费并落点 |

### 5.3 跨页队列

`lib/stores/node-insert.svelte.ts` 存的是 `PendingNodeInsert { nodeType, name }`，而不是模板对象：类型解析在入队前完成，消费端零判断。队列为内存态，刷新即丢。

模板详情页的 `insertNodeType` 派生值直接决定按钮是否渲染以及按钮上显示的 kind。

---

## 6. 关键改动说明（不含完整代码）

- **节点类型契约**：新建 `lib/graph/node-kind.ts`，把后端枚举搬进前端并作为唯一判定入口。
- **端口校验**：`connectByPort` 由"语义类型兼容矩阵"改为"边界端口存在性"，文案对齐后端。
- **去容错**：`node-templates.ts` 用 `toSummary(row): NodeTemplateSummary | null` 显式判定形状，缺失即丢弃并计数；不再有 `??` 兜底链。
- **模板页插入**：`insertIntoCanvas` 只在 `insertNodeType` 非 null 时可用，`node_type` 从 `detail.raw.node_type` 直接读取（后端字段名确定），不做别名兼容。
- **落点改进**：`GraphCanvas` 新增 `viewportCenter()`，工具栏入口不再落在 `(0,0)`。

### 后端：`Custom` 链路打通

- `static.rs` 的 `Deserialize` 改为"空值报错、内置名优先、其余进 `Custom`"，并补 4 个用例覆盖大小写、往返、空值。
- `wf-config` 的三处校验从"拒绝未知名"改为"只在空白时报错"；其中 `parse_node_type` 的 22 臂 `match` 整段删除，统一走 `from_str_ci` + `Custom` 兜底。
- 三处陈旧断言（原本断言 `LLMM` 报 "unknown node type"）改为断言插件类型被保留/放行。

### 前端：随后端放开插件类型

- `node-kind.ts` 新增 `normalizeNodeType`（内置名规范化 + 插件名原样，空值返回 null）与 `isBuiltinNodeType`，作为"写回后端的值"的唯一入口。
- 类型链路从 `StaticNodeType` 放宽为 `string`：`onaddnode` / `PendingNodeInsert.nodeType` / `handleAddNode`。放宽的是**值的集合**，不是校验——空值仍然进不了画布。
- 抽屉不再隐藏非内置模板，改为分组打 `plugin` 标记；模板页插入按钮对任何非空 `node_type` 渲染。

### 前端：`renderKind` 只认后端类型

- 五个猜测集合（`TRIGGER_KINDS` / `NOTE_KINDS` / `AGENT_KINDS` / `TOOL_KINDS` / `TERMINAL_KINDS` / `ERROR_KINDS`）全部删除，换成三个 `Set<StaticNodeType>`：`terminal` = START/END/START_FROM_MESSAGE/CONTINUE_FROM_MESSAGE，`decision` = ROUTE，`tool` = LLM，其余一律 `step`。
- `renderKind` 去掉 `preset` 参数（唯一用到的分支是 `error`，而那是状态不是类型）；`nodeShape` 保留 `preset`，因为 `tool` 在 workflow 与非 workflow 下的形状差异是纯视觉选择。
- `layout.ts` 删除便签停靠分支与 `NOTE_GAP`。

---

## 7. 代码落点（实际行号）

### 7.1 P0-B

| 文件                                             | 行号              | 动作                                                                                                                                                                                                                                           |
| ------------------------------------------------ | ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lib/graph/node-kind.ts`                         | 全文新增（99 行） | `STATIC_NODE_TYPES`(10) / `StaticNodeType`(35) / `DEFAULT_NODE_TYPE`(38) / `parseStaticNodeType`(48) / `normalizeNodeType`(58) / `isBuiltinNodeType`(68) / `ENTRY_NODE_TYPES`(76) / `EXIT_NODE_TYPES`(85) / `NodePorts`(90) / `nodePorts`(101) |
| `lib/graph/display-model.ts`                     | 8                 | 引入 `node-kind`                                                                                                                                                                                                                               |
| 同上                                             | 431-452           | `ConnectByPortCheck` 收敛为两端 kind；`connectByPort` 改为边界端口判断                                                                                                                                                                         |
| `lib/graph/canvas-connect.ts`                    | 11-14             | `ConnectCheck` 新增可选 `sourceKind` / `targetKind`                                                                                                                                                                                            |
| 同上                                             | 37-41             | `describeConnectRejection` 末尾追加端口分支                                                                                                                                                                                                    |
| `lib/graph/canvas-model.ts`                      | 40-53             | 恢复为无端口字段的 `ConnectSpot` / `ConnectDrag`                                                                                                                                                                                               |
| `lib/components/domain/GraphCanvas.svelte`       | 170               | `kindById`（`nodes` → `kind`）                                                                                                                                                                                                                 |
| 同上                                             | 353-354 / 365-366 | `connectRejectReason` / `connectValid` 传入两端 kind                                                                                                                                                                                           |
| 同上                                             | 659-668           | 新增 `viewportCenter()`                                                                                                                                                                                                                        |
| 同上                                             | 98                | `onconnect` 保持两参（端口透传已回退）                                                                                                                                                                                                         |
| `lib/components/domain/GraphExplorer.svelte`     | 990-992           | `onconnect` 两参透传                                                                                                                                                                                                                           |
| `lib/components/domain/WorkflowEditPanel.svelte` | 35 / 141          | `onconnect` 两参透传                                                                                                                                                                                                                           |

### 7.2 P0-A

| 文件                                             | 行号            | 动作                                                                                                       |
| ------------------------------------------------ | --------------- | ---------------------------------------------------------------------------------------------------------- |
| `lib/services/node-templates.ts`                 | 全文重写        | `NodeTemplateSummary`(12) / `NodeTemplatePage`(20) / `toSummary`(32) / `listNodeTemplates`(52)，无容错字段 |
| `lib/components/domain/AddNodeDrawer.svelte`     | 全文重写        | `Choice`(26) / 可用性过滤(57-70) / 分组含 Blank 首组(91-116) / 隐藏与截断提示(221-232)                     |
| `lib/stores/node-insert.svelte.ts`               | 全文重写        | 存 `PendingNodeInsert { nodeType, name }`                                                                  |
| `lib/components/domain/GraphExplorer.svelte`     | 49 / 102-107    | 引入 `StaticNodeType`；`onaddnode` 改为 `(position, nodeType, name)`                                       |
| 同上                                             | 438-456         | `addNodeOpen` / `pendingAddPosition` / `openAddNode` / `handleAddNodeChoice`（无坐标时取视口中心）         |
| 同上                                             | 791             | 工具栏「Add node」按钮                                                                                     |
| 同上                                             | 988             | 双击空白 → `openAddNode(position)`                                                                         |
| 同上                                             | 1227-1232       | 渲染 `AddNodeDrawer`                                                                                       |
| 同上                                             | 829             | 编辑提示文案同步                                                                                           |
| `lib/components/domain/WorkflowEditPanel.svelte` | 9 / 29-34 / 139 | `onaddnode` 三参透传                                                                                       |
| `routes/workflows/[id]/+page.svelte`             | 49 / 111-114    | 引入类型；编辑 tab 载入后消费队列                                                                          |
| 同上                                             | 240-255         | `handleAddNode(position, nodeType, name)`                                                                  |
| `routes/templates/[kind]/[id]/+page.svelte`      | 47 / 573-581    | 引入 `parseStaticNodeType`；`insertNodeType` 派生值                                                        |
| 同上                                             | 589-598         | `insertIntoCanvas` 入队（含 `resolve()` 导航）                                                             |
| 同上                                             | 648-653         | 页头「Insert as \<KIND\>」按钮，仅节点模板且类型可解析时渲染                                               |

### 7.3 后端：`Custom` 链路

| 文件                                                    | 行号              | 动作                                                                                                                                                                                         |
| ------------------------------------------------------- | ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/foundation/wf-types/src/node/static.rs`         | 157-170           | `Deserialize` 改为"空值报错 + `from_str_ci` 优先 + `Custom` 兜底"                                                                                                                            |
| 同上                                                    | 184-231           | 新增 `mod tests`：`builtin_names_deserialize_case_insensitively`(189) / `unknown_names_become_custom_types`(198) / `custom_types_round_trip_verbatim`(206) / `empty_names_are_rejected`(215) |
| `crates/infra/wf-config/src/processor/workflow.rs`      | 229-238           | `parse_node_type` 删除 22 臂 `match`，改为 `from_str_ci` + `Custom`；空白名报 `blank node_type`                                                                                              |
| 同上                                                    | 410-421 / 423-435 | `test_transform_nodes_keeps_plugin_types` / `test_transform_nodes_rejects_empty_type` 取代 `test_transform_nodes_rejects_unknown_type`                                                       |
| `crates/infra/wf-config/src/processor/node_config.rs`   | 26-30 / 66-68     | 文档注释更新；`_ =>` 分支由产出 issue 改为 `Vec::new()`                                                                                                                                      |
| 同上                                                    | 342-350           | `plugin_node_types_pass_without_config_inspection` 取代 `unknown_node_type_is_rejected`                                                                                                      |
| `crates/infra/wf-config/src/processor/node_template.rs` | 7 / 13-22         | 移除 `StaticNodeType` 导入；`ALL` 白名单校验改为空白名校验                                                                                                                                   |
| 同上                                                    | 94-105            | `test_blank_node_type_rejected` / `test_plugin_node_type_accepted` 取代 `test_invalid_node_type_rejected`                                                                                    |
| `crates/engine/wf-workflow/src/node_validation.rs`      | 263-267           | `plugin_node_type_is_accepted` 取代 `unknown_node_type_is_rejected`                                                                                                                          |
| `crates/app/wf-api/src/infra/config.rs`                 | 203-215           | `keeps_plugin_node_type_on_transform` 取代 `rejects_unknown_node_type_on_transform`                                                                                                          |

### 7.4 前端：`renderKind` 与插件类型适配

| 文件                                             | 行号               | 动作                                                                                                                            |
| ------------------------------------------------ | ------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| `lib/graph/display-model.ts`                     | 8-12               | 导入增加 `type StaticNodeType`                                                                                                  |
| 同上                                             | 36-80              | `NodeRenderKind` 收敛为四值(49)；`TERMINAL_TYPES`(52) / `DECISION_TYPES`(60) / `TOOL_TYPES`(63)；`renderKind`(72) 去掉 `preset` |
| 同上                                             | 94-95              | `nodeShape` 改调 `renderKind(kind)`                                                                                             |
| `lib/graph/layout.ts`                            | 2 / 12             | 只导入类型；删除 `NOTE_GAP`                                                                                                     |
| 同上                                             | 82-96              | `layeredPositions` 删除便签停靠分支（原 96-99、158-194）                                                                        |
| `lib/components/domain/AddNodeDrawer.svelte`     | 5-9 / 26 / 28      | 导入 `isBuiltinNodeType` / `normalizeNodeType`；`onselect` 与 `Choice.nodeType` 放宽为 `string`                                 |
| 同上                                             | 63-65              | 可用性过滤由"必须是内置名"改为"必须非空"                                                                                        |
| 同上                                             | 91-123             | 分组带 `plugin` 标记                                                                                                            |
| 同上                                             | 186-199            | 组名后显示 `plugin` 徽标                                                                                                        |
| 同上                                             | 221-236            | 删除 `hiddenCount` 提示，新增插件类型说明                                                                                       |
| `lib/stores/node-insert.svelte.ts`               | 1-8                | `nodeType` 放宽为 `string`                                                                                                      |
| `lib/components/domain/GraphExplorer.svelte`     | 49 / 102-105 / 446 | 移除 `StaticNodeType` 导入；`onaddnode` 与 `handleAddNodeChoice` 放宽为 `string`                                                |
| `lib/components/domain/WorkflowEditPanel.svelte` | 9 / 29-33          | 同上                                                                                                                            |
| `routes/workflows/[id]/+page.svelte`             | 49 / 239-242       | 同上                                                                                                                            |
| `routes/templates/[kind]/[id]/+page.svelte`      | 47 / 574-581       | 改用 `normalizeNodeType`；插入按钮对任何非空类型渲染                                                                            |

### 7.5 测试

| 文件                               | 状态               | 内容                                                                                                                                                                                                                       |
| ---------------------------------- | ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `lib/graph/node-kind.test.ts`      | 新增（12 用例）    | 22 个合法名大小写均可解析；`STEP` / `CUSTOM` / `trigger` 不在内置集；默认值合法；`normalizeNodeType` 规范化内置名、原样保留插件名、只拒空值；`isBuiltinNodeType` 判定；入口/出口端口闭合；其余类型双向；未知类型按普通节点 |
| `lib/graph/canvas-connect.test.ts` | 新增（7 用例）     | 拓扑回归、边界端口拦截（大小写不敏感）、普通类型放行、拓扑优先于端口                                                                                                                                                       |
| `lib/graph/display-model.test.ts`  | 改写渲染与端口用例 | 由 `portType` 语义映射改为边界端口文案断言；`renderKind` 用例改为只断言后端真实类型，并新增一条"后端不存在的字面量一律落到 `step`"的回归用例；便签停靠用例改为"无连线的节点也会被布局"                                     |

---

## 8. 验证情况

| 项       | 命令                                                        | 结果                                                                                     |
| -------- | ----------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| 类型检查 | `npm run check`（`apps/web-app`）                           | **0 errors / 0 warnings**                                                                |
| 单元测试 | `npm run test`（`apps/web-app`）                            | **10 文件 / 126 用例全部通过**                                                           |
| 代码风格 | `npx prettier --write` + `npx eslint`                       | 格式化完成，改动文件 **0 error**                                                         |
| 后端编译 | `cargo check --workspace --all-targets -j 2`                | **通过**（仅 1 条既有警告：`wf-plugin/src/engine/loader.rs:206` 未使用变量，非本次改动） |
| 后端单测 | `cargo test -p wf-types -p wf-config -j 2`                  | **通过**（`wf-types` 96 用例 + `wf-config` 96 用例）                                     |
| 后端单测 | `cargo test -p wf-types --lib 'node::r#static::tests' -j 2` | **4/4 通过**（新增的反序列化用例）                                                       |
| 后端单测 | `cargo test -p wf-workflow --lib node_validation -j 2`      | **12/12 通过**（含 `plugin_node_type_is_accepted`）                                      |
| 后端单测 | `cargo test -p wf-api --lib infra::config -j 2`             | **6/6 通过**（含 `keeps_plugin_node_type_on_transform`）                                 |

编译与测试全程限制 `-j 2`（`~/.cargo/config.toml` 已设 `[build] jobs = 2`）。容器内 `static.rust-lang.org` 不可达，需显式 `RUSTUP_TOOLCHAIN=stable-x86_64-unknown-linux-gnu` 绕过 `rust-toolchain.toml` 里的跨平台 target 同步，否则 rustup 会尝试联网下载工具链。

容器内无后端服务，未做前后端联调；接口字段与类型规则已按后端源码逐条核对（第 1 节），联调时无需再改前端结构。

---

## 9. 风险与回退

- **边界端口校验可能拦住历史错边**：只读模式不受影响；编辑态下给 START 加入边、从 END 出边会被拒（这些边服务端本来也会拒）。回退只需删掉 `canvas-connect.ts:37-41`。
- **插件类型节点保存成功但执行失败**：开放 `Custom` 后，草稿能存下未注册插件的节点类型，运行期才会因找不到 handler 报错。这是"注册解析属于运行期"的必然代价；抽屉已用 `plugin` 徽标与说明文字提前告知。若希望保存期即拦截，需要 `wf-workflow` 引入插件注册表依赖，属于独立议题。
- **后端行为变化面**：`LLMM` 这类原本被拒的 `node_type` 现在会进入图。三处断言已从"拒绝"改为"接受"，若有端到端测试依赖旧的错误文案需同步更新（已全仓搜索 `unknown node type`，无残留）。
- **渲染角色变少**：`node` / `trigger` / `agent` 角色删除后，原本（事实上从不）命中它们的节点现在统一画成圆角矩形。视觉上无回归，因为后端从未产出过这些 kind。
- **模板接口不可用**：抽屉弹 toast 并只保留 Blank step，画布其余功能不受影响。
- **跨页队列丢失**：入队后若未进入任何工作流 Edit tab，刷新即丢。页面已提示"打开工作流并进入编辑模式以放置"。

---

## 10. 遗留与后续

本章所列两项（后端 `Custom` 矛盾、渲染猜测字面量）**均已在本次修复**，此处只记录后续可能继续推进的方向。

- **便签 / 批注节点**：n8n 的 `StickyNote` 是画布元素（`CanvasNodeRenderType`）而非节点类型，参考文档 `docs/ref/frontend-viz/02-工作流可视化编辑.md:66` 有提及。本次删除了前端的便签停靠逻辑，若要做，需要后端先给出 NOTE 节点类型或独立的画布批注接口，前端再按真实契约重建。
- **插件节点类型的发现与展示**：目前抽屉只能显示模板库里的 `node_type`，无法列出插件注册表里已注册但未建模板的类型。后端暂无"列出已注册节点类型"的 HTTP 接口，需要时另开议题。
- **插件类型的配置校验**：插件类型目前完全不做 config 校验。若希望校验，合理位置是插件自身声明 schema，而非 `wf-config` 猜。

**P1 起**：按 `06` 文档优先级清单，建议下一步为逐节点追踪卡（Dify）、Execution 筛选/重跑（n8n）、`@wf-agent/ui` 抽包。

> 更新：这三项**已在 P1 完成**，细化方案与落地记录见 `docs/plan/frontend-viz-p1.md`。其中共享包抽取后，本文 7.x 节引用的 `web-app/src/lib/components/ui/*`、`web-app/src/lib/utils/cn|status` 路径已迁至 `apps/ui/src/`，引用本文行号时需以新路径为准。

---

## 11. 附录：参考文档与后端出处索引

| 主题                     | 出处                                                                                              |
| ------------------------ | ------------------------------------------------------------------------------------------------- |
| 优先级清单（P0 来源）    | `docs/ref/frontend-viz/06-可借鉴Dify与n8n的设计.md` 第 6 节                                       |
| 端口校验建议             | 同上 §2.1                                                                                         |
| 节点创建器建议           | 同上 §2.2                                                                                         |
| 执行可视化维度           | `docs/ref/frontend-viz/01-执行过程可视化.md`                                                      |
| 可视化编辑维度           | `docs/ref/frontend-viz/02-工作流可视化编辑.md`                                                    |
| 组件设计维度             | `docs/ref/frontend-viz/04-组件设计.md`                                                            |
| 节点类型枚举             | `crates/foundation/wf-types/src/node/static.rs:13-38`、`42-65`、`69-95`、`157-170`                |
| 边界拓扑规则             | `crates/engine/wf-workflow/src/validation/node_types.rs:14-54`                                    |
| 边模型                   | `crates/foundation/wf-types/src/workflow/edge.rs:28-46`                                           |
| 节点模板 summary         | `crates/app/wf-api/src/template/node_template.rs:80-88`                                           |
| 节点模板存储模型         | `crates/foundation/wf-types/src/storage/node_template.rs:4-11`                                    |
| 草稿保存强类型入口       | `crates/app/wf-server/src/api/workflow/drafts.rs:58-61`                                           |
| 引擎侧已有 `Custom` 意图 | `crates/engine/wf-workflow/src/coordinator/workflow/routing.rs:40`                                |
| API 侧插件类型解析       | `crates/app/wf-api/src/infra/context.rs:323-335`                                                  |
| 插件节点类型注册         | `crates/infra/wf-plugin/src/contributions/manager.rs:30`、`114`                                   |
| 配置层类型校验           | `crates/infra/wf-config/src/processor/workflow.rs:229`、`node_config.rs:31`、`node_template.rs:9` |
| n8n 便签渲染类型         | `docs/ref/frontend-viz/02-工作流可视化编辑.md:66`                                                 |
| 前端 `node_type` 出口    | `apps/web-app/src/lib/graph/edit-store.svelte.ts:255-272`                                         |
