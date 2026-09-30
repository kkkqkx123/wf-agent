# 前端可视化 P2 阶段细化方案与落地说明

> 依据 `docs/ref/frontend-viz/`（尤其是 `06-可借鉴Dify与n8n的设计.md` 第 6 节优先级清单）编写。
> P2 共三项：**BaseLayout 插槽化**（源自 n8n §3.1）、**并排执行对照**（源自 n8n §3.2）、
> **品牌色锚点**（源自 n8n §5.1）。
> 本文先给细化方案，再记录实际落地结果（含文件与行号），最后给出全部决策结论。
>
> 本阶段为纯前端改动，未触碰后端 Rust 代码，因此未执行后端编译。
> 后端事实核对见第 1 节；凡后端不支持的能力，前端只做到“诚实上限”，不伪造数据。

---

## 0. 目标与范围

| 编号 | P2 项             | 来源     | 目标收益   | 本次状态                            |
| ---- | ----------------- | -------- | ---------- | ----------------------------------- |
| P2-A | BaseLayout 插槽化 | n8n §3.1 | 布局灵活度 | 已落地（壳可组合，默认视觉零差异）  |
| P2-B | 并排执行对照      | n8n §3.2 | 调试对比   | 已落地（SplitView dual + 执行对照） |
| P2-C | 品牌色锚点        | n8n §5.1 | 辨识度     | 已落地（`--brand` 令牌 + 三处落点） |

范围内只做这三项；P0/P1/P3 已交付项、§3.3 的画布内极简工具条、§2.3 的协作光标，均不在本阶段。

三项之间无依赖，可并行落地；实际按 A→B→C 顺序实施。

---

## 1. 事实基线（前端现状与后端契约，逐条带出处）

### 1.1 壳只有唯一调用方，写死三件套

`apps/web-app/src/routes/+layout.svelte:38-40` 是 `AppShell` 的唯一调用方
（全仓 `AppShell` 引用仅此一处）。
`AppShell.svelte:61-72` 写死“侧栏 + 顶栏 + 主区”：`Sidebar` 包在
`hidden lg:flex` 内，`TopBar` 常驻 `h-12`，`main` 直接渲染 `children`。
`HelpModal` 与移动端 `Sheet` 导航（`:75-111`）常驻，无扩展缝。

### 1.2 详情对照只有单检视器

`SplitView.svelte` 只有 `children + inspector` 两槽（`:10-19`），
宽屏停靠、窄屏转 `Sheet` 覆盖（`:36-90`）。
四个调用方（`workflows/+page`、`chat/+page`、`executions/+page`、`agent-loops/+page`）
均为单详情语义；其中执行页（`executions/+page.svelte:160-265`）是
“列表 + 单详情”：`selectedId` 拉一份 `getExecutionDetail` 进 `ExecutionInspector`。

版本对照页（`WorkflowVersionsPanel.svelte:63-66`）用 `buildVersionDiffView`
把版本间 diff 叠加到**当前 live 图**上做单图合并展示；`diffWorkflowVersions`
（`services/workflows.ts:201-251`）在内部取两个版本定义做结构 diff，
但只返回 diff，不返回两端图。

### 1.3 令牌无品牌色，主按钮与 Logo 共用中性色

`apps/ui/src/styles/tokens.css` 无 `--brand`（全仓零命中）；
`app.css:7-43` 的 `@theme inline` 无 `brand` 映射；
`variants.ts:9-18` 的 `BUTTON_VARIANT` 无 `brand`，主 CTA 与 Logo 方块
（`Sidebar.svelte:109-113` 的 `bg-primary` 方块、
`workflows/[id]/+page.svelte` 的 Run 按钮默认 `variant`）均为中性 `primary`。

`EmptyState.svelte:30-34` 的图标容器为中性 `bg-muted`，
全仓 13 处调用，形态一致。

### 1.4 改动前的问题清单

| 问题                                  | 出处（改动前）                            | 后果                    |
| ------------------------------------- | ----------------------------------------- | ----------------------- |
| 要“无侧栏沉浸画布 / 三栏对照”需改骨架 | `AppShell.svelte:61-72`                   | 布局实验只能动公用壳    |
| 执行只能一次看一条详情                | `executions/+page.svelte` 单 `selectedId` | 历史与当前无法并排比对  |
| 版本合并图叠在 live 图上              | `WorkflowVersionsPanel.svelte:63`         | live 有未存草稿时会误导 |
| 无品牌锚点，全中性冷灰蓝              | tokens 无 `brand`                         | 第一眼无记忆点          |

---

## 2. 差距分析（对照 Dify / n8n）

