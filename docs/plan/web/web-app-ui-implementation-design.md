# Web App 样式、布局与组件实现方案

> 目标：为 `apps/web-app`（SvelteKit 2 + Svelte 5）落地**样式体系、页面布局、组件体系**三层设计，并给出可执行的任务分解。
> 本阶段**不对接真实 API**：页面数据一律来自 `src/lib/fixtures` 下的本地样例数据，API 客户端、SSE/WS 接入留待后续阶段。
> 上游依据：`docs/plan/web/frontend-feature-list.md`（页面与功能边界）、`docs/plan/web/web-frontend-borrow-analysis.md`（借鉴结论）、`docs/ref/frontend/*`（原始实现）、`docs/plan/web-app-integration.md`（集成与部署约定）。

---

## 0. 文档定位与范围

本文只解决三件事：**长什么样**（样式）、**怎么摆**（布局）、**用什么拼**（组件）。

本文档范围内：

- Design Token 体系（颜色、字体、间距、圆角、阴影、动效）。
- 应用壳（三栏 Shell、侧栏、顶栏、命令面板、Toast）。
- 基础组件库与领域组件库。
- 十个一级页面及其详情子路由的静态结构与占位数据流。

本文档范围外：

- API 客户端、信封拆包、鉴权注入（见 `docs/plan/web-app-integration.md` 第 2 节）。
- SSE / WS 流式接入（见 `docs/plan/web/web-app-streaming-markdown-design.md`）。
- 国际化、PWA、登录计费（功能清单第 5 节已列为非目标）。

---

## 1. 现状事实核查

下表是**改造前**逐条核验的基线；凡已随本次落地产变化的，在行内标注现状。

| 事实 | 依据 |
|---|---|
| `apps/web-app` 为 SvelteKit 2 + Svelte 5 空壳，仅一个占位首页（**已删**：根路由改为重定向），无布局、无路由分组、无 store、无 API 客户端、无样式体系 | 原状 `apps/web-app/src/routes/+page.svelte:1-8`；`docs/plan/web/web-frontend-borrow-analysis.md:7`；现状 `apps/web-app/src/routes/+page.ts:4-6` |
| 仅 14 个源文件，其中 `src/lib/api/schema.d.ts` 占 41309 行，是唯一实质资产 | `apps/web-app` 目录清点 |
| 后端契约含约 390 paths / 452 operations，覆盖 11 个域前缀 | `apps/web-app/src/lib/api/schema.d.ts`；`docs/plan/web-app-integration.md:15` |
| 域分布（按路径前缀计数）：executions 83、agent-loops 56、workflows 39、templates 23、llm 19、file-checkpoint 17、events 15、skills 14、agent-executions 12、variables 10，其余为 tools/scripts/interactions/checkpoints/query/metrics/analysis 等 | `apps/web-app/src/lib/api/schema.d.ts` 路径前缀统计 |
| **状态字段在契约中是 `string` 而非枚举**，注释给出示例值 `"running"`, `"paused"`, `"completed"`, `"failed"` | `apps/web-app/src/lib/api/schema.d.ts:6772` |
| 构建适配器原为 `adapter-auto`，与集成方案要求的 `adapter-static` 不一致（**已改**：现为 `adapter-static` + SPA fallback） | 原状 `apps/web-app/svelte.config.js:8`；`docs/plan/web-app-integration.md:107-108`；现状 `apps/web-app/svelte.config.js:1,8` |
| 工程已配置 prettier（tabs + 单引号）、eslint、vitest、svelte-check | `apps/web-app/package.json:9-23`；`.prettierrc` |
| 原依赖树不含任何样式框架、组件库、图标库 | `apps/web-app/package.json` devDependencies 原始内容 |

**关键结论**：状态色不能依赖枚举穷举，组件必须为未知状态值提供中性兜底，否则契约演进时会出现无样式状态。

### 1.1 实现期新发现的事实

落地过程中核查出两条会约束写法的工程事实：

