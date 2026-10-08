# 前端架构设计

> 本文回答：`/git` 页面的路由、组件、状态与服务层怎么组织，以及哪些能复用现有资产。
>
> 上游依据：`00-overview.md` §9、`02-keyinput-bar-design.md`、`03-cli-terminal-design.md`；现状基线见各节引述。

---

## 1. 现有可复用资产

| 资产 | 位置 | 本方案用法 |
|---|---|---|
| `AppShell` / `Sidebar` / `TopBar` | `src/lib/components/layout/AppShell.svelte` 等 | 页面外壳，已由 `+layout.svelte` 挂载（`src/routes/+layout.svelte:29-31`） |
| `DiffView` | `src/lib/components/ui/DiffView.svelte:1-48` | **直接复用**渲染 diff 行（支持 add/del/context/meta 四类行 + 标题） |
| `SplitView` | `src/lib/components/layout/SplitView.svelte` | 侧栏 / 主视图 / CLI 三区分割 |
| `DataTable` / `FilterBar` / `CursorPager` | `src/lib/components/ui/` | 提交列表、分支列表的表格与分页 |
| `Dialog` / `Input` / `Select` / `Textarea` | `src/lib/components/ui/` | 参数 Dialog（02 文档 §5）与危险确认（§6） |
| `Segmented` | `src/lib/components/ui/Segmented.svelte` | 主视图三态切换（对应 lazygit 的 `<tab>`） |
| `Toaster` | `src/lib/components/layout/Toaster.svelte` | 结果提示 + `soft` 级撤销入口 |
| `ContextMenu` / `DropdownMenu` | `src/lib/components/ui/` | 右键动作菜单（02 文档 §10） |
| `EmptyState` / `ErrorState` / `Skeleton` | `src/lib/components/ui/` | 空仓库、加载、失败态 |
| `CommandPalette` | `src/lib/components/layout/CommandPalette.svelte` | **不合并**，仅共享候选列表渲染（02 文档 §8） |
| API 客户端与信封 | `src/lib/api/client.ts:30`、`api/envelope.ts:48,77,88` | `call()` / `requireData()` / `extractPage()` 沿用 |
| 导航配置 | `src/lib/config/navigation.ts:19` | `NAV_GROUPS` 新增一项 |
| URL 参数工具 | `src/lib/utils/route.ts:18-22` | `panel` / `tab` 已是既有 LIST_KEYS，可直接用于面板持久化 |

**需要新增的运行时依赖只有一个**：`@xterm/xterm`（+ `@xterm/addon-fit`），用于 CLI 终端（03 文档 §4.3）。

---

## 2. 路由与导航

| 项 | 设计 |
|---|---|
| 路由 | `src/routes/git/+page.svelte`（一级页面，与 `workflows` / `chat` / `executions` 同级） |
| 导航 | 在 `NAV_GROUPS` 中新增一组（建议 `id: 'source'`，`label: 'Source'`），`href: '/git'`，图标从 `IconName` 中挑选现有项 |
| URL 状态 | 复用 `LIST_KEYS` 中的 `panel`（当前侧栏面板）与 `tab`（面板内 tab），通过 `gotoWithParams` 写入；刷新页面能恢复所在面板 |
| 子路由 | 首期**不做**子路由（提交详情、分支详情等一律用主视图/inspector 承载，不占路由）。与 `workflows/[id]` 那种详情路由区分开：Git 工作台是单页密集操作界面，路由跳转会打断输入栏焦点 |

`href` 必须声明为 `AppPath`（`route.ts:8`）而非 `string`，这是既有约束（`docs/plan/web/web-app-ui-implementation-design.md` §1.1）。

---

## 3. 页面布局

四区（自上而下）：

| 区 | 组件 | 内容 | 可折叠 |
|---|---|---|---|
| 顶栏区 | `GitStatusBar` | 仓库名、当前分支、上游、ahead/behind、中间态徽标（变基中/合并中/二分中） | 否 |
| 主体区 | 左 `GitSidePanel` / 右 `GitMainPane` | 侧栏面板组 / 主视图 | 侧栏可折叠 |
| 输入栏 | `GitKeyBar` | **常驻**，见 02 文档 | 否 |
| CLI 区 | `GitTerminal` | 可折叠，默认收起 | 是 |

