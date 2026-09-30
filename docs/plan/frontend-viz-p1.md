# 前端可视化 P1 阶段细化方案与落地说明

> 依据 `docs/ref/frontend-viz/`（尤其是 `06-可借鉴Dify与n8n的设计.md` 第 6 节优先级清单）编写。
> P1 共三项：**逐节点追踪卡**（源自 Dify）、**Execution 筛选 / 重跑**（源自 n8n）、**共享 UI 包抽取**（多前端复用前提）。
> 本文先给细化方案，再记录实际落地结果（含文件与行号），最后给出全部决策结论。
>
> 本阶段为纯前端改动，未触碰后端 Rust 代码，因此未执行后端编译。

---

## 0. 目标与范围

| 编号 | P1 项               | 来源    | 目标收益                   | 本次状态                        |
| ---- | ------------------- | ------- | -------------------------- | ------------------------------- |
| P1-A | 逐节点追踪卡        | Dify    | 单次执行可下钻到节点级归因 | 已落地                          |
| P1-B | Execution 筛选/重跑 | n8n     | 运行列表可收敛、可复现     | 已落地                          |
| P1-C | 共享 UI 包抽取      | 多前端  | 第二前端复用同一套视觉基线 | 已落地（仅主前端完成迁移）      |

范围外：P2/P3 的分层时间线、跨执行对比、调试器式断点等，一律不在本阶段。

三项之间不是并列关系：P1-C 是结构性改动，把组件、图标、设计令牌与工具函数从主前端搬进独立包；P1-A 与 P1-B 的新增代码直接按新包路径编写，因此实际落地顺序是 **先抽包、再写功能**，以减少二次改导入的成本。

---

## 1. 事实基线（后端契约，逐条带出处）

这一节的事实全部来自后端源码，未做任何推测；前端字段与后端字段一一对应，不存在"容错读取"或"兜底改名"。

### 1.1 节点追踪记录是每节点一条、最新尝试胜出

`crates/app/wf-api/src/workflow/workflow_iteration.rs:112-113` 的文档注释写明：同一节点的多次尝试会折叠为一条记录，最新尝试胜出，按开始时间排序。结构体定义在同文件 `:54-75`：

| 后端字段            | 类型                     | 前端字段           |
| ------------------- | ------------------------ | ------------------ |
| `execution_id`      | `String`                 | `executionId`      |
| `node_id`           | `String`                 | `nodeId`           |
| `node_name`         | `String`                 | `nodeName`         |
| `node_type`         | `String`                 | `nodeType`         |
| `status`            | `String`（见 1.5）       | `status`           |
| `start_time`        | `i64`                    | `startedAt`        |
| `end_time`          | `Option<i64>`            | `endedAt`          |
| `duration`          | `Option<i64>`            | `durationMs`       |
| `input` / `output`  | `Option<Value>`          | `input` / `output` |
| `retry_count`       | `u32`                    | `retryCount`       |
| `error`             | `Option<String>`         | `error`            |
| `tool_dependencies` | `Vec<ToolDependencyView>` | `toolDependencies` |

`status` 的取值在 `workflow_iteration.rs:59` 的注释里是 `pending | running | completed | failed | skipped | cancelled`——**与执行状态枚举不同**（见 1.5），节点级状态是另一套词表，前端按词表原样展示，不映射到执行状态。

`ToolDependencyView` 定义在 `workflow_iteration.rs:26`，字段为 `tool_name` / `call_count`。

### 1.2 三个追踪端点在后端已存在，无需新增

路由注册在 `crates/app/wf-server/src/api/workflow/execution_analysis.rs`：

- `:78` — `/api/v1/executions/{id}/nodes`，节点记录列表
- `:85` — `/api/v1/executions/{id}/nodes/{nodeId}/input-context`
- `:102` — `/api/v1/executions/{id}/llm-reasoning-path/{nodeId}`

三者都由同一份生成的 `schema.d.ts` 覆盖，前端无需重新生成客户端类型。

### 1.3 输入上下文与推理步骤的字段是确定的