| 事实 | 依据 | 约束 |
|---|---|---|
| SvelteKit 2.69+ 的 `resolve()` 参数是**生成路由字面量联合类型**，传 `string` 直接类型报错 | `apps/web-app/.svelte-kit/non-ambient.d.ts` 中 `AppTypes['Pathname']`；`node_modules/@sveltejs/kit/types/index.d.ts:3488-3491` 的 `resolve<T>(...args: ResolveArgs<T>)` | 导航配置的 `href` 必须声明为生成类型 `Pathname`，不能是 `string` |
| eslint `svelte/no-navigation-without-resolve` 只认**直接调用 `resolve()`** 的表达式，包一层自定义函数也会被判违规 | `node_modules/eslint-plugin-svelte/lib/rules/no-navigation-without-resolve.js` 中 `expressionIsResolveCall` | 禁止用 `appPath(x)` 之类包装函数替代 `resolve()`，包装函数只能收敛「类型转换」这一步 |

对应做法：新增 `src/lib/utils/route.ts`（`apps/web-app/src/lib/utils/route.ts:1-17`）导出 `AppPath`（即生成的 `Pathname`）与 `appPath()`，**只负责把动态字符串收窄为 `AppPath`**；所有链接仍必须写成 `resolve(...)`。`NavItem.href` 类型由 `string` 改为 `AppPath`（`apps/web-app/src/lib/config/navigation.ts:5`），`Button` 的 `href` 同样收窄（`apps/web-app/src/lib/components/ui/Button.svelte:16`）并在渲染处 `resolve(href)`（`apps/web-app/src/lib/components/ui/Button.svelte:37`）。

> 行号以 `npx prettier --write src` 之后的状态为准（`npm run format` 会整体重排，引用前请重新核验）。

---

## 2. 设计目标与非目标

### 2.1 目标

- **一致性**：所有页面共享同一套 Token，禁止在组件内写死颜色值。
- **信息密度**：面向开发者的运维/调试工作台，默认紧凑档，允许放宽。
- **状态可读**：执行状态、变更类型、工具类型一眼可辨，且与终端配色解耦。
- **可扩展**：新增域页面只需加路由 + 复用组件，不改壳。
- **无数据可跑**：不对接后端时，每个页面都能展示完整骨架与样例内容。

### 2.2 非目标

- 不实现登录、计费、多媒体生成、桌面专属能力（功能清单第 5 节）。
- 不实现需要新后端的功能：文件编辑器、触发器启停、技能安装、模板评分、通知收件箱。
- 本阶段不做虚拟化长列表的真实数据压测，只预留接口。

---

## 3. 技术选型与取舍

### 3.1 选型结论

| 维度 | 选择 | 理由 |
|---|---|---|
| 样式 | **Tailwind CSS v4**（CSS-first，`@theme` 定义 Token） | 与 `docs/ref/frontend/live-agent/liveagent-ui-analysis-and-web-frontend-strategy.md:395` 的选型一致；v4 无需 JS 配置文件，Token 直接用 CSS 变量暴露，与 shadcn 变量体系天然兼容 |
| 组件库 | **自建 shadcn 风格**，不引入 shadcn-svelte CLI 与 Radix | CLI 需交互式初始化与外网拉取 registry；自建可完全控制体积，且本项目只需要其中十余个原语 |
| 图标 | **自建内联 SVG 图标组件** | 避免引入图标库依赖；统一 24px 网格、1.5px 描边的细线风格（对齐 `docs/ref/frontend/zcode-frontend.md:112`） |
| 状态 | **Svelte 5 runes**（`$state` / `$derived` / `$props`） + 模块级 `.svelte.ts` 单例 | 与 `docs/ref/frontend/live-agent/liveagent-ui-analysis-and-web-frontend-strategy.md:393` 一致，避免外部状态库 |
| 类名合并 | 自写极简 `cn()` | 避免 `clsx + tailwind-merge` 依赖，本项目条件类名复杂度低 |
| 图可视化 | 本阶段用**内联 SVG 占位渲染**，不引入 D3/ECharts | 本阶段不接数据；引入重量级图表库会锁死后续选型 |
| 适配器 | 改为 **`adapter-static`** | 消除 `adapter-auto` 无平台时构建失败的风险，且符合 `docs/plan/web-app-integration.md:107` 的部署约定 |