侧栏面板组沿用 lazygit 默认分组（`lazygit/pkg/config/user_config.go:882-888`）：

```
[status]
[files, worktrees, submodules]
[branches, remotes, tags]
[commits, reflog]
[stash]
```

首期只激活 `status` / `files` / `branches` / `commits` / `stash` 五组中的四组（worktrees 与 submodules 的写操作不做，tab 可先隐藏）。

主视图三态（对应 lazygit 的 `<tab>` 切换）：`Diff`（默认）/ `Staging`（块行暂存）/ `CommitFiles`。用 `Segmented` 承载。

---

## 4. 新增组件清单

统一放 `src/lib/components/git/`，与现有 `components/domain/`、`components/chat/` 的组织惯例一致。

| 组件 | 职责 |
|---|---|
| `GitWorkbench.svelte` | 页面容器：四区布局、键盘仲裁总入口、仓库状态订阅 |
| `GitStatusBar.svelte` | 顶栏：分支/上游/中间态/刷新按钮 |
| `GitSidePanel.svelte` | 侧栏壳：面板组 tab + 当前面板的过滤框 |
| `GitFileList.svelte` | 文件面板：扁平/树形切换、三态标记、多选 |
| `GitCommitList.svelte` | 提交面板：图/列表、拣选标记、拖拽重排 |
| `GitBranchList.svelte` | 分支面板（本地/远程/标签共用一套列表，按 tab 切换数据源） |
| `GitStashList.svelte` | 贮藏面板 |
| `GitMainPane.svelte` | 主视图壳：三态切换 + 工具条 |
| `GitDiffPane.svelte` | 复用 `DiffView` 渲染 diff，附加 hunk 导航 |
| `GitKeyBar.svelte` | **核心**：输入栏 + 候选列表 + 控件条（02 文档） |
| `GitActionMenu.svelte` | 右键/更多菜单：按上下文渲染全部动作 |
| `GitActionDialog.svelte` | 参数输入 + 危险确认（02 文档 §5、§6） |
| `GitTerminal.svelte` | CLI 终端（03 文档 §4.3） |
| `GitHelpPanel.svelte` | `?` 唤出的当前上下文动作全量表 |

**不新增基础组件**。所有交互元素都从 `components/ui/` 取，与既有页面保持视觉一致。

---

## 5. 状态层：`stores/git.svelte.ts`

沿用现有 store 惯例：class + `$state` 字段 + 方法（`src/lib/stores/ui.svelte.ts:19-30` 的写法）。

状态分四组：

| 组 | 字段 | 说明 |
|---|---|---|
| 仓库快照 | `repo`、`files`、`commits`、`branches`、`stash`、`revision` | `revision` 为状态指纹，用于判断是否需刷新 |
| 上下文 | `activePanel`、`activeTab`、`mainMode`、`selection`（各面板独立选中项） | `activePanel` 决定输入栏作用域 |
| 输入栏 | `token`、`candidates`、`highlightedIndex`、`barFocused` | 02 文档 §3 的解析状态 |
| 执行 | `pendingAction`、`running`、`lastError`、`undoToken` | `undoToken` 支撑 `soft` 级撤销（02 文档 §6） |

关键约定：

- **选中项按面板分离保存**：切换面板再切回来时选中项不丢（lazygit 也是这个行为，各 context 独立维护选中）。
- `revision` 变化时按 scope 局部刷新，不整页重载——`status`/`files` 一个 scope，`commits`/`branches` 一个 scope。
- 输入栏状态与业务状态分开，避免输入时触发大列表重渲染。

---

## 6. 服务层：`services/git.ts`

沿用现有 service 惯例（参考 `src/lib/services/workflows.ts:1-20`）：从 `client` 取、`call()` 拆信封、`requireData()` 兜底、DTO 转领域模型。

职责划分：

| 函数族 | 用途 |
|---|---|
| `getRepo()` / `getStatus()` / `getFiles()` / `getDiff()` / `getCommits()` / `getBranches()` / `getStash()` | 只读拉取 |
| `runAction(actionId, params, selection)` | **单一写操作入口**，对应 `POST /git/action`（04 文档 §3.3） |
| `getActions()` | 拉取动作注册表，供启动期校验（02 文档 §2.3） |
| `connectRepoEvents()` | 订阅 WS 的 `repo_changed`，推 `revision` 更新（03 文档 §5） |
| `connectTerminal()` | CLI 的 WS 连接管理（03 文档 §4.2） |