`crates/app/wf-api/src/workflow/execution_state.rs:706-713` 定义 `NodeInputContextView`：`node_id` / `node_name` / `node_type` / `input_parameters: BTreeMap<String, Value>` / `timestamp: i64` / `available_variables: Vec<VariableValueSnapshotView>`。

`crates/app/wf-api/src/workflow/workflow_iteration.rs:161-172` 定义 `LlmReasoningRecordView`：`step_id` / `reasoning_type` / `content` / `confidence: Option<f64>` / `related_entities: Vec<String>` / `conclusions: Vec<String>`。`related_entities` 本次未上屏，见第 10 节。

### 1.4 时间戳是毫秒 epoch

`crates/foundation/wf-types/src/common.rs:4` 定义 `pub type Timestamp = i64`，写入侧用毫秒：如 `crates/infra/checkpoint/wf-checkpoint/src/coordinator/workflow.rs:756` 的 `Utc::now().timestamp_millis()`。前端统一乘 1 后交给 `Date`，不再做任何单位猜测。

### 1.5 执行状态是 8 值封闭枚举，线上形态为 snake_case

`crates/foundation/wf-types/src/execution/status.rs:4-15` 定义 `ExecutionStatus`，带 `#[serde(rename_all = "snake_case")]`；`as_str` 在 `:32-43` 给出全部线上字面量：

```
created  running  paused  stopped  completed  failed  cancelled  timeout
```

### 1.6 列表接口的 status 是精确相等过滤，不是模糊匹配

`crates/infra/wf-storage/src/adapter/execution.rs:23-24`：`status_filter` 转成 `FilterOp::Eq("status", value)`。查询参数在 `crates/app/wf-server/src/api/workflow/executions.rs:133-141` 的 `ListExecutionsQuery`：`limit` / `offset` / `workflow_id` / `status`。

这条决定了下拉框的选项值必须是后端字面量本身——任何"显示名 → 值"的再映射都会让过滤落空。

### 1.7 执行记录自带 input 与 error

`crates/foundation/wf-types/src/workflow_execution/definition.rs:29` 有 `input: Option<Value>`，`:40` 有 `error: Option<String>`。列表接口返回的就是这个实体（`:44-53` 直接返回 `Vec<WorkflowExecution>`），所以重跑所需输入与失败摘要都能从列表页拿到，不需要再拉详情。

### 1.8 执行入口只接受 input，没有版本参数

`crates/app/wf-server/src/api/workflow/executions.rs:48-50` 的 `ExecuteBody` 只有 `input: Option<Value>` 一个字段。这意味着"重跑"没有版本锚点，只能用当前生效的定义。

### 1.9 改动前的代码问题清单

| 问题                                                                                   | 出处（改动前）                          | 后果                             |
| -------------------------------------------------------------------------------------- | --------------------------------------- | -------------------------------- |
| 过滤条的 `status` 是 `$bindable`，但内部 `Select` 用单向 `value={status}`，变更不回传    | `web-app/src/lib/components/ui/FilterBar.svelte` 旧版 `:38-43` | 选了状态等于没选，父组件永远是空串 |
| 运行列表只渲染 id + 状态 + 开始时间，无时长、无失败节点数、无失败原因                     | `web-app/src/lib/components/domain/WorkflowRunsPanel.svelte` 旧版整体 | 失败定位只能逐条点进详情          |
| `Execution` 视图模型没有 `input` / `error` 字段                                          | `web-app/src/lib/types/models.ts`        | 无法重跑，也无法在列表展示失败摘要 |
| 执行检查器只有 Overview / Graph / Timeline / Tools / Analysis / State，没有节点级视图     | `web-app/src/lib/components/domain/ExecutionInspector.svelte` | 节点归因要跳到 Graph 逐个点       |
| UI 组件、图标、设计令牌、工具函数全部住在主前端 `src/lib` 下                             | `web-app/src/lib/components/ui`、`icons`、`utils` | 第二个前端只能复制，无法复用      |
| 设计令牌内联在 `app.css` 顶部，与 Tailwind 配置同文件                                     | `web-app/src/app.css` 旧版 `:5-147`      | 令牌无法被第二前端以文件粒度引入  |

---

## 2. 差距分析（对照 Dify / n8n）