### 3.2 新增依赖

仅新增 `tailwindcss`、`@tailwindcss/vite`、`@tailwindcss/typography`、`@sveltejs/adapter-static` 四项，其余能力全部用 Svelte / SvelteKit 原生能力自建。这样依赖树可控，也不会把图表、组件库的选型提前锁死。

实际写入 `apps/web-app/package.json:34-57` 的版本：`tailwindcss@^4.1.18`、`@tailwindcss/vite@^4.1.18`、`@tailwindcss/typography@^0.5.19`、`@sveltejs/adapter-static@^3.0.10`。`adapter-auto` 保留未删（其他工程可能引用），但已不再被 `svelte.config.js` 使用。

---

## 4. 样式体系

### 4.1 Token 分层

采用三层结构，与 `docs/ref/frontend/live-agent/design-style-analysis.md:284-330` 的建议一致：

1. **基础层**：HSL 通道值（如 `--background: 220 15% 10%`），供 `hsl(var(--x) / <alpha>)` 调透明度。
2. **语义层**：`--color-bg`、`--color-surface`、`--color-border`、`--color-fg-muted` 等，组件只消费语义层。
3. **状态层**：`--color-running`、`--color-success`、`--color-danger`、`--color-warning`，专供状态徽标与指示灯。

### 4.2 色板主轴

沿用 **蓝灰中性主轴（HSL 210°–220°）**，不引入品牌亮色——理由见 `docs/ref/frontend/live-agent/design-style-analysis.md:47`（专业工具感）。语义色在此基础上叠加：

| 语义 | 用途 | 说明 |
|---|---|---|
| 成功 / 已完成 | 执行成功、新增变更 | 绿色系，独立于终端配色 |
| 危险 / 失败 | 失败、删除变更 | 红色系 |
| 运行中 | 执行中、流式推送中 | 紫蓝色系（对齐 LiveAgent running 色相 252°） |
| 排队 / 暂停 | 等待、挂起 | 琥珀色系 |
| 中性 / 未知 | 契约中出现未收录的状态字串 | 灰系兜底，**这是硬要求**，见第 1 节结论 |

### 4.3 字体与字号

- 三套字体族：界面（含 `PingFang SC` / `Microsoft YaHei` 中文回退）、代码（等宽栈）、可选衬线（空态问候语）。
- 字号档：紧凑 / 默认 / 宽松三档，通过根元素 `--font-scale` 缩放，作用于界面字号而非常规排版单位，保证切换时不破坏布局。
- 基准 14px（对齐 `docs/ref/frontend/zcode-frontend.md:111`），语义阶梯为标题、常规、说明、小字、极小字。

### 4.4 圆角、阴影、动效

- 圆角：`--radius-sm/md/lg/xl` 四级（6/8/12/14px），全圆用于徽标与图标按钮。
- 阴影：sm/md/lg/xl 四级，另设 `--shadow-frost` 供浮层。
- 统一缓动 `cubic-bezier(0.16, 1, 0.3, 1)`（快速起始 + 弹性结束），时长分 150/250/350ms 三档。这是 LiveAgent 最能形成品牌一致性的单一决策，见 `docs/ref/frontend/live-agent/design-style-analysis.md:127-131`。
- **毛玻璃收敛**：只在顶部栏与浮层（对话框背景、抽屉）使用低强度模糊（blur 8–12px），列表项与卡片一律不用。理由见 `docs/ref/frontend/live-agent/design-style-analysis.md:389-397` 的性能权衡。
- `prefers-reduced-motion` 下全部动效降级为无过渡。

### 4.5 主题切换

- 三态：浅色 / 深色 / 跟随系统，持久化到本地。
- **预注水防闪烁**：在 `app.html` 的 `<head>` 内注入同步内联脚本，在首次绘制前写入根元素类名与 `color-scheme`，避免主题闪烁（对齐 `docs/ref/frontend/deeix-frontend.md:76`）。
- 切换时不做全局 `*` 过渡（会造成整页重绘卡顿），只对背景与边框色做短过渡。

