# Web App 样式、布局与组件实现方案

> 目标：为 `apps/web-app`（SvelteKit 2 + Svelte 5）落地**样式体系、页面布局、组件体系**三层设计，并给出可执行的任务分解。
> UI 阶段已完成；其后的正式化收敛已把数据层切换为真实 API（样例数据与开发模式开关已删除），流式渲染、虚拟化与 API 接入三处临时形态已收敛，见 `docs/plan/web/web-app-formalization-design.md`。
> 上游依据：`docs/plan/web/frontend-feature-list.md`（页面与功能边界）、`docs/plan/web/web-frontend-borrow-analysis.md`（借鉴结论）、`docs/ref/frontend/*`（原始实现）、`docs/plan/web-app-integration.md`（集成与部署约定）。

---

## 0. 文档定位与范围

本文只解决三件事：**长什么样**（样式）、**怎么摆**（布局）、**用什么拼**（组件）。

本文档范围内：

- Design Token 体系（颜色、字体、间距、圆角、阴影、动效）。
- 应用壳（三栏 Shell、侧栏、顶栏、命令面板、Toast）。
- 基础组件库与领域组件库。
- 十一个一级页面（含对话页）及其详情子路由的静态结构与数据流。

正式化收敛后，以下原范围外事项已落地，不再是后续阶段：API 客户端、信封拆包、鉴权注入、SSE 流式接入、错误态组件、`DataTable` 虚拟化、Markdown 渲染与结构化载荷展示。对话优先的落地页与导航重组一并完成。

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

### 1.2 正式化收敛后的现状（UI 阶段之后的变化）

| 事实 | 依据 |
|---|---|
| `src` 下共 118 个源文件；`lib/api/` 含类型化客户端、信封拆包、推送读取与帧切分；`lib/services/` 共 13 个 API 直连的领域封装 | `apps/web-app/src` 目录清点 |
| 样例数据目录与开发模式开关已删除，正式应用无静默回退样例；样例仅保留于 `apps/web-app-preview` 的模拟客户端内 | `apps/web-app/src/lib` 目录清点（无 `fixtures/`、`stores/app.ts`） |
| 对话页已上线：`routes/chat/` + `components/chat/`（作曲器、推理块、流式 Markdown、转录滚动器），根路由重定向到 `/chat`，导航首组为 Conversation | `apps/web-app/src/routes/+page.ts`；`apps/web-app/src/lib/config/navigation.ts` |
| 运行时依赖新增 Markdown 渲染器与 OpenAPI 请求库 | `apps/web-app/package.json` 的 dependencies |
| 错误态已建：`components/ui/ErrorState.svelte` + 路由级 `routes/+error.svelte`，聊天页加载失败带重试入口 | 对应文件 |
| 未知状态兜底与无硬编码颜色保持：组件内无十六进制颜色字面量；`theme-color` 仅剩两处 meta 名引用，属可接受例外 | 全量检索 |

---

## 2. 设计目标与非目标

### 2.1 目标

- **一致性**：所有页面共享同一套 Token，禁止在组件内写死颜色值。
- **信息密度**：面向开发者的运维/调试工作台，默认紧凑档，允许放宽。
- **状态可读**：执行状态、变更类型、工具类型一眼可辨，且与终端配色解耦。
- **可扩展**：新增域页面只需加路由 + 复用组件，不改壳。
- **API 优先**：正式应用只经类型化客户端访问后端；空结果走空态，失败走错误态并可重试，不静默回退样例。

### 2.2 非目标

- 不实现登录、计费、多媒体生成、桌面专属能力（功能清单第 5 节）。
- 不实现需要新后端的功能：文件编辑器、触发器启停、技能安装、模板评分、通知收件箱。
- 全局错误聚合（审计报告、错误分析）等待后端聚合器，当前返回空列表而非占位数据。
- 多会话并发流与自动重连不在范围内，失败保持手动重试语义。

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