对照材料取自 `docs/ref/frontend-viz/` 与两个参考仓库的实际组件。

| 能力                   | Dify                                                          | n8n                                                             | 改动前 wf-agent | 本次做法                                            |
| ---------------------- | ------------------------------------------------------------- | --------------------------------------------------------------- | --------------- | --------------------------------------------------- |
| 节点级追踪             | 右侧/底部面板以可折叠卡片逐节点列出，含 Input/Output、耗时、重试 | 执行详情以节点列表呈现，树形折叠粒度更粗                         | 无              | 取 Dify 的卡片形态，挂到执行检查器新标签页          |
| 失败定位               | 失败节点高亮并置顶错误块                                       | 列表行左侧色条标识状态                                           | 列表无色条      | 两者都取：列表左侧色条 + 卡片内独立错误块           |
| LLM 推理链             | tracing 面板内展开推理步骤                                     | 无对应能力                                                       | 无              | 已有后端 `llm-reasoning-path`，展开卡片时按需拉取   |
| 执行列表筛选           | 列表页状态筛选较薄                                             | `ExecutionsFilter` 提供状态/时间筛选，卡片左色条，支持重跑       | 无筛选          | 取 n8n：状态走服务端过滤，文本走本地过滤            |
| 重跑                   | 从运行记录重跑                                                 | 执行卡片直接重跑                                                 | 无              | 取 n8n：行内重跑按钮，复用记录里的 input            |
| 多前端复用             | 不适用                                                         | 设计系统独立成包 `@n8n/design-system`                            | 未拆分          | 取 n8n 的做法：抽出 `@wf-agent/ui`                  |

一处刻意不照抄：Dify 的 tracing 面板支持重试/迭代/循环的子日志树，而后端 `workflow_iteration.rs:112-113` 已经把同一节点的多次尝试折叠成一条（只留最新），前端拿不到尝试级明细。因此本阶段只做"每节点一卡 + 重试次数标记"，不做尝试树——那需要后端先暴露尝试级数据。

---

## 3. 决策记录（原"待拍板开放点"全部拍板）

| #  | 开放点                                     | 结论                                                                 | 理由                                                                 |
| -- | ------------------------------------------ | -------------------------------------------------------------------- | -------------------------------------------------------------------- |
| D1 | 节点追踪放哪里                             | 执行检查器新增标签页，与 Graph / Timeline 平级                        | 与 P0 已建立的标签化结构一致，避免新增一种容器                        |
| D2 | 卡片详情（输入上下文、推理）何时加载       | 展开时按需加载，收起不重新请求                                        | 推理链可能很长，全量预取会拖慢首屏；展开即加载符合阅读节奏            |
| D3 | 节点 id 缺失的行怎么处理                   | 丢弃并计数，在汇总行显式报出                                          | 无 id 的行既不能定位到图，也不能与其他视图连接；按 P0 定下的"不静默兜底"原则，宁可显式丢弃也不造合成 id |
| D4 | 状态筛选走服务端还是本地                   | 状态走服务端（`status` 查询参数），自由文本走本地                      | 后端是精确相等过滤（1.6），本地筛状态会与分页语义冲突；文本筛不到 payload，只筛已取回的页 |
| D5 | 下拉选项用显示名还是后端字面量             | 直接用后端 8 个字面量                                                  | 1.6 的精确相等决定了别名会让过滤恒为空                                |
| D6 | 重跑是否带版本                             | 不带，用执行记录里的 input 调执行接口，并在注释里点明"重放当前定义"    | 执行入口无版本参数（1.8），前端不应假装能锚定版本                      |
| D7 | UI 包如何避免依赖 SvelteKit                 | 包内不做路由假设，改为"链接解析器注入"：宿主启动时装一个 route→URL 的函数 | 包若引 `$app/paths` 就只能用于 SvelteKit 应用，失去"多前端复用"的意义  |
| D8 | 设计令牌搬不搬                             | 搬，原样搬到包内独立 CSS 文件，主前端改为 import                       | 多前端复用首先是视觉基线复用；令牌留在 `app.css` 里无法被第二个前端引入 |
| D9 | 预览工程（`web-app-preview`）是否一起迁移   | 不迁移                                                                | 该工程当前是停留在 P0 之前的旧镜像（自带一份旧的组件副本），本阶段不动，见第 10 节 |
| D10| 抽包是否引入 monorepo 构建工具             | 不引入；包以源码形式发布（`svelte` 字段指向源码入口），消费方直接编译  | 与现有 SvelteKit + Tailwind 的编译链路一致，避免新增一层打包步骤        |