### 4.6 滚动条

- 全局 6px 细滚动条，默认半透明，悬停加深。
- 终端态、代码块内使用更窄且默认隐藏的策略。

---

## 5. 页面布局与信息架构

### 5.1 总体形态：三栏 Shell

采用 zcode 式三栏（见 `docs/plan/web/web-frontend-borrow-analysis.md:18`）：

```
┌──────────┬────────────────────────────────┬────────────┐
│ 侧栏      │ 主内容区                         │ 检查器      │
│ 240px    │ 1fr                             │ 360px      │
│ 可折叠    │ 页头 + 内容 + 可选底栏            │ 可选/浮层   │
└──────────┴────────────────────────────────┴────────────┘
```

- **左栏**：一级导航（按域分组）+ 折叠控制 + 底部状态（连接态、版本）。可折叠为图标轨并悬停展开，宽度持久化。
- **中栏**：页面主体，页头含标题、面包屑、主操作；内容区按页面类型在列表 / 详情 / 三栏之间切换。
- **右栏（检查器）**：承载执行详情、时间线、审批、产物等**非一级路由**的上下文面板，避免为每个域单开页面。这是从 zcode SidePane 借鉴的关键决策（`docs/plan/web/web-frontend-borrow-analysis.md:18`）。

### 5.2 响应式断点

| 断点 | 布局 |
|---|---|
| > 1280px | 三栏全开 |
| 1024–1280px | 右侧检查器转为浮层抽屉 |
| 768–1024px | 侧栏折叠为图标轨 |
| < 768px | 单栏；侧栏改抽屉，检查器改底部 Sheet |

### 5.3 一级导航（十项）

严格对齐 `docs/plan/web/frontend-feature-list.md:13-27` 的域划分，执行工作台为默认落地页：

| 路由 | 页面 | 主要区块 |
|---|---|---|
| `/` | 入口重定向 | `+page.ts` 内 `redirect(307, '/executions')`，不渲染组件（`apps/web-app/src/routes/+page.ts:4-6`） |
| `/executions` | 执行工作台 | 概览指标条 + 执行列表（左）+ 执行详情（右检查器） |
| `/workflows` | 工作流 | 列表（卡片/表格切换）+ 详情 + 版本 + 草稿 + 图 |
| `/agent-loops` | Agent 回路 | 列表 + 详情 + 消息 + 变量 + 图 + 分析 + 检查点 |
| `/checkpoints` | 检查点与文件 | 检查点链 + 文件分区 + 变更时间线 + 差异 + 审批 |
| `/triggers` | 触发器与钩子 | 触发记录 + 钩子试发 |
| `/resources` | 模型与工具 | 模型档案 / 供应商 / 生成试算 / 工具 / 脚本 / 技能（内部分段） |
| `/templates` | 模板库 | 节点模板 / 触发器模板 / Agent 模板 / 注册表（内部分段） |
| `/insights` | 查询与审计 | 即席查询 / 审计报告 / 错误分析 / 性能（内部分段） |
| `/events` | 事件与系统 | 事件流 / 依赖 / 运维诊断（内部分段） |
| `/settings` | 设置 | 外观 / 执行默认 / 通知（左导航 + 右面板） |

### 5.4 页面内部布局范式

三类范式，覆盖全部页面：

1. **列表 + 检查器**（执行、回路、工作流）：左列表可筛选，右检查器随选中项切换。
2. **分段内容页**（资源、模板、洞察、事件）：页头下横排分段控件，切换内容面板。
3. **左导航 + 右面板**（设置）：窄侧导航 + 表单面板，对齐 `docs/ref/frontend/live-agent/design-style-analysis.md:202-213`。

### 5.5 全局能力