正式化收敛追加的两项运行时依赖：`markstream-svelte@2.0.13`（流式 Markdown 渲染）、`openapi-fetch@^0.17.0`（类型化 API 客户端），与预览工程已验证版本对齐。

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

### 5.3 一级导航（十一项）

严格对齐 `docs/plan/web/frontend-feature-list.md:13-27` 的域划分，对话优先 IDE 为默认落地页（见 `docs/plan/web/chat-ide-primary-design.md`）：

| 路由 | 页面 | 主要区块 |
|---|---|---|
| `/` | 入口重定向 | `+page.ts` 内 `redirect(307, '/chat')`，不渲染组件（`apps/web-app/src/routes/+page.ts`） |
| `/chat` | 对话 | 会话 + 作曲器 + 流式尾部 + 会话检查器（首导航组 Conversation） |
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
- **加载态体系**：`Skeleton` 提供 line / block / circle 三种形状（可配行数与尺寸）；`EmptyState` 负责空态；`ErrorState` 负责错误态（带重试）并有路由级 `+error.svelte` 兜底；流式尾部由转录滚动器的 tail 插槽承载。
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
| 虚拟化接口 | `DataTable` 已实现等高窗口化（阈值驱动自动启用 + 手动覆盖 + 粘性表头），阈值集中在 `config/virtualization.ts` | 开放点 O7 已关闭 |

### 6.2 布局层（7 个）

`src/lib/components/layout/`：AppShell、Sidebar、TopBar、PageHeader、SplitView、CommandPalette、Toaster。

`SidebarRail` 与 `InspectorPane` 未单开：折叠轨是 `Sidebar` 的一种渲染态（`components/layout/Sidebar.svelte` 中 `preferences.sidebarCollapsed` 分支），检查器容器统一为 `SplitView`（宽屏停靠 / 窄屏转 Sheet）。

### 6.3 领域层（14 个）

`src/lib/components/domain/`：StatusBadge、ExecutionCard、WorkflowCard、WorkflowGraph（内联 SVG 占位）、ToolCallCard、Timeline、DiffView、MessageBubble、MetricGrid、KeyValueList、ExecutionInspector、FilterBar、CursorPager、JsonViewer。

其中 `MessageBubble` 已收敛为单一渲染入口：助手消息走流式 Markdown 封装，用户消息保持纯文本并保留提及分段，工具消息走等宽结构样式，推理文本独立为可折叠推理块；`ToolCallCard` 的输入输出经 `JsonViewer` 展示（折叠、复制语义、截断）。

其中 `StatusBadge` 必须实现第 1 节要求的未知值兜底；`CursorPager` 对应后端"无总数、只有 `has_more`"的游标包络（`docs/plan/web/frontend-feature-list.md:34`），提供"加载更多 + 无总数提示"而非页码。

### 6.4 对话层（4 个）

`src/lib/components/chat/`：Composer（作曲器、附件、斜杠命令、草稿）、ReasoningBlock（可折叠推理）、StreamMarkdown（Markdown 唯一入口，主题随浅深模式派生）、TranscriptScroller（分组窗口、估算高度、尾部跟随与回底计数）。

### 6.5 组件契约约定

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
│   ├── api/                      # 类型化客户端、信封拆包、推送读取、帧切分（含生成的 schema.d.ts）
│   ├── components/
│   │   ├── ui/                   # 基础原语（第 6.1 节，含 ErrorState）
│   │   ├── layout/               # 壳与导航（第 6.2 节）
│   │   ├── domain/               # 领域组件（第 6.3 节，含 JsonViewer）
│   │   ├── chat/                 # 对话组件（第 6.4 节）
│   │   └── icons/                # 内联 SVG 图标
│   ├── config/                   # 导航、虚拟化阈值等集中配置
│   ├── services/                 # API 直连的领域封装 + 流式帧分类（含单测）
│   ├── stores/                   # runes 单例：theme、shell、toast、command、preferences、sessions、stream-run 等
│   ├── types/                    # 视图模型类型（不重复声明契约类型）
│   └── utils/                    # cn、格式化、状态色、路由类型、附件与提及
└── routes/
    ├── +layout.svelte / +layout.ts    # 壳挂载、主题/字号应用、命令面板与 Toaster
    ├── +page.ts                       # 根重定向到 /chat（无需 +page.svelte）
    ├── +error.svelte                  # 路由级错误边界
    ├── chat/+page.svelte              # 对话优先 IDE
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