---

## 4. P1-A：逐节点追踪卡

### 4.1 数据层先于视图层

新增服务 `web-app/src/lib/services/node-trace.ts`，只做三件事：调用 1.2 的三个端点、把后端 DTO 映射成视图模型、提供纯函数式的筛选与汇总。映射函数全部单独导出，便于单测覆盖而不必挂网络。

映射的关键约束（对应 D3）：`node_id` 缺失或全空白的行直接返回 `null`，调用方把它计入 `skipped`，最终在界面上以"N 条不可寻址记录已丢弃"的形式报出来，而不是静默合成 id。

筛选与汇总是纯函数：状态筛选里"失败"不是单一字面量，而是 `failed / failure / error / errored / timeout` 的集合——后端节点状态与执行状态词表不同（1.1），把失败判定收在一处，避免视图里散落多处字符串比较。搜索只覆盖 id / 名称 / 类型，因为 payload 是按需加载的，搜它需要先把全部 payload 拉下来。

### 4.2 卡片与列表

`NodeTraceCard.svelte` 取 Dify 的形态：一行头部（状态色图标、节点名、类型、时长、重试次数徽标）+ 可折叠体（错误块、Input/Output 两个 JSON 查看器、工具依赖标签、输入参数与可用变量、LLM 推理步骤列表）+ 一个"在图上定位"按钮。

`NodeTracePanel.svelte` 负责列表：搜索框 + 状态下拉 + 汇总行（节点数 · 失败数 · 重试次数 · 被丢弃的行数），逐行渲染卡片，展开时并发拉取输入上下文与推理链。

与图联动是这次的重点：面板接收 `selectedNodeId`，一旦外部选中图上某节点，对应行自动展开并滚动到可视区；卡片上的"定位"按钮则反向把节点 id 抛回给检查器去聚焦图节点。两个方向都走已有的 `focusGraphNode`，没有新增第二套选中逻辑。

### 4.3 挂载点

执行检查器新增一个标签页，位置在 Graph 之后——因为这两者是最常互相跳转的一对。加载沿用该组件已有的"标签 + 已加载标记"模式：切到该标签且当前执行 id 未加载过才发请求，失败时把标记复位以便重试。Graph 侧的节点检查片段加一个"查看该节点追踪"入口，把当前选中的节点带过去。

---

## 5. P1-B：Execution 筛选 / 重跑

### 5.1 筛选

过滤条复用 P1-C 抽出的共享组件，并补上一个之前缺失的能力：状态变更回调。原实现里 `Select` 用的是单向 `value`，导致选了状态父组件收不到（见 1.9）；改为双向绑定并在变更时回调宿主，由宿主决定是否重新拉取。

状态选项直接是 1.5 的 8 个字面量（D5），不 mapping。选项变更触发服务端重查；自由文本只筛已取回的这一页的执行 id，不假装能搜到 payload。两者语义不同，界面上也分开表达：重查会显示骨架屏，本地过滤不会。

### 5.2 行内容

取 n8n 的左侧色条：每行 `border-left-color` 绑定到状态色调，扫一眼就能分出成败。行内补上时长、失败节点数，以及执行记录里的 `error` 摘要（`title` 属性保留全文，视觉上截断）。这三项此前都没有，是"能不能在列表页完成初判"的关键。

### 5.3 重跑

行内按钮调执行接口，输入取自该行执行记录的 `input` 字段（1.7），成功后提示新执行 id 并刷新列表。D6 的约束写进代码注释：执行接口没有版本参数，所以对旧版本的执行做重跑，实际重放的是当前生效的定义。这一点必须在界面/注释里说清楚，否则使用者会误以为重跑等价于"按当时版本重放"。

---

## 6. P1-C：共享 UI 包抽取