- **命令面板**（`components/layout/CommandPalette.svelte`）：跨执行、工作流、回路、导航项与设置的统一搜索入口，`⌘K` / `Ctrl+K` 唤起，含最近访问记录。
- **Toast**（`components/layout/Toaster.svelte` + `stores/toast.svelte.ts`）：右下角堆叠，success / error / warning / info 四类，可带操作按钮，默认 4.5s 自动消解（error 8s）。
- **加载态体系**：`Skeleton` 提供 line / block / circle 三种形状（可配行数与尺寸）；`EmptyState` 负责空态。**错误态与流式占位本阶段未建**——前者依赖真实请求失败路径，后者属流式阶段，见第 11.2 节。
- **面包屑**：由当前路由派生的导航项生成（`config/navigation.ts` 的 `navItemFor`），非手工维护。

---

## 6. 组件体系

三层，与 `docs/plan/web/web-frontend-borrow-analysis.md:22-26` 的分层一致。

### 6.1 基础层（约 18 个）

`src/lib/components/ui/`：Button、IconButton、Input、Textarea、Select、Switch、Card、Badge、Separator、Skeleton、EmptyState、Tooltip、DropdownMenu、Dialog、Sheet、DataTable，另加两个非组件的辅助模块 `variants.ts`（变体类名）与 `table.ts`（`Column<T>` 列定义）。

优先补齐的三件套（否则数据量撑不住，见 `docs/plan/web/web-frontend-borrow-analysis.md:24`）：**命令面板、数据表（含虚拟化接口）、消息/日志滚动器**。

**实际落地与计划的差异**（均已实现等价能力，不再单开组件）：

| 计划组件 | 实际做法 | 理由 |
|---|---|---|
| Tabs | `Segmented`（`components/ui/Segmented.svelte`） | 分段控件即 tablist 语义，`role="tablist"` 已在组件内声明 |
| ScrollArea | 全局细滚动条（`app.css` 的 `@layer base` 滚动条块与 `.scrollbar-none`） | 本阶段无独立滚动容器需求，避免多一层 DOM |
| Checkbox / Label | 未建 | 本阶段无表单提交场景；接 API 后随设置页表单一并补 |
| 虚拟化接口 | `DataTable` 仅预留列模型，未实现虚拟化 | 依赖真实数据量，见开放点 O7 |

### 6.2 布局层（7 个）

`src/lib/components/layout/`：AppShell、Sidebar、TopBar、PageHeader、SplitView、CommandPalette、Toaster。

`SidebarRail` 与 `InspectorPane` 未单开：折叠轨是 `Sidebar` 的一种渲染态（`components/layout/Sidebar.svelte` 中 `preferences.sidebarCollapsed` 分支），检查器容器统一为 `SplitView`（宽屏停靠 / 窄屏转 Sheet）。

### 6.3 领域层（13 个）

`src/lib/components/domain/`：StatusBadge、ExecutionCard、WorkflowCard、WorkflowGraph（内联 SVG 占位）、ToolCallCard、Timeline、DiffView、MessageBubble、MetricGrid、KeyValueList、ExecutionInspector、FilterBar、CursorPager。

`MetricCard` 合并进 `MetricGrid`（网格自带单元格渲染），`JsonViewer` 与 `SplitPane` 未建——前者等流式阶段与 Markdown 渲染一并决定（开放点 O4），后者由 `SplitView` 承担。

其中 `StatusBadge` 必须实现第 1 节要求的未知值兜底；`CursorPager` 对应后端"无总数、只有 `has_more`"的游标包络（`docs/plan/web/frontend-feature-list.md:34`），提供"加载更多 + 无总数提示"而非页码。

### 6.4 组件契约约定

- 所有组件以 `class` 透传追加样式，`cn()` 合并。
- 尺寸统一 `sm / md / lg`，变体命名沿用 shadcn 语义（default / secondary / outline / ghost / destructive）。
- 可交互元素统一聚焦环样式，禁止各组件自定义描边颜色。
- 组件内**禁止**出现十六进制或具名颜色字面量，一律走 Token。

---

## 7. 目录结构

