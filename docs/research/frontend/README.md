# 前端图工作流与执行实例管理对比研究

本目录基于当前项目前端源码，对照 n8n 与 Dify 的前端实现，分析「图工作流（画布）」与「执行实例管理」相关页面的 UI/UX 逻辑、页面布局与样式风格。

## 1. 研究范围

| 类别 | 对象 |
| --- | --- |
| 当前项目 | `apps/web-app`（SvelteKit + Svelte 5 + Tailwind v4） |
| 参考实现 A | `n8n-io/n8n`（Vue 3 + VueFlow + SCSS Modules + `@n8n/design-system`） |
| 参考实现 B | `langgenius/dify`（React + Next.js + reactflow + Zustand + Tailwind v4） |

对比聚焦两类页面：

- **图工作流**：工作流列表、工作流详情、图查看/编辑、画布工具条、节点与边、节点配置面板。
- **执行实例管理**：执行列表/工作台、执行详情、执行日志、时间线、运行回放、重跑与调试。

不包含后端执行语义、API 协议、权限模型与数据库设计；仅在解释前端行为必要时提及。

## 2. 参考快照与许可

| 仓库 | 提交 | 拉取方式 | 许可 |
| --- | --- | --- | --- |
| n8n | `9a489b65ed539a042e819012558cfc6985aab78d` | `--depth 1 --filter=blob:none --sparse`，仅检出 `packages/frontend/editor-ui/src`、`packages/workflow/src`、`packages/frontend/@n8n/design-system` | Sustainable Use License（见 `docs/ref/n8n/LICENSE.md`） |
| Dify | `b535806c4acd03f3970dedc99537e1c1928034ce` | 同样 sparse，仅检出 `web` | Dify Open Source License（见 `docs/ref/dify/LICENSE`） |

复制到 `docs/ref/` 的文件仅用于本地阅读与对照，不参与构建，也不得直接复制进产品代码。具体清单见 [07-参考源码索引.md](07-参考源码索引.md)。

## 3. 引用约定

- 当前项目路径相对仓库根，例如 `apps/web-app/src/lib/components/domain/GraphExplorer.svelte:842`。
- 参考实现路径指向已复制的副本，例如 `docs/ref/n8n/packages/frontend/editor-ui/src/features/workflows/canvas/components/Canvas.vue:2069`。
- 所有事实性结论均给出 `文件:行号`；无法从源码确认的内容明确标注为「未确认」，不做推测性断言。
- 版本局限：两个上游仓库均为浅克隆快照，行号只对本次快照有效。

## 4. 总体结论

### 4.1 能力定位差异

| 维度 | 当前项目 | n8n | Dify |
| --- | --- | --- | --- |
| 产品重心 | 通用 Agent/工作流运行时，图与执行可观测性并重 | 工作流自动化编辑器，画布即主界面 | LLM 应用编排，画布 + 应用日志 |
| 画布角色 | 详情页中的一个 Tab，可切换图表/编辑/版本/草稿/运行 | 全屏主界面，画布承载执行与日志 | 全屏主界面，画布承载运行面板与节点面板 |
| 执行详情形态 | 右侧 Inspector，12 个 Tab（按 kind 取 6/9/10 个） | 画布下方 LogsPanel + 独立执行页预览 | 底部 RunPanel 三 Tab + 执行日志页抽屉 |
| 信息密度 | 高（多 Tab、多面板、A/B 对比） | 高（树形日志、NDV、多浮层） | 中高（折叠节点卡、追踪树） |

### 4.2 八条核心判断