| 能力     | Dify                 | n8n                                 | 改动前 wf-agent | 本次做法                           |
| -------- | -------------------- | ----------------------------------- | --------------- | ---------------------------------- |
| 布局骨架 | 画布悬浮头，不占栅格 | `BaseLayout` 具名插槽，可组出多形态 | 写死三件套      | 取 n8n：壳接受四插槽，默认回落原样 |
| 执行对照 | 单 RUN 追踪树        | 列表 + 预览分栏，可并排/重叠对照    | 单检视器        | 取 n8n：SplitView dual，双详情并排 |
| 品牌表达 | 轻透留白             | 一处高饱和品牌色打 CTA/Logo         | 全中性          | 取 n8n：一处 `--brand`，只打三处   |

一处刻意不照抄：n8n 的可缩放侧栏（42↔200px 拖拽）与底部 `LogsPanel`。
前者已有折叠/宽度记忆（`preferences.sidebarCollapsed/sidebarWidth`），
后者执行日志已有检查器内 Timeline/Tools，两者均不重做。

---

## 3. 决策记录

| #   | 议题                   | 决策                                                         | 依据                                                                     |
| --- | ---------------------- | ------------------------------------------------------------ | ------------------------------------------------------------------------ |
| D1  | 插槽用何种形态         | `header/sidebar/aside/overlays` 四可选 Snippet，未传回落默认 | 06 §3.1 点名此四槽；Svelte 5 Snippet 已就绪                              |
| D2  | `+layout` 是否显式注入 | 保持只注 `children`，其余走壳内默认                          | 壳唯一调用方即 `+layout`；显式注四槽只是重复导入，不加信息               |
| D3  | 沉浸画布本阶段做不做   | 只留能力，不改任何页的 chrome                                | §3.3 不在 P2 清单；改默认 chrome 属视觉回归风险                          |
| D4  | dual 第二 pane 的形态  | 与主详情同宽同容器，宽屏并排、窄屏同 Sheet 内上下叠          | 与现有停靠/覆盖二态一致，不新增第三种布局状态                            |
| D5  | 对照选哪对内容         | 执行 A/B 双详情（历史 vs 当前），版本合并图保持不动          | 执行页已在 SplitView 内，零结构迁移；版本真双图需新增版本图解析，超出 P2 |
| D6  | B 的选择器放哪里       | Compare 开关放页头动作区，B 下拉放筛选条下方独立行           | 不动 `ExecutionCard`，不污染列表行                                       |
| D7  | 品牌色值               | 浅 `--brand: 24 95% 53%`（`#f97316` 级），深 `24 90% 62%`    | 06 §5.1 点名此饱和度；深色提亮保可读                                     |
| D8  | 品牌落点范围           | 仅 Logo 方块、Run 主按钮、两处列表空状态图标                 | n8n 原则即“一处锚点”，铺开即稀释                                         |
| D9  | 空状态是否全改         | 加 `tone` 可选 prop，默认中性，仅两处传 `brand`              | 13 处全改属视觉回归；opt-in 可控                                         |
| D10 | preview 工程           | 不同步改代码，只同步文档                                     | preview 是旧镜像（P1 D9、P3 D7），只镜像代码                             |

---

## 4. P2-A：BaseLayout 插槽化

### 4.1 壳的新契约

`AppShell` 新增四个可选插槽，语义对齐 n8n `BaseLayout`：

- `header`：未传渲染 `TopBar`；传空即隐藏（沉浸画布预留）。
- `sidebar`：未传渲染默认 `Sidebar`（含 `hidden lg:flex` 包裹）；传空即隐藏。
- `aside`：未传不渲染；传入时在主区右侧开右轨（带左 border 的可滚列）。
- `overlays`： additive，始终保留 `HelpModal` 与移动端导航 `Sheet`，
  自定义叠层追加其后，调用方自管定位。

主区结构：无 `aside` 时与改动前像素一致（`header` + `main` 纵列）；
有 `aside` 时主行变为 `main + aside` 横排，`main` 保持 `flex-1`。

### 4.2 调用方

`+layout.svelte` 不动（继续只注 `children`），注释点明四槽注入点。
本阶段无页面传入自定义槽，视觉零差异；对照视图的 `aside` 双画布属后续消费，
届时页面侧传入即可，壳不用再改。

---

## 5. P2-B：并排执行对照

### 5.1 `SplitView` dual

新增可选 `dual` + `secondaryTitle` + `secondary` + `secondaryOpen`：