### 6.1 边界划分

搬进包的内容：UI 基础组件、图标、`cn` 类名工具、状态色调工具、焦点陷阱、数字格式化、虚拟化阈值常量、设计令牌。

留在应用内的内容：所有领域组件（与后端契约耦合）、服务层、路由配置、主题偏好存储。判断标准只有一条——**是否需要知道后端契约或应用路由**。需要的一律留在应用里。

### 6.2 路由解耦

包内的按钮组件原本直接用 SvelteKit 的 `resolve` 处理 `href`。抽包后这会反向依赖框架，因此改成注入式：包导出一个"设置/读取链接解析器"的模块，宿主在根布局里装一个把路由转成 URL 的函数。没有装解析器时按路由原样输出，保证组件在隔离环境（单测、静态预览）里仍可渲染。

### 6.3 令牌与常量

设计令牌原样搬到包内 CSS 文件（`:root`、`.dark`、字号档位整段搬移，不做改写），主前端 `app.css` 改为一条 import。虚拟化阈值与数字格式化改为从包里再导出，应用侧保留原有导出名，避免扩散式改调用点。

### 6.4 消费方式

npm workspaces 增加该包，主前端以 `file:../ui` 依赖它。包以源码形式提供（`exports` 指向 `src`），由消费方的 Vite/SvelteKit 链路直接编译，不引入额外打包步骤（D10）。

---

## 7. 代码落点（实际行号）

### 7.1 P1-C：包本体

| 文件                              | 说明                                                     |
| --------------------------------- | -------------------------------------------------------- |
| `apps/ui/package.json`            | 新建；包名 `@wf-agent/ui`，源码入口与子路径导出          |
| `apps/ui/src/index.ts`            | 新建；桶文件，统一导出组件与工具（44 行）                |
| `apps/ui/src/link.ts`             | 新建；链接解析器注入（`:21-23` 的 `resolveHref` 为唯一出口） |
| `apps/ui/src/format.ts`           | 新建；数字格式化                                         |
| `apps/ui/src/virtualization.ts`   | 新建；虚拟化阈值常量                                     |
| `apps/ui/src/styles/tokens.css`   | 新建；从 `app.css` 原样搬来的设计令牌（142 行）          |
| `apps/ui/src/components/*`        | 由 `web-app/src/lib/components/ui/*` 整体移动（含 `table.ts`、`variants.ts`） |
| `apps/ui/src/icons/*`             | 由 `web-app/src/lib/components/icons/*` 移动             |
| `apps/ui/src/cn.ts` / `status.ts` / `focus-trap.ts` | 由 `web-app/src/lib/utils/*` 移动             |
| `apps/ui/src/components/Button.svelte:7,16,37` | `href` 改为字符串 + `resolveHref`，去掉框架依赖 |
| `apps/ui/src/components/DataTable.svelte` | 改用包内阈值常量                                   |

### 7.2 P1-C：主前端适配

| 文件                                        | 说明                                              |
| ------------------------------------------- | ------------------------------------------------- |
| `apps/package.json:6-9`                     | workspaces 增加 `ui`                              |
| `apps/web-app/package.json`                 | 依赖增加 `@wf-agent/ui`                           |
| `apps/web-app/src/app.css:3`                | 令牌改为 import 包内 CSS（378 行 → 236 行）       |
| `apps/web-app/src/routes/+layout.svelte:26` | 安装链接解析器                                    |
| `apps/web-app/src/lib/utils/format.ts:1`    | 数字格式化改为从包再导出                          |
| `apps/web-app/src/lib/config/virtualization.ts` | 阈值改为从包再导出                            |
| 其余约 60 个组件/路由                       | 导入路径从 `$lib/components/ui|icons`、`$lib/utils/cn|status` 改为包路径 |

### 7.3 P1-A