```
apps/web-app/src/
├── app.css                       # Tailwind 入口 + Token 定义 + 基础层
├── app.html                      # 预注水脚本
├── lib/
│   ├── components/
│   │   ├── ui/                   # 基础原语（第 6.1 节）
│   │   ├── layout/               # 壳与导航（第 6.2 节）
│   │   ├── domain/               # 领域组件（第 6.3 节）
│   │   └── icons/                # 内联 SVG 图标
│   ├── stores/                   # runes 单例：theme、shell、toast、command、preferences
│   ├── fixtures/                 # 本阶段样例数据（后续阶段替换为 API 调用）
│   ├── types/                    # 视图模型类型（不重复声明契约类型）
│   └── utils/                    # cn、格式化、状态色、路由类型
└── routes/
    ├── +layout.svelte / +layout.ts    # 壳挂载、主题/字号应用、命令面板与 Toaster
    ├── +page.ts                       # 根重定向到 /executions（无需 +page.svelte）
    ├── executions/(+page.svelte, [id]/+page.svelte)
    ├── workflows/(+page.svelte, [id]/+page.svelte)
    ├── agent-loops/(+page.svelte, [id]/+page.svelte)
    ├── checkpoints/+page.svelte       # 内部分段：chain / files / approvals
    ├── triggers/+page.svelte          # 内部分段：records / hooks
    ├── resources/+page.svelte         # 内部分段：models / tools / scripts / skills
    ├── templates/+page.svelte         # 内部分段：all / node / trigger / agent / workflow
    ├── insights/+page.svelte          # 内部分段：query / audit / errors / performance
    ├── events/+page.svelte            # 内部分段：stream / dependencies / operations
    └── settings/+page.svelte          # 左导航 + 面板：appearance / execution / notifications / workspace
```

计划中的 `workflows/[id]/versions`、`agent-loops/[id]/messages` 一类子路由**未单独建路由**，改为详情页内的分段内容（`ExecutionInspector` 提供 overview / timeline / tools / analysis / state 五个分段）。理由：这些面板是同一对象的不同视图，单独建路由会让面包屑与返回栈膨胀，且后续接 API 时每个子路由都要重复取一次详情。若后续需要深链（分享某个版本），再拆成子路由并补 `?tab=` 兼容。

`fixtures/` 是本阶段的临时数据层，命名与结构直接对齐视图模型，后续接 API 时只替换数据来源，不动组件。

---

## 8. 任务分解

按依赖顺序推进，每步都是可独立验证的增量。

| 序号 | 任务 | 产出 | 验收 | 状态 |
|---|---|---|---|---|
| T1 | 样式底座 | `app.css`、`app.html` 预注水脚本（`:8-37`）、Tailwind 接入（`vite.config.ts:6`）、适配器改 static（`svelte.config.js:1,8`） | 深浅色切换无闪烁，`npm run build` 通过 | 已完成 |
| T2 | 主题与偏好 store | `stores/preferences.svelte.ts`、`stores/theme.svelte.ts`、`stores/ui.svelte.ts`、`stores/toast.svelte.ts` | 主题/字号/侧栏宽度持久化并恢复 | 已完成 |
| T3 | 图标与工具类 | `icons/paths.ts`（52 组图标）、`utils/cn.ts`、`utils/format.ts`、`utils/status.ts`、`utils/route.ts` | 图标覆盖导航与操作所需集合 | 已完成 |
| T4 | 基础组件库 | `components/ui/` 18 个原语 + `variants.ts` + `table.ts` | 每个组件在无数据下可渲染 | 已完成 |
| T5 | 应用壳 | AppShell、Sidebar、TopBar、PageHeader、SplitView | 三栏可折叠、响应式断点生效 | 已完成 |
| T6 | 全局能力 | CommandPalette、Toaster | 快捷键唤起、Toast 堆叠消解 | 已完成 |
| T7 | 领域组件 | `components/domain/` 13 个 | StatusBadge 对未知状态有中性兜底 | 已完成 |
| T8 | 页面骨架 | 十个一级页面 + `executions/[id]`、`workflows/[id]`、`agent-loops/[id]` | 每页有页头、内容区、加载/空态 | 已完成 |
| T9 | 全量校验 | `npm run check`、`npm run build`、`npm run lint`、`prettier --check` | 四项全绿 | 已完成 |

### 8.1 落地清单

新增/改动文件（路径相对 `apps/web-app/`）。不列行数——`prettier` 会整体重排，行数易失效；规模口径见表下说明。