- 未传 `secondary` 时行为与改动前完全一致（单栏回归零风险）。
- 宽屏停靠态：主区 + 详情一 + 分隔 + 详情二横排，两详情同用详情宽度。
- 窄屏覆盖态：同一 `Sheet` 内上下叠放两详情（各带标题行），不新增 Sheet。
- `oninspectorclose` 语义不变，只管主详情；副详情随 `dual`/`secondaryOpen` 开关。

### 5.2 执行页对照

页头动作区加 Compare 开关；开启后筛选条下方出现对照行：
B 下拉（候选为已筛列表 trừ A，`Select` 复用）、Swap 按钮（A/B 对调）。
`compareDetail` 复用 `getExecutionDetail` 独立拉取，失败走 toast，
不污染主详情状态。

副槽渲染：有 B 详情时渲染第二个 `ExecutionInspector`；
无 B 时渲染引导空状态（“在下方选择一次执行进行对照”），不自动请求。
A/B 相同 id 时副槽提示另选，不发重复请求。

版本合并图（`WorkflowVersionsPanel` + `buildVersionDiffView`）保持不动：
dual 与 merged 是两种对照模态，前者并排看两次完整执行，
后者单图看拓扑增删；本阶段只新增前者，不重构后者。

---

## 6. P2-C：品牌色锚点

### 6.1 令牌

`apps/ui/src/styles/tokens.css` 新增 `--brand` / `--brand-foreground`
（浅深两值，D7），`app.css @theme inline` 加
`--color-brand` / `--color-brand-foreground` 映射，
`variants.ts` 加 `brand` 按钮变体（`bg-brand` + `hover:bg-brand/90`）。
`docs/design-tokens.md` 补品牌行，并修正“`--running` 是唯一彩色”的旧述。

### 6.2 落点

- Logo 方块：`Sidebar.svelte` 由 `bg-primary` 改 `bg-brand`
  （文字同步 `text-brand-foreground`）。
- Run 主按钮：`workflows/[id]/+page.svelte` 的 Run 由默认变体改 `brand`；
  其余按钮（Import/New/Export/Start from workflow）保持原变体，
  全局只保留这一个高饱和 CTA。
- 空状态图标：`EmptyState` 加 `tone="brand"`（图标容器改品牌淡底 + 品牌字），
  仅工作流列表空态与执行列表空态传入，其余 11 处保持中性。

---

## 7. 代码落点（实际行号）

### 7.1 P2-A：插槽化

| 文件                                                     | 行号       | 动作                                                   |
| -------------------------------------------------------- | ---------- | ------------------------------------------------------ |
| `apps/web-app/src/lib/components/layout/AppShell.svelte` | `:15-27`   | Props 加四可选插槽 + 注释                              |
| 同上                                                     | `:69-99`   | 侧栏/顶栏回落默认、主行嵌 `main + aside` 右轨          |
| 同上                                                     | `:101-105` | `overlays` additive（`HelpModal` 与移动端 Sheet 常驻） |
| `apps/web-app/src/routes/+layout.svelte`                 | 末尾注释   | 点明四槽注入点，继续只注 `children`                    |

### 7.2 P2-B：并排对照

| 文件                                                      | 行号               | 动作                                                            |
| --------------------------------------------------------- | ------------------ | --------------------------------------------------------------- |
| `apps/web-app/src/lib/components/layout/SplitView.svelte` | `:18-21,32-35`     | `dual/secondaryTitle/secondary/secondaryOpen`                   |
| 同上                                                      | `:47-64`           | `secondaryVisible/dockedSecondary/sheetTitle` + 联合开关 effect |
| 同上                                                      | `:93-112`          | 宽屏第二停靠栏（同宽，仅标题无调宽钮）                          |
| 同上                                                      | `:113-133`         | 窄屏同一 Sheet 内上下叠放                                       |
| `apps/web-app/src/routes/executions/+page.svelte`         | `:43-45`           | `compareMode/compareId/compareDetail`                           |
| 同上                                                      | `:121-142`         | B 详情独立拉取（同 id 不请求，失败 toast）                      |
| 同上                                                      | `:156-186`         | `compared/compareOptions/toggleCompare/swapCompare`             |
| 同上                                                      | `:221-223`         | SplitView dual 接线                                             |
| 同上                                                      | `:248-249,278-303` | Compare 开关 + 对照行（B 下拉/Swap）                            |
| 同上                                                      | `:366-376`         | `secondary` 副槽（双 `ExecutionInspector` / 引导空态）          |

### 7.3 P2-C：品牌色