| 文件                                                          | 说明                                                     |
| ------------------------------------------------------------- | -------------------------------------------------------- |
| `apps/web-app/src/lib/types/models.ts:38-83`                  | 新增节点追踪、工具依赖、输入上下文、推理步骤等视图模型   |
| `apps/web-app/src/lib/services/node-trace.ts:75-100`          | 节点记录映射，无 id 直接返回 `null`                      |
| `apps/web-app/src/lib/services/node-trace.ts:142-162`         | 列表端点调用 + 丢弃计数                                  |
| `apps/web-app/src/lib/services/node-trace.ts:165-190`         | 输入上下文与推理链按需加载                               |
| `apps/web-app/src/lib/services/node-trace.ts:192-247`         | 筛选项常量、失败判定、筛选与汇总纯函数                   |
| `apps/web-app/src/lib/components/domain/NodeTraceCard.svelte` | 新建；单节点卡片                                         |
| `apps/web-app/src/lib/components/domain/NodeTracePanel.svelte:95-108` | 图选中 → 自动展开并滚动                        |
| `apps/web-app/src/lib/components/domain/ExecutionInspector.svelte:611-626` | 新标签的按需加载分支                      |
| 同上 `:651`                                                    | 标签表新增 `Trace`（位于 `graph` 之后）                  |
| 同上 `:363-366`、`:925-937`、`:903-909`                       | 图上节点跳追踪、面板渲染、Graph 侧入口                   |

### 7.4 P1-B

| 文件                                                             | 说明                                              |
| ---------------------------------------------------------------- | ------------------------------------------------- |
| `apps/ui/src/components/FilterBar.svelte:16,40,44`               | 新增状态变更回调；`Select` 改为双向绑定            |
| `apps/web-app/src/lib/components/domain/WorkflowRunsPanel.svelte:33-42` | 状态选项直接用后端 8 个字面量               |
| 同上 `:71-82`                                                     | 重跑；注释写明"无版本参数，重放当前定义"          |
| 同上 `:86-91`                                                     | 文本仅在已取回页内过滤                            |
| 同上 `:142-144`                                                   | n8n 式左侧状态色条                                |
| `apps/web-app/src/lib/types/models.ts:25-28`                    | `Execution` 增加 `input` / `error`                 |
| `apps/web-app/src/lib/services/executions.ts:49,86-87`            | DTO 与映射补上这两个字段                          |

### 7.5 测试

`apps/web-app/src/lib/services/node-trace.test.ts`（新建，10 例）覆盖：映射字段与工具依赖、空白 id 被丢弃、毫秒时间戳转 ISO、输入上下文扁平化、五种状态筛选、搜索命中 id/名称/类型、汇总计数。

---

## 8. 验证情况

| 检查项                       | 命令                                    | 结果                                    |
| ---------------------------- | --------------------------------------- | --------------------------------------- |
| 类型检查                     | `apps/web-app` 下 `npm run check`       | 0 errors / 0 warnings                   |
| 单测                         | 同上 `npm run test`                     | 11 文件 / 136 例全部通过（新增 10 例） |
| 静态检查                     | 同上 `npm run lint`                     | 无告警                                  |
| 格式                         | 同上 `npm run format:check`             | 全部通过                                |
| 新包格式                     | `apps/ui` 下 `prettier --check .`       | 全部通过                                |
| 生产构建                     | `apps/web-app` 下 `npm run build`       | 成功；产物 CSS 中确认 `--running` 与字号档位令牌存在 |

过程中修掉的三个具体问题：

1. 空白 `node_id` 未被判为缺失，导致测试期望落空——判空改为 trim 后判空。
2. 面板里用原生 `Set` 触发 `prefer-svelte-reactivity` 规则——改为响应式集合并原地增删。
3. 包内通配子路径导出无法解析无扩展名的 TS 文件（`variants` / `table` / `icons/paths`）——在包 `exports` 中补显式条目；令牌 CSS 走 `styles/*` 子路径，避免被默认导出条件挡住。

后端未编译：本阶段不改动 Rust 代码，故未执行 `cargo` 相关命令。

---

## 9. 风险与回退