`fixtures/` 与开发模式开关已随正式化收敛删除；样例数据仅保留于 `apps/web-app-preview` 的模拟客户端内，正式应用的领域服务与页面无需感知数据源差异。

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
| T8 | 页面骨架 | 十一个一级页面（含对话页） + `executions/[id]`、`workflows/[id]`、`agent-loops/[id]` | 每页有页头、内容区、加载/空态，失败走错误态 | 已完成 |
| T9 | 全量校验 | `npm run check`、`npm run build`、`npm run lint`、`prettier --check` | 四项全绿 | 已完成（UI 阶段实测，见第 11.1 节；正式化收敛后的复验待补，见第 8.2 节） |
| T10 | 正式化收敛 | 依赖补齐、消息渲染合回、JsonViewer、生成流分支、表格虚拟化、服务去样例化、样例删除、落地页翻转 | 见第 8.2 节验收 | 已完成（代码收敛，静态复验待补） |

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
| 样例数据 | ~~`src/lib/fixtures/`：clock、executions、workflows、agentLoops、checkpoints、resources、insights、triggers~~（已删除，见 T10） |
| 路由 | `src/routes/+layout.ts`、`+layout.svelte`、`+page.ts`、`+error.svelte`；`chat`、`executions`、`executions/[id]`、`workflows`、`workflows/[id]`、`agent-loops`、`agent-loops/[id]`、`checkpoints`、`triggers`、`resources`、`templates`、`insights`、`events`、`settings` |
| 对话与数据层 | `src/lib/components/chat/`：Composer、ReasoningBlock、StreamMarkdown、TranscriptScroller；`src/lib/api/`：client、envelope、stream、sse；`src/lib/services/`：agent-loops、agentLoops、checkpoints、events、executions、favorites、insights、preferences、resources、search、streaming、templates、triggers、workflows |

规模口径：`src` 下（排除 `lib/api/schema.d.ts`）共 **118 个 `.svelte` / `.ts` 文件**。

### 8.2 正式化收敛（T10，设计见 `docs/plan/web/web-app-formalization-design.md`）

| 序号 | 任务 | 产出 | 验收 |
|---|---|---|---|
| 一 | 依赖与引用完整性 | 运行时依赖补齐、契约引用统一 | 静态引用无悬空包与悬空路径 |
| 二 | 消息渲染合回 | 消息气泡具备 Markdown、提及、附件与操作入口，历史与流式同路 | 回放与直播排版一致，用户消息仍纯文本 |
| 三 | 结构化展示新增 | 独立 JsonViewer 并接入工具调用卡输入输出 | 大载荷截断折叠可用 |
| 四 | 生成流分支补齐 | 推理与用量帧进入回调面 | 单次生成可显示推理与用量 |
| 五 | 数据表窗口化合回 | 等高窗口、粘性表头、手动覆盖 | 超阈值列表渲染行数有界 |
| 六 | 领域服务去样例化 | 移除样例导入与开发模式分支，失败走错误态 | 无正式服务引用样例 |
| 七 | 样例目录移除 | 删除样例目录与模式存储 | 仅预览保留样例 |
| 八 | 落地页翻转 | 根路由与导航按对话优先调整 | 根地址进入对话空态 |

> 待补：T10 落地后尚未执行 `npm run check` / `build` / `lint` / `test`（环境缺少 `node_modules` 且无外网安装依赖），上述验收目前以读码核查为准，复验通过前不得视为关闭。

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