| 分类 | 文件 |
|---|---|
| 样式底座 | `src/app.css`、`src/app.html`、`vite.config.ts`、`svelte.config.js`、`package.json` |
| 状态与偏好 | `src/lib/stores/preferences.svelte.ts`、`theme.svelte.ts`、`ui.svelte.ts`、`toast.svelte.ts` |
| 工具与配置 | `src/lib/utils/cn.ts`、`format.ts`、`status.ts`、`route.ts`；`src/lib/config/navigation.ts` |
| 视图模型 | `src/lib/types/models.ts` |
| 图标 | `src/lib/components/icons/paths.ts`、`Icon.svelte` |
| 基础组件 | `src/lib/components/ui/`：Badge、Button、Card、DataTable、Dialog、DropdownMenu、EmptyState、IconButton、Input、Progress、Segmented、Select、Separator、Sheet、Skeleton、Switch、Textarea、Tooltip，另加 `table.ts`、`variants.ts` |
| 布局组件 | `src/lib/components/layout/`：AppShell、Sidebar、TopBar、PageHeader、SplitView、CommandPalette、Toaster |
| 领域组件 | `src/lib/components/domain/`：StatusBadge、ExecutionCard、WorkflowCard、ExecutionInspector、WorkflowGraph、ToolCallCard、Timeline、DiffView、MessageBubble、MetricGrid、KeyValueList、FilterBar、CursorPager |
| 样例数据 | `src/lib/fixtures/`：clock、executions、workflows、agentLoops、checkpoints、resources、insights、triggers |
| 路由 | `src/routes/+layout.ts`、`+layout.svelte`、`+page.ts`；`executions`、`executions/[id]`、`workflows`、`workflows/[id]`、`agent-loops`、`agent-loops/[id]`、`checkpoints`、`triggers`、`resources`、`templates`、`insights`、`events`、`settings` |

规模口径：`src` 下（排除 `lib/api/schema.d.ts`）共 **76 个 `.svelte` / `.ts` 文件、9264 行**。

> 说明：`+page.svelte` 占位首页已删除——根路由在 `+page.ts` 里恒定重定向，保留该组件会残留硬编码颜色（`#333` / `#666`），与第 11.1 节验收项冲突。

---

## 9. 关键约束

- **代码语言**：代码、注释、日志、错误信息一律英文，禁止中文入代码（`AGENTS.md:13-14`）。
- **注释禁引文档**：注释只描述代码意图，禁止出现文档结构标识符（`AGENTS.md:9`）。
- **格式化**：tabs 缩进、单引号（`.prettierrc`）。
- **不手写契约类型**：视图模型放 `types/`，不得复制粘贴 `schema.d.ts` 中的结构。
- **链接必须过 `resolve()`**：组件内 `href` 一律写成 `resolve(...)`，类型为 `AppPath`（见第 1.1 节）。禁止因类型报错而退回裸字符串。
- **颜色只能来自 Token**：组件内出现 `#hex` / `rgb()` / 裸 `hsl(...)` 一律视为违规；唯一例外见开放点 O8。

---

## 10. 待拍板开放点