**只有一个写操作函数**，这是后端统一 action 入口在前端的自然映射。新增动作不需要动 service 层。

DTO 转换：后端返回的是 git 原始字段（`XY` 状态码、porcelain 格式等），在 service 层转成领域模型（`staged` / `unstaged` / `conflicted` 三态布尔），组件层不接触 git 内部表示。

---

## 7. 绑定表：`bindings/git.ts`

一份纯数据模块，是输入栏、控件条、右键菜单的唯一事实源（02 文档 §2.2）。

组织方式建议：

- 按 context 分节导出（`filesBindings` / `commitsBindings` / `branchesBindings` / `stashBindings` / `globalBindings`）。
- 每项含 `id` / `label` / `alias` / `legacyKey` / `nonPrintable` / `controlPriority` / `danger` / `params` / `availability`。
- `danger` 与 `params` **不在前端硬编码**——从 `/git/actions` 拉取后覆盖（后端是权威）。前端表里的同名字段只作为离线兜底与类型提示。

这样既保证键位归属前端（UI 关注点），又保证危险级别归属后端（执行关注点），靠启动校验防漂移。

---

## 8. 键盘仲裁的实现位置

02 文档 §4 的焦点仲裁规则需要一个统一入口，建议放在 `GitWorkbench.svelte`：

- 在容器根节点挂 `keydown` 监听（捕获阶段），根据「输入栏是否为空」决定事件去向。
- 输入栏本身只处理自己的 `keydown`，不拦截冒泡上来的列表导航。
- 列表组件的原生键盘导航保持不变，不感知输入栏存在。

避免在多个组件里各挂一套 keydown——那会随组件增加而变成无法推理的优先级问题。

---

## 9. 响应式与降级

| 视口 | 形态 |
|---|---|
| `wide`（≥1280） | 完整四区；侧栏常驻，CLI 折叠时仅留标题条 |
| `compact` / `tablet`（768–1279） | 侧栏可折叠为抽屉；输入栏与控件条仍常驻 |
| `mobile`（<768） | **降级为「列表 + 详情」单栏流**：底部输入栏变动作搜索框，控件条横向滚动；CLI 默认隐藏（触屏无 PTY 价值） |

视口判定复用 `ui.viewport`（`src/lib/stores/ui.svelte.ts:28-30`），不另写断点逻辑。

移动端不做单键输入优化——触屏上单键没有效率优势，此形态下以候选列表点击为主（02 文档 §9）。

---

## 10. 测试策略

| 层 | 做法 |
|---|---|
| 绑定表 | 单测：每个 `actionId` 唯一；同 context 内 `alias` 不冲突；`nonPrintable` 项都有 `controlPriority` |
| 词元解析 | 单测（纯函数最佳）：单字符别名精确命中、多字符前缀搜索、空输入、无匹配、`?` 全量列出 |
| 焦点仲裁 | 组件测：输入栏空/非空两种状态下 `keydown` 的去向 |
| service | 单测：DTO 转换（porcelain 状态码 → 三态）、`runAction` 的错误分支 |
| 终端 | 连接生命周期测（折叠不销毁、离开路由销毁）；PTY 行为以手工验证为主 |

词元解析做成**不依赖 Svelte 的纯函数**，是这套设计能否被可靠测试的关键——它承载了全部交互语义，却不该绑在组件上。

---

## 11. 待拍板项

| # | 问题 | 建议默认值 | 影响 |
|---|---|---|---|
| Q16 | Git 页面放哪个导航组 | 新增 `Source` 组 | 影响信息架构；也可并入既有 `Execution` 组 |
| Q17 | 是否用 URL 持久化面板与选中 | 用 `panel` / `tab` 两个 key | 影响刷新后是否回到原面板 |
| Q18 | 主视图是否做子路由 | 首期不做 | 影响输入栏焦点是否被导航打断 |
| Q19 | 移动端是否保留 CLI | 隐藏 | 触屏 PTY 价值低，且占用大量屏幅 |

完整清单见 `07-open-questions.md`。
