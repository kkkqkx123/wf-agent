# 分阶段实施计划

> 本文把 `00-overview.md` §10 的阶段表展开为可执行的任务分解与验收点。
>
> 依赖顺序原则：**先有后端契约，再做前端**；**先能看，再做输入栏**；**输入栏与 CLI 各自独立可验收**。

---

## 阶段总览

| 阶段 | 主题 | 交付物 | 可独立验收 |
|---|---|---|---|
| P1 | 后端 git 只读域 | `/api/v1/git/*` 只读接口 + 动作注册表骨架 | 是（curl 可验） |
| P2 | 前端工作台骨架 | `/git` 页面：侧栏面板组 + 主视图 diff + 焦点上下文 | 是（能看能选） |
| P3 | 键位输入栏 | `GitKeyBar` + 绑定表 + 控件派生 + 词元解析 | 是（单键可触发） |
| P4 | 后端写操作与危险分级 | `POST /git/action` + 动作注册表完整化 | 是（GUI 完成提交闭环） |
| P5 | CLI 终端 | `WS /git/terminal` + `GitTerminal`（PTY） | 是（可跑命令） |
| P6 | 增强 | hunk/行级 staging、stash 全套、rebase 动作、GUI→CLI 回显、撤销栈 | 否（增量） |

P1–P5 每阶段结束都应是一个**可用的中间状态**，不是半成品。

---

## P1：后端 git 只读域

**目标**：前端有真实数据可消费，且契约稳定。

| # | 任务 | 落点 |
|---|---|---|
| 1.1 | 新增仓库根路径配置项（走 `wf-config`），并做仓库内路径校验工具 | `wf-config` + `wf-api/src/git/` |
| 1.2 | 实现 git 命令封装层：argv 数组调用、超时、输出解析（`--porcelain=v2` / `--format`） | `wf-api/src/git/` |
| 1.3 | 实现只读查询：`repo` / `status` / `files` / `diff` / `commits` / `commits/{sha}/files` / `branches` / `stash` | `wf-api/src/git/` |
| 1.4 | 新增 HTTP routes 与 OpenAPI 标注 | `wf-server/src/api/git/` |
| 1.5 | 域路由注册 | `wf-server/src/router.rs`（参照 `router.rs:72-75`） |
| 1.6 | 实现 `/git/actions` 动作注册表（首期返回只读 + 待实现的写动作，标注可用状态） | `wf-api/src/git/` |

**验收点**

- `curl /api/v1/git/status` 返回分支/上游/ahead-behind 与文件三态，字段稳定。
- 路径逃逸被拒：传入仓库外路径返回明确错误，不执行。
- 所有 git 调用均为 argv 形式，全仓检索无 `sh -c` 拼接（04 文档 §4.2 硬性约定）。
- `/api-docs/openapi.json` 能列出 git 域全部路径。
- 前端 `openapi.json` 快照重新生成，`schema.d.ts` 含 git 类型。

**风险**：`status --porcelain=v2` 的输出解析是 P1 最容易出错的地方（rename、untracked 目录、稀疏检出等情形）。建议先针对这些边界写解析单测。

---

## P2：前端工作台骨架

**目标**：能看、能选、能切换面板；此时还没有输入栏，全靠点击。

| # | 任务 | 落点 |
|---|---|---|
| 2.1 | 新增 `/git` 路由与导航项 | `routes/git/+page.svelte`、`config/navigation.ts` |
| 2.2 | 四区布局：`GitWorkbench` / `GitStatusBar` / `GitMainPane` | `components/git/` |
| 2.3 | 侧栏面板组（tab 分组沿用 lazygit 默认）与文件/提交/分支/贮藏列表 | `components/git/` |
| 2.4 | 主视图 diff 渲染（复用 `DiffView`） | `GitDiffPane.svelte` |
| 2.5 | `services/git.ts` 只读函数 + `stores/git.svelte.ts` 状态骨架 | `services/` `stores/` |
| 2.6 | 订阅 WS `repo_changed`，按 scope 局部刷新 | `services/git.ts` |
| 2.7 | 上下文模型：`activePanel` / 各面板独立选中项 | `stores/git.svelte.ts` |

**验收点**

- 页面能显示文件/提交/分支/贮藏四个面板，切换不丢选中项。
- 选中文件后主视图显示其 diff，行类型（add/del/context）着色正确。
- 面板与 tab 状态写入 URL（`panel` / `tab`），刷新后恢复。
- 窄屏下降级为单栏流，无横向溢出。
- 无 git 仓库时显示 `EmptyState` 并引导配置，不白屏。