| 编号 | 问题 | 影响 | 建议 |
|---|---|---|---|
| O1 | 样例数据保留到何时 | 决定 `fixtures/` 的生命周期 | 建议接 API 时逐页替换，全部替换完成后整目录删除，不留兼容分支 |
| O2 | 图可视化选型（D3 / ECharts / 自研 SVG） | 工作流编辑器与路径分析的交互上限 | 本阶段用自研 SVG 占位；待图编辑需求明确后再定，避免提前锁死 |
| O3 | 检查器是常驻还是浮层 | 影响三栏的信息密度 | **已按建议落地**：`SplitView` 由 `preferences.inspectorPinned` 决定停靠，宽屏停靠、窄屏转 Sheet，页面可自行选择 |
| O4 | 是否引入 Markdown 渲染库 | 消息与产物展示 | 待流式阶段与 `web-app-streaming-markdown-design.md` 一并决定，本阶段只留渲染插槽 |
| O5 | `adapter-static` 与后续 SSR 需求冲突 | 部署形态 | **已按建议落地**（`svelte.config.js` 用 `adapter-static` + `fallback: 'index.html'`，`+layout.ts` 关闭 SSR）；若后续需 SSR 再改回 |
| O6 | 命令面板的数据源 | 是否依赖统一搜索后端 | **已按本阶段方案落地**：检索本地 fixtures + 静态导航 + 设置项；接 API 后并入统一搜索通道 |
| O7 | 虚拟化的触发阈值 | 长列表性能 | 建议超过 200 行启用；阈值集中配置，不散落在组件内 |
| O8 | `theme-color` meta 的两处颜色字面量 | 违反「无硬编码颜色」验收项 | 现状：`src/app.html` 首绘前脚本与 `src/lib/stores/theme.svelte.ts` 的 `DARK_META` 各存一份 `#ffffff` / `#16181d`。meta 标签在首绘前写入且不接受 CSS 变量，故无法完全去字面量。建议后续统一到一处常量并加注释说明例外；若不接受例外，可改为运行时读 `--background` 通道拼 `hsl()`，代价是深色判定与 CSS 加载时序耦合 |

---

## 11. 验收标准与后续衔接

### 11.1 本阶段验收

在 `apps/web-app` 下实测结果（命令与输出）：

| 验收项 | 命令 | 实测结果 |
|---|---|---|
| 类型检查 | `npm run check` | `svelte-check found 0 errors and 0 warnings` |
| lint | `npm run lint` | 无输出，0 error 0 warning |
| 格式 | `npx prettier --check src` | `All matched files use Prettier code style!` |
| 构建 | `npm run build` | `Wrote site to "build"` + `✔ done`（约 3.3s，产物 CSS 约 40 KB） |
| 路由可访问 | `vite preview` + 无头 Chromium 逐个 `--dump-dom` | `/`、`/executions`、`/workflows`、`/agent-loops`、`/checkpoints`、`/triggers`、`/resources`、`/templates`、`/insights`、`/events`、`/settings` 全部渲染出 DOM（25–48 KB），无 500 / 白屏 |
| 无硬编码颜色 | 全量检索 `#hex`、`rgb()`、裸 `hsl(...)` | 组件内仅剩 `hsl(var(--token))` 形式；浮层遮罩已收敛为 `--overlay`（`app.css` 的 `:root` / `.dark` 两处定义）；遗留两处 `theme-color` 字面量登记为开放点 O8 |
| 未知状态兜底 | `src/lib/utils/status.ts` 的 `statusTone()` | 空值返回 `neutral`，未收录值经 `?? 'neutral'` 兜底；`StatusBadge.svelte` 消费该结果 |

- 深浅色切换无闪烁由 `src/app.html` 的首绘前内联脚本（`<script>` 块位于 `<head>` 内）保证；主题/密度/侧栏宽度持久化在 `src/lib/stores/preferences.svelte.ts`。

> 未在本阶段验证：真实 API 数据下的分页与虚拟化表现、移动端真机断点、屏幕阅读器走查。这三项依赖后续接 API 与真机环境。

### 11.2 后续衔接

- **接 API 阶段**：`fixtures/` → `src/lib/api/` 客户端 + 领域封装，视图模型不变。同步补齐两件本阶段留白：错误态组件（带重试）与 `DataTable` 虚拟化（开放点 O7）。
- **流式阶段**：Timeline 与 MessageBubble 预留的渲染插槽接 `sse.ts` / `ws.ts`；Markdown 渲染选型与 `JsonViewer` 一并决定（开放点 O4）。
- **深链需求出现时**：把详情页分段拆成子路由，并按第 7 节的说明补 `?tab=` 兼容。
- **深度能力阶段**：工具调用卡、Diff 与产物预览、大纲导轨按 `docs/plan/web/web-frontend-borrow-analysis.md:71-73` 的第二批推进。
- **收尾**：`fixtures/` 全部替换完成后整目录删除（开放点 O1），并复核开放点 O8 是否接受 `theme-color` 字面量例外。