| 风险                                                     | 影响                             | 处置                                                                 |
| -------------------------------------------------------- | -------------------------------- | -------------------------------------------------------------------- |
| 节点状态词表与执行状态词表不同，前端可能误判失败         | 失败计数偏少                     | 失败判定集中在服务层一个函数，词表以数组常量列出，新增词表只改一处     |
| 重跑无版本锚点，旧执行重跑语义与使用者预期不符           | 误判重跑结果                     | 已在代码注释与按钮提示中写明"用该次记录的输入重跑当前定义"            |
| 包以源码形式消费，若宿主编译链路不同可能解析失败          | 第二前端接入受阻                 | 包只依赖 Svelte 本身，路由通过注入解耦；接入新前端时先装解析器即可     |
| 大量导入路径改写可能漏改                                 | 运行时模块缺失                   | 类型检查 + 生产构建双重把关，二者均通过                              |
| 预览工程仍是旧镜像，与主前端结构已分叉                    | 预览工程无法直接构建             | 本阶段不动它（D9），见第 10 节                                        |

回退方式：整体为一个提交集，直接回退即可；包抽取是移动而非复制，回退不会留下重复副本。

---

## 10. 遗留与后续

1. **预览工程未同步**：`apps/web-app-preview` 当前停留在 P0 之前的旧镜像，仍自带一份旧的组件副本。同步脚本会把主前端的依赖块合并过去，因此新包依赖会在下次同步时自动带入，但组件目录的清理需要一次完整同步。建议在下个阶段单独做一次"预览工程追平"，不要混在功能改动里。
2. **`related_entities` 未上屏**：推理步骤里的相关实体字段已存在于后端视图（1.3），本阶段未找到合适的展示位置，待 P2 的推理可视化一起处理。
3. **尝试级明细缺失**：后端已把同节点多次尝试折叠为一条（1.1），Dify 式的"重试子日志树"需要后端先暴露尝试级数据，属于后端前置项。
4. **节点追踪未做分页**：列表端点一次返回全部节点记录，节点数极多时首屏会变长。当前按"先有后优"处理，若实测成为瓶颈，再考虑前端窗口化或后端分页。
5. **P2 分层时间线**与跨执行对比仍未开始，均依赖本次的共享包落地后再推进。

---

## 11. 附录：参考文档与后端出处索引

参考文档：

- `docs/ref/frontend-viz/00-总览与分析方法.md`
- `docs/ref/frontend-viz/01-执行过程可视化.md`
- `docs/ref/frontend-viz/06-可借鉴Dify与n8n的设计.md`（第 6 节优先级清单）
- `docs/plan/frontend-viz-p0.md`（P0 结论与本阶段的前置约束）

对照仓库（本地检出）：

- Dify：`workflow/` 下的运行面板与 `tracing-panel.tsx`（可折叠树）
- n8n：`executions` 视图的 `ExecutionsFilter.vue`、`WorkflowExecutionsCard.vue`（左侧状态色条）

后端出处：

| 事实                     | 出处                                                            |
| ------------------------ | --------------------------------------------------------------- |
| 节点记录结构             | `crates/app/wf-api/src/workflow/workflow_iteration.rs:54-75`      |
| 每节点一条、最新尝试胜出 | `crates/app/wf-api/src/workflow/workflow_iteration.rs:112-113`    |
| 工具依赖结构             | `crates/app/wf-api/src/workflow/workflow_iteration.rs:26`         |
| 推理步骤结构             | `crates/app/wf-api/src/workflow/workflow_iteration.rs:161-172`    |
| 输入上下文结构           | `crates/app/wf-api/src/workflow/execution_state.rs:706-713`      |
| 三个追踪端点             | `crates/app/wf-server/src/api/workflow/execution_analysis.rs:78,85,102` |
| 时间戳为毫秒            | `crates/foundation/wf-types/src/common.rs:4`；写入侧 `crates/infra/checkpoint/wf-checkpoint/src/coordinator/workflow.rs:756` |
| 执行状态 8 值            | `crates/foundation/wf-types/src/execution/status.rs:4-15,32-43`  |
| status 精确相等过滤      | `crates/infra/wf-storage/src/adapter/execution.rs:23-24`         |
| 列表查询参数             | `crates/app/wf-server/src/api/workflow/executions.rs:133-141`     |
| 执行入口只收 input       | `crates/app/wf-server/src/api/workflow/executions.rs:48-50`       |
| 执行记录含 input / error | `crates/foundation/wf-types/src/workflow_execution/definition.rs:29,40` |