| 文件                                                      | 行号             | 动作                                  |
| --------------------------------------------------------- | ---------------- | ------------------------------------- |
| `apps/ui/src/styles/tokens.css`                           | `:43-44,112-113` | `--brand/--brand-foreground` 浅深两值 |
| `apps/web-app/src/app.css`                                | `:32-33`         | `--color-brand(-foreground)` 映射     |
| `apps/ui/src/components/variants.ts`                      | `:11`            | `brand` 按钮变体                      |
| `apps/ui/src/components/EmptyState.svelte`                | `:12,36-37`      | `tone` prop + 品牌淡底图标            |
| `apps/web-app/src/lib/components/layout/PageState.svelte` | `:16` + 透传     | `emptyTone` 转发                      |
| `apps/web-app/src/lib/components/layout/Sidebar.svelte`   | `:110`           | Logo 方块改品牌                       |
| `apps/web-app/src/routes/workflows/[id]/+page.svelte`     | `:630`           | Run 改 `brand`                        |
| `apps/web-app/src/routes/workflows/+page.svelte`          | `:264`           | 列表空态 `emptyTone="brand"`          |
| `apps/web-app/src/routes/executions/+page.svelte`         | 空态行           | 空态 `tone="brand"`                   |
| `docs/design-tokens.md`                                   | 品牌行 + 用法    | 补品牌速查，修正旧述                  |

---

## 8. 验证情况

| 检查项   | 命令                                                      | 结果                                                         |
| -------- | --------------------------------------------------------- | ------------------------------------------------------------ |
| 类型检查 | `apps/web-app` 下 `npm run check`                         | 0 errors / 0 warnings                                        |
| 单测     | 同上 `npm run test`                                       | 13 文件 / 146 例全部通过（P3 基线，无新增，P2 无新增纯函数） |
| 静态检查 | 同上 `npm run lint`                                       | 无告警                                                       |
| 格式     | 同上 `npm run format:check` + 相关文件 `prettier --check` | 全部通过（中途修掉两 docs 文件格式）                         |
| 生产构建 | 同上 `npm run build`                                      | 成功；产物 CSS 确认 `--brand` 存在（6 处）                   |

后端未编译：本阶段不改动 Rust 代码，故未执行 `cargo` 相关命令。

---

## 9. 风险与回退

| 风险                     | 影响                 | 处置                                                 |
| ------------------------ | -------------------- | ---------------------------------------------------- |
| 插槽默认值与旧骨架像素差 | 全页布局偏移         | `aside` 缺席分支与旧 DOM 等价；回退即删四 prop       |
| 双详情请求翻倍           | 执行页多一次详情请求 | 仅 Compare 开启且选 B 后触发；关闭即单请求           |
| 品牌色对比度             | 橙底白字约 3.3:1     | 只用于大字 Logo 与主按钮，不用于正文；回退改两处类名 |
| 空状态品牌化过载         | 页面变花             | 仅两处 opt-in；回退删 `tone` 传参                    |

---

## 10. 遗留与后续

1. **沉浸画布**：壳已支持传空 `header`/`sidebar`，画布页（graph/edit Tab）
   的顶栏收起/内联工具条（06 §3.3）待后续页面侧传入即可。
2. **版本真双图**：合并图仍叠在 live 图上；若要 From/To 真双图，
   需新增版本定义转展示图解析（后端定义无前端坐标，`layout` 重排即可），属独立议题。
3. **执行对照增强**：当前为两次完整详情并排；n8n 式的重叠 diff 高亮、
   跨执行节点级 diff，待 `execution-projection` 扩展双份 overlay 输入后跟进。

---

## 11. 附录：参考文档与前后端出处索引

| 主题                     | 出处                                                                     |
| ------------------------ | ------------------------------------------------------------------------ |
| 优先级清单（P2 来源）    | `docs/ref/frontend-viz/06-可借鉴Dify与n8n的设计.md` 第 6 节              |
| 插槽化 / dual / 品牌建议 | 同上 §3.1 / §3.2 / §5.1                                                  |
| 布局维度                 | `docs/ref/frontend-viz/03-页面布局.md`（n8n BaseLayout 插槽、执行分栏）  |
| 风格维度                 | `docs/ref/frontend-viz/05-UI风格.md`（品牌色、状态编码、密度）           |
| 执行维度                 | `docs/ref/frontend-viz/01-执行过程可视化.md`（Execution 实体、卡片色条） |
| 壳唯一调用               | `apps/web-app/src/routes/+layout.svelte:38-40`                           |
| 单检视器现状             | `SplitView.svelte`、四调用方、`executions/+page.svelte:160-265`          |
| 合并图现状               | `WorkflowVersionsPanel.svelte:63`、`services/workflows.ts:201-251`       |
| 令牌源                   | `apps/ui/src/styles/tokens.css`、`apps/web-app/src/app.css:7-43`         |