1. **执行可观测性维度，当前项目明显更深。** 当前项目定义 12 个执行 Tab，workflow 形态展示 9 个、agent 10 个、chat 6 个（`apps/web-app/src/lib/config/execution-tabs.ts:9-21`、`:45-77`），并提供回放游标、倍速、热力、关键路径、决策点等画布叠加（`ExecutionInspector.svelte:163-165`、`:251-271`）。n8n 的画布执行视图以日志树 + 详情为主（`LogsOverviewRow.vue:144-185`、`LogDetailsPanel.vue:245-264`），Dify 的执行详情只有 RESULT/DETAIL/TRACING 三 Tab（`run/index.tsx:183-213`）。
2. **「画布内完成操作」的连贯性，当前项目弱于两家。** n8n 把 LogsPanel 作为 `WorkflowLayout` 的 `#footer` 插槽常驻画布下方（`WorkflowLayout.vue:115-116`），Dify 把 RunPanel 与 NodePanel 绝对定位在画布内（`panel/index.tsx:124`、`run/index.tsx:183`）。当前项目的执行信息集中在右侧 Inspector，图与执行分属不同 Tab（`WorkflowGraphTab.svelte`、`ExecutionInspector.svelte`）。
3. **画布视觉精细度存在代差。** 当前节点固定 120×40、字号 10、背景硬编码 `#18181b`（`lib/graph/canvas-style.ts:12-20`）；n8n 节点 1.5px 边框、6px 选中环、`conic-gradient` 旋转执行边框且按缩放补偿（`_canvasNodeStyles.scss:4`、`:24-30`、`:57-63`）；Dify 节点固定 240px 宽、`rounded-[15px]`、`shadow-xs → hover:shadow-lg`（`workflow-preview/components/nodes/base.tsx:36-47`）。
4. **边状态表达，当前项目只有静态类，缺少渐变与缩放补偿。** 当前边 `width 1.5`、`bezier`、三角箭头（`canvas-style.ts:106-111`）；n8n 非主连接虚线 `5,6`、成功/固定色、按缩放计算亮度（`CanvasEdge.vue:35`、`:69`、`:110-123`）；Dify 用源/目标状态生成线性渐变、`strokeWidth 2`、`curvature 0.16`（`custom-edge.tsx:19-26`、`:30-47`、`:78-79`）。
5. **执行列表的浏览模型不同。** 当前项目是「卡片网格 / 表格」切换 + 游标分页 + SSE 脏标记（`routes/executions/+page.svelte:35`、`:479-490`）；n8n 是固定 310px 侧栏 + IntersectionObserver 无限滚动 + 动画列表（`WorkflowExecutionsSidebar.vue:60`、`:231`、`:262`）；Dify 是独立页面表格 + 分页 + 右侧 Drawer（`workflow-log/index.tsx:121`、`workflow-log/list.tsx:154-162`）。
6. **状态语义系统，当前项目统一但粒度较粗。** 当前项目以 `StatusBadge` + 少量 tone 表达（`ExecutionCard.svelte:60`、`WorkflowCard.svelte:40`）；n8n 用 8 种执行状态的图标/颜色字典（`GlobalExecutionsListItem.vue:76-112`）；Dify 用 `StatusDot` + 语义色，并在结果面板区分 7 种状态文案（`run/status.tsx:96-114`、`run/node.tsx:93`）。
7. **布局抽象层，n8n 最可复用。** n8n `BaseLayout` 以 `banners/header/sidebar/content/footer/aside/overlays` 具名插槽 + CSS grid areas 组织（`BaseLayout.vue:17-54`），`PageViewLayout` 再统一内容最大宽度与内边距（`PageViewLayout.vue:24-36`）。当前项目 `AppShell` 在组件内写死 Sidebar/TopBar/main/aside 与 `w-80 xl:w-96`（`AppShell.svelte:92-113`），扩展点少于插槽模型。
8. **设计令牌完整度高，但存在规范漂移。** 当前项目 `app.css` 已有完整语义色、圆角、间距、动效与 `prefers-reduced-motion`（`app.css:7-57`、`:80-81`、`:141-237`），但 `canvas-style.ts` 直接写 `#18181b`、`#52525b`（`:14`、`:166`），与 `docs/spec/web/style-guide.md` 的「禁止硬编码色值」冲突；n8n 用 `light-dark()`、`oklch(from ...)` 与缩放补偿变量（`_tokens.scss:78-91`、`WorkflowCanvas.vue:341`）；Dify 用 `@theme`/`@theme inline` + 语义类（`tailwind-core.css:36-67`、`nodes/base.tsx:37-72`）。

### 4.3 差距分级

| 级别 | 含义 | 代表项 |
| --- | --- | --- |
| P0 | 影响主路径效率或规范一致性，建议优先处理 | 画布硬编码色值；执行与图之间的上下文跳转成本；执行状态图标/语义不统一 |
| P1 | 明显提升可读性与操作效率 | 画布内执行日志面板；边状态渐变；节点选中/运行态视觉；快捷键体系 |
| P2 | 结构性改良，收益中等 | `AppShell` 插槽化；执行列表无限滚动；筛选 chip 化；统一空/载入态 |
| P3 | 选择性借鉴 | 协作光标、评论、节点面板 presence、AI 助手浮层、画布分屏 |

详细论证与实施建议见各专题文档与 [06-差距清单与优先级.md](06-差距清单与优先级.md)。

## 5. 文档索引

| 文档 | 内容 |
| --- | --- |
| [01-信息架构与页面布局对比.md](01-信息架构与页面布局对比.md) | 应用外壳、导航、内容宽度、画布占位、详情呈现方式、响应式 |
| [02-图工作流画布与编辑体验对比.md](02-图工作流画布与编辑体验对比.md) | 渲染管线、节点、边、工具条、快捷键、编辑与配置面板、只读/历史/协作 |
| [03-执行实例管理对比.md](03-执行实例管理对比.md) | 列表、筛选、分页、实时刷新、状态表达、详情、日志、回放、重跑 |
| [04-组件与交互模式对比.md](04-组件与交互模式对比.md) | 卡片、徽标、面板、Tab、筛选、空态、Toast、浮层、协作 |
| [05-视觉风格与设计令牌对比.md](05-视觉风格与设计令牌对比.md) | 色彩模型、字体、圆角、阴影、密度、动效、暗色模式、规范漂移 |
| [06-差距清单与优先级.md](06-差距清单与优先级.md) | 可执行差距清单、优先级、落地顺序、不建议照搬项 |
| [07-参考源码索引.md](07-参考源码索引.md) | `docs/ref` 文件清单与用途映射 |

## 6. 应用方式

研究成果以 git patch 形式打包，包含本目录全部文档与 `docs/ref` 下的参考源码副本。应用后可直接在本地打开对照，不会影响任何构建产物。

```shell
git apply --stat frontend-research.patch   # 预览
git apply frontend-research.patch          # 应用
```

如需撤销：

```shell
git apply -R frontend-research.patch
```