**注意**：P2 阶段就要把「上下文模型」做对（`activePanel` + 分离选中项），否则 P3 的输入栏无处绑定。

---

## P3：键位输入栏

**目标**：单键输入 + Enter 触发动作；不可见键控件化。

| # | 任务 | 落点 |
|---|---|---|
| 3.1 | 绑定表数据模块（按 context 分节，含 `alias` / `legacyKey` / `nonPrintable` / `controlPriority`） | `bindings/git.ts` |
| 3.2 | 词元解析纯函数（别名精确 → 全局前缀搜索 → `?` 全量），**不依赖 Svelte** | `lib/git/resolve.ts` |
| 3.3 | `GitKeyBar`：输入 + 候选列表 + 输入即预览 | `components/git/` |
| 3.4 | 控件条自动派生（按 `nonPrintable` + `controlPriority` 取前 4，`<enter>` 为主按钮） | `GitKeyBar.svelte` |
| 3.5 | 焦点仲裁统一入口（输入栏空则透传列表导航） | `GitWorkbench.svelte` |
| 3.6 | 右键动作菜单与 `?` 帮助面板，共用绑定表 | `GitActionMenu` / `GitHelpPanel` |
| 3.7 | 启动期校验：绑定表 `actionId` vs `/git/actions` | `services/git.ts` |
| 3.8 | 绑定表单测（唯一性、别名冲突、解析分支） | `*.test.ts` |

**验收点**

- 文件面板敲空格 + Enter 能切换暂存（此时后端尚未就绪，可用 mock 或等 P4；验收以「正确解析并发起调用」为准）。
- 敲 `d` 时预览显示完整动作名与危险色，不只是 `d`。
- 输入栏为空时方向键能导航列表；非空时 Enter 提交输入栏。
- 控件条在没有 `nonPrintable` 项的上下文下自动收起。
- 右键菜单列出当前上下文全部动作，含 `legacyKey` 副标题。

**注意**：P3 可以先用只读动作验收（刷新、复制、聚焦主视图），写动作等 P4 后端就绪后打通。

---

## P4：后端写操作与危险分级

**目标**：GUI 能完成完整提交闭环，且危险操作有分级保护。

| # | 任务 | 落点 |
|---|---|---|
| 4.1 | 动作注册表完整化：注册全部首期写动作 + `danger` 级别 + `params` 声明 | `wf-api/src/git/` |
| 4.2 | `POST /git/action` 统一入口与 handler 分发 | `wf-api/src/git/`、`wf-server/src/api/git/` |
| 4.3 | 动作前置校验（选中项类型、仓库中间态），返回带原因的错误 | 同 4.2 |
| 4.4 | `destructive` 级要求客户端传确认标识，后端独立校验 | 同 4.2 |
| 4.5 | 写操作串行锁 + 超时分级（常规 15s / push·fetch 120s） | 同 4.2 |
| 4.6 | 前端 `GitActionDialog`：参数输入（`prompt` / `choice` / `ref` 三种 source）与危险确认 | `components/git/` |
| 4.7 | `soft` 级 toast 撤销（`undoToken`）+ 后端 `undo` / `redo` 动作（基于 reflog） | `services/git.ts` + 后端 |
| 4.8 | 状态指纹与 `repo_changed` 推送 | `wf-api/src/git/` |

**验收点**

- 完整闭环：暂存 → 提交（填信息）→ 推送 → 面板自动刷新。
- 放弃文件变更弹确认框并说明将丢失什么。
- 强制推送要求输入分支名才启用按钮；**绕过前端直接调 API 且不带确认标识时被后端拒绝**。
- 提交后 8 秒内 toast 有撤销入口，撤销后文件回到已暂存状态。
- 并发触发两个写操作不报 `index.lock` 冲突。

**重点验收 4.4**：这是「危险级别是后端属性而非前端装饰」的唯一验证手段，必须实测绕过 UI 调用。

---

## P5：CLI 终端

**目标**：页面内可跑任意 git 命令，包括交互式命令；且执行后 GUI 自动刷新。