| 编号 | 问题 | 影响 | 结论 |
|---|---|---|---|
| O1 | 样例数据层去留 | 正式应用是否保留 fixtures 回退 | 已关闭：`fixtures/` 与开发模式开关已删除，样例仅保留于预览工程 |
| O2 | 图可视化选型 (D3 / ECharts / 自研 SVG) | 工作流编辑器与路径分析的交互上限 | 未决：仍用自研 SVG 占位；待图编辑需求明确后再定，避免提前锁死 |
| O4 | 是否引入 Markdown 渲染库 | 消息与产物展示 | 已关闭：选用 `markstream-svelte`，流式 Markdown 封装为唯一入口，结构化载荷由独立 JsonViewer 承担 |
| O7 | 虚拟化的触发阈值 | 长列表性能 | 已关闭：超过 200 行启用；阈值集中在 `config/virtualization.ts`，转录变高窗口、表格等高窗口 |
| O8 | `theme-color` 字面量例外 | 颜色约束的完整性 | 已关闭：仅剩两处 meta 名引用（非颜色值），接受为例外 |

---

## 11. 验收标准与后续衔接

### 11.1 本阶段验收

在 `apps/web-app` 下实测结果（命令与输出）。以下为 UI 阶段落点时的记录；T10 正式化收敛后的复验待补（见第 8.2 节说明）。

| 验收项 | 命令 | 实测结果 |
|---|---|---|
| 类型检查 | `npm run check` | `svelte-check found 0 errors and 0 warnings` |
| lint | `npm run lint` | 无输出，0 error 0 warning |
| 格式 | `npx prettier --check src` | `All matched files use Prettier code style!` |
| 构建 | `npm run build` | `Wrote site to "build"` + `✔ done`（约 3.3s，产物 CSS 约 40 KB） |
| 路由可访问 | `vite preview` + 无头 Chromium 逐个 `--dump-dom` | `/`、`/executions`、`/workflows`、`/agent-loops`、`/checkpoints`、`/triggers`、`/resources`、`/templates`、`/insights`、`/events`、`/settings` 全部渲染出 DOM（25–48 KB），无 500 / 白屏 |
| 无硬编码颜色 | 全量检索 `#hex`、`rgb()`、裸 `hsl(...)` | 组件内仅剩 `hsl(var(--token))` 形式；浮层遮罩已收敛为 `--overlay`（`app.css` 的 `:root` / `.dark` 两处定义）；两处 `theme-color` 均为 meta 名引用，接受为例外（开放点 O8 已关闭） |
| 未知状态兜底 | `src/lib/utils/status.ts` 的 `statusTone()` | 空值返回 `neutral`，未收录值经 `?? 'neutral'` 兜底；`StatusBadge.svelte` 消费该结果 |

- 深浅色切换无闪烁由 `src/app.html` 的首绘前内联脚本（`<script>` 块位于 `<head>` 内）保证；主题/密度/侧栏宽度持久化在 `src/lib/stores/preferences.svelte.ts`。

> 未在本阶段验证：真实 API 数据下的分页与虚拟化表现、移动端真机断点、屏幕阅读器走查。这三项依赖后续接 API 与真机环境。

### 11.2 后续衔接

- **复验（待补）**：T10 落地后执行 `npm run check`、`npm run build`、`npm run lint`、`npm run test` 四项，全绿后关闭 T10。
- **预览同步**：正式应用变更后按既有同步脚本更新 `apps/web-app-preview`（样例目录与模拟客户端为预览独有，不被覆盖）。
- **深链需求出现时**：把详情页分段拆成子路由，并按第 7 节的说明补 `?tab=` 兼容。
- **深度能力阶段**：工具调用卡、Diff 与产物预览、大纲导轨按 `docs/plan/web/web-frontend-borrow-analysis.md:71-73` 的第二批推进；图表与公式能力缺失为有意取舍，待真实需求出现后再评估可选依赖。
- **范围外（仍未决）**：多会话并发流与自动重连；全局错误聚合需后端聚合器；图可视化选型（开放点 O2）。