| # | 任务 | 落点 |
|---|---|---|
| 5.1 | 新增 `WS /api/v1/git/terminal` 端点（独立端点，不复用事件订阅 WS） | `wf-server/src/api/git/terminal.rs` |
| 5.2 | 会话管理：`get_or_create`（PTY）+ 常驻 shell 启动 + `send_input` / `resize` / `kill` / 空闲回收 | `wf-api/src/git/` + `wf-shell` |
| 5.3 | 会话级安全：cwd 锁仓库根、路径逃逸拒绝、env 注入（`GIT_TERMINAL_PROMPT=0`、`GIT_PAGER=cat`）、拒绝清单 | 同 5.2 |
| 5.4 | 引入 `@xterm/xterm` + `@xterm/addon-fit`，实现 `GitTerminal` | `components/git/` |
| 5.5 | 协议帧实现：`start` / `input` / `resize` / `kill` / `ping` 与 `ready` / `output` / `exit` / `repo_changed` / `error` | 前后端 |
| 5.6 | 状态指纹比对触发 `repo_changed`（03 文档 §5.2） | 后端 |
| 5.7 | 连接生命周期：折叠不销毁、离开路由销毁、`beforeunload` 主动 kill | `GitTerminal.svelte` |

**验收点（含一个必须实测的未知项）**

- 能执行 `git status`、`git log --oneline`，输出正确渲染（含 ANSI 着色）。
- **能执行 `git add -p` 并完成一次交互式暂存**——这是选用 PTY 方案的核心理由，必须实测通过。
- 终端里执行 `git checkout <branch>` 后，上半部分面板自动刷新（不需要手动点刷新）。
- 终端里执行非 git 命令（如 `rm` 一个受版本控制的文件）后，文件面板同样刷新。
- 折叠/展开终端不丢失 cwd 与历史；离开 `/git` 后会话被回收。
- `git fetch` 需要凭据时**不卡死**（验证 `GIT_TERMINAL_PROMPT=0` 生效）。

**P5 的实测未知项**：`wf-shell` 的 `send_input` 要求会话中存在 `running` 状态的当前命令（`store.rs:388-399`），因此需先 `execute_in_session(sid, "<shell>", None)` 起常驻 shell。这一行为在 P5 开工时先用最小程序验证，若不可行则回退到「每条命令一个 PTY 会话 + 命令结束后重启」的变体，或降级为批处理方案（03 文档 §2.3）。**这个验证应放在 P5 的第一天，不要等到最后。**

---

## P6：增强（增量，不阻塞前序）

| # | 任务 | 说明 |
|---|---|---|
| 6.1 | hunk / 行级 staging（`Staging` 主视图模式） | 对应 lazygit 的 Staging context；工作量大，独立评估 |
| 6.2 | stash 全套动作（变体选择、重命名、建分支） | |
| 6.3 | rebase 动作集（reword / squash / fixup / drop / move / 变基到分支） | 交互式变基靠后端命令构造，不用 `rebase -i` 的编辑器 |
| 6.4 | 拖拽重排提交（对应 `<ctrl+j>` / `<ctrl+k>`） | |
| 6.5 | GUI → CLI 命令回显（03 文档 §5.3） | 教学价值高，成本低 |
| 6.6 | 自定义补丁构建（PatchBuilding） | lazygit 的高级特性，优先级最低 |
| 6.7 | 合并冲突解决视图 | 对应 MergeConflicts context |
| 6.8 | 用户键位自定义（覆盖值存 `web` 域偏好） | 依赖 Q1 的拍板结论 |
| 6.9 | worktrees / submodules 的读与写 | 首期隐藏的 tab 在此激活 |

---

## 跨阶段约束

| 约束 | 说明 |
|---|---|
| 文档同步 | 代码落地后须同步更新本目录对应文档，而非仅标记过期（项目既有约定） |
| 契约先行 | 每阶段开工前先更新 `apps/web-app/openapi.json` 快照并重生成 `schema.d.ts` |
| 无新增基础组件 | 所有 UI 元素从 `components/ui/` 取（05 文档 §1） |
| 唯一写入口 | 前端只有 `runAction` 一个写函数，后端只有 `POST /git/action` 一个写路由（04 文档 §3.3） |
| 测试 | 词元解析与 porcelain 解析为纯函数单测；终端行为以手工验证为主（05 文档 §10） |

---

## 未纳入本计划的项

- 多仓库与 recent-repos 切换（依赖 Q11）。
- Git 托管平台集成（PR 创建/打开/复制 URL）——依赖外部服务，超出本方案。
- 多人协作与仓库级锁（依赖 Q8）。
