# Web 端 Git 工作台设计方案（lazygit 交互范式的 Web 化）

> 目标：在 `apps/web-app`（SvelteKit 2 + Svelte 5）中为 wf-agent 增加一个**类 lazygit 的 Git 管理界面**，把 lazygit 的「模态单键」交互替换为**专用键位输入栏**（单键输入、Enter 提交），无法以字符输入表达的控制键（`<esc>` / `<enter>` / `<ctrl+x>` 等）改为**点击控件**，并额外提供一个**命令行界面（CLI）**。
>
> 分析基线：`lazygit` @ `a3fae72578e472b11b9763c7d5536c6853f31da4`（克隆于 `/workspace/lazygit`）；`wf-agent` @ `0a9230a139c9ddb8b5fb684de229cb1da016cb51`（`main`）。
>
> 上游依据：`docs/apps/web-app/architecture.md`、`docs/plan/web/web-app-ui-implementation-design.md`、`docs/plan/web-app-integration.md`、`AGENTS.md`（分层与文档约定）。

---

## 0. 文档定位与范围

本目录回答一个问题：**lazygit 那套键盘优先的 Git TUI，搬到浏览器里该怎么重新设计，才既保留效率又不别扭。**

范围内：

- lazygit 交互模型的拆解与可迁移性量化（哪些键位能直接搬、哪些必须重做）。
- 键位输入栏的语义模型、寻址规则、反馈与控件化策略。
- 命令行界面（CLI）的形态、链路与和 GUI 的联动。
- 后端新增 `git` 域的接口划分与执行策略。
- 前端组件/状态/路由结构与分阶段落地计划。

范围外（不在本目录展开）：

- wf-agent 自身的 checkpoint 文件版本系统（`crates/infra/checkpoint/checkpoint-file/`）——它与 Git 是两套东西，本方案**不**复用其分支/历史模型。
- 多用户、鉴权与权限模型（沿用 `crates/app/wf-server/src/middleware.rs` 现有配置）。

### 文档索引

| 文件 | 内容 |
|---|---|
| `00-overview.md` | 本文件。总览、核心决策摘要、全局章节 0~11 |
| `01-lazygit-interaction-model.md` | lazygit 交互模型拆解 + 294 条键位的三分类量化 |
| `02-keyinput-bar-design.md` | **核心**：键位输入栏的语义模型、寻址、控件化、危险操作 |
| `03-cli-terminal-design.md` | CLI 形态选型、PTY 链路、与 GUI 的状态联动 |
| `04-backend-git-domain.md` | 后端 `git` 域：接口清单、执行策略、安全边界 |
| `05-frontend-architecture.md` | 前端路由/组件/store/service 结构 |
| `06-implementation-plan.md` | 分阶段实施计划与验收点 |
| `07-open-questions.md` | 待拍板开放点清单 |

---

## 1. 现状事实核查

### 1.1 lazygit 侧

| 事实 | 依据 |
|---|---|
| 面板是「Context 树」，共 30 个 context 节点（含 6 个纯显示 context） | `lazygit/pkg/gui/context/setup.go:5-137` |
| 侧栏 tab 共 10 种合法取值：status / files / worktrees / submodules / branches / remotes / tags / commits / reflog / stash | `lazygit/pkg/config/side_panel.go:17-30` |
| 默认布局为 5 个侧栏面板组：`[status]`、`[files, worktrees, submodules]`、`[branches, remotes, tags]`、`[commits, reflog]`、`[stash]`，侧栏宽度 1/3 | `lazygit/pkg/config/user_config.go:882-888`、`user_config.go:879` |
| 键位配置分 12 个组（universal / status / files / branches / commits / amendAttribute / stash / commitFiles / main / submodules / commitMessage） | `lazygit/pkg/config/user_config.go:460-472` |
| 单个键位值类型是 `[]string`（支持别名，如 `[<pgup>, K, <ctrl+u>]`），`<disabled>` 与空串被自动过滤 | `lazygit/pkg/config/keybinding.go:18`、`keybinding.go:36-42` |
| 绑定 = 全局绑定（`ViewName=""`）+ 每个 context 自带绑定；冲突时自定义命令前置覆盖默认 | `lazygit/pkg/gui/keybindings.go:81-296`、`keybindings.go:298-309`、`keybindings.go:349-351` |
| 绑定执行前有 disabled 检查，可返回「不处理 / 面板内报错 / toast 提示」三种结果 | `lazygit/pkg/gui/keybindings.go:393-412` |
| 中文键位速查表共 **294 条**绑定，覆盖 25 个面板段 | `lazygit/docs/keybindings/Keybindings_zh-CN.md`（417 行，自动生成） |
| 未处于过滤模式时，默认焦点 context 是 Files | `lazygit/pkg/gui/context_config.go:16-22` |
| Git 操作通过 shell 调用 `git` 二进制实现，而非 libgit2/git2 | `lazygit/pkg/commands/`（整体结构） |

### 1.2 wf-agent 侧

| 事实 | 依据 |
|---|---|
| 前端为 SvelteKit 2.69 + Svelte 5.56 + TS 5.9 + Tailwind 4.1 + Vite 8，测试用 vitest 5，适配器 `adapter-static` | `apps/web-app/package.json:20-52`、`apps/package.json:8`（devDep `@sveltejs/adapter-static`） |
| 前端已用 `openapi-fetch` + 生成的 `schema.d.ts` 做类型化调用，运行时依赖仅 4 个（cytoscape / dagre / markstream-svelte / tailwindcss） | `apps/web-app/package.json:53-59` |
| 后端路由按域 merge，域前缀挂在 `/api/v1` 下；新增域只需在 `router.rs` 加一行 `.merge(...)` | `crates/app/wf-server/src/router.rs:44-104` |
| 已存在 WebSocket 端点 `GET /api/v1/ws`，但协议是**事件订阅模型**（subscribe/unsubscribe/ping），不是通用双向 RPC | `crates/app/wf-server/src/ws.rs:1-30`、`ws.rs:65-70` |
| SSE 已用于执行流/LLM 流等 5 处 | `crates/app/wf-server/src/sse.rs:2`；`api/agent/loops.rs:467` 等 |
| `wf-shell` **已编译进 PTY 支持**（`portable-pty`，非 feature-gated），由 `pty_enabled` + `SessionCreateOptions::interactive` 决定是否走 PTY，否则 fallback 到 pipe | `crates/infra/wf-shell/src/lib.rs:18-30` |
| `wf-shell` 的会话模型是「terminal session 常驻、每条命令一个独立 `ShellSession`，共享 cwd/env 与输出 buffer」 | `crates/infra/wf-shell/src/terminal_session.rs:1-8`、`session.rs:1-6` |
| 已有终端会话的高级封装：交互式脚本会话驱动（含 prompt 检测、轮次限制、workspace capture） | `crates/engine/wf-execution-shared/src/interactive_script_session/driver.rs:89,290` |
| 已有命令安全策略（白名单/前缀匹配/拒绝规则） | `crates/infra/wf-shell/src/command_safety.rs:266`；`crates/infra/wf-sandbox/src/command_policy.rs:962-1007` |
| **后端目前完全没有 git 域**：全仓 `git2` / `gix` 零引用，`git` 只出现在 shell 白名单与 demo 中 | 全仓检索（见 04 文档 1.1 节） |
| 已有 `web` 域承载 UI 偏好与收藏夹（快照式持久化），可作为键位自定义配置的落点 | `crates/app/wf-api/src/web.rs:1-8`；`router.rs:72-75` |
| 前端已有可复用组件：`DiffView`（diff 行渲染）、`SplitView`、`DataTable`、`Dialog`、`Segmented`、`Toaster`、`CommandPalette` | `apps/web-app/src/lib/components/ui/`、`components/layout/` 目录清点 |
| 前端无终端组件、无 xterm.js 依赖 | `apps/web-app/package.json` 依赖清点 |

### 1.3 关键结论

1. **输入栏方案在数量上是可行的**：294 条绑定中 73% 是可打印单字符，可直接「输入 + Enter」；真正必须控件化的只有约 23%（详见 01 文档）。
2. **后端缺一整块**：git 域从零开始，但基础设施（域路由注册、PTY、命令安全、WS、偏好持久化）全部现成，是「填一个域」而非「搭一套设施」。
3. **PTY 已就绪**：`wf-shell` 提供 `send_input` / `resize` / `kill` 与 PTY 后端，CLI 不需要引入新依赖，也不需要新 crate。

---

## 2. 设计目标与非目标

### 2.1 目标

- **保留 lazygit 的效率手感**：熟练用户单手输入单字符 + Enter 完成高频动作，不比 TUI 慢。
- **消除模态记忆负担**：输入栏显式显示当前上下文，边输入边提示，不需要背键位表。
- **不可见键不丢失**：`<esc>` / `<enter>` / `<ctrl+x>` 全部有等价点击控件，鼠标用户零键位知识也能完成全部操作。
- **CLI 兜底**：任何 GUI 没暴露的 git 能力，都能在页面内终端里完成，不用来回切窗口。
- **危险操作可控**：破坏性动作（force push / hard reset / 丢弃变更）分级确认，可撤销的走「toast 撤销」。

### 2.2 非目标

- 不做 lazygit 的 1:1 移植：不追求 294 条绑定全量搬过来，首期只覆盖高频子集（见 06 文档阶段划分）。
- 不做交互式变基的 TUI 编辑器（`git rebase -i`）：Web 上以拖拽重排 + 动作菜单替代，交互式命令留给 CLI。
- 不做多仓库工作区（lazygit 的 recent repos 切换）：wf-agent 单工作区即可，仓库路径由服务端配置指定。
- 不做 Git 托管平台集成（PR 创建、浏览器打开 PR）：首期不做，键位表中对应项标记为不可用。

---

## 3. 核心决策摘要

| 决策点 | 结论 | 理由 |
|---|---|---|
| Git 执行方式 | shell 调用 `git` 二进制，不用 git2/gix | 与 lazygit 一致；SSH/credential helper 场景 git2 覆盖不全；可直接复用 `wf-shell` 的命令安全策略 |
| 动作寻址 | 统一 `actionId` 寻址，**单字符是 actionId 的别名** | 让 C 类键位（`<ctrl+o>` 等）也能通过动词名触达，同时保留 lazygit 手感；一份绑定表同时驱动输入栏、控件、右键菜单 |
| 输入栏作用域 | 绑定「当前焦点面板（active context）」，全局动作任意上下文可用 | 对齐 lazygit 的 context 模型；消解「同一个 `d` 在不同面板含义不同」的歧义 |
| 不可见键 | 从绑定表自动派生控件条，而非手工在 UI 上摆按钮 | 键位表是唯一事实源，控件与输入栏永不脱节 |
| CLI 形态 | **PTY 常驻 shell 真交互**（xterm.js + 独立 WS 端点） | `wf-shell` 已提供 `send_input` / `resize` / PTY，无需新依赖；且 `git add -p`、`rebase -i` 等交互式命令只有 PTY 能承载 |
| 动作执行方式 | GUI 动作走**批处理**（单命令 spawn），CLI 走 **PTY 会话** | 按入口分流：GUI 动作需危险分级/白名单/可审计，CLI 需自由交互（见 03 文档 §2.2） |
| 状态刷新 | 动作执行后服务端推送 `repo.changed` 事件（复用现有 WS），前端按 scope 局部刷新 | 避免全量轮询；CLI 里的写操作也能驱动 GUI 刷新 |
| 键位自定义 | 绑定表后端可下发，用户覆盖值存在 `web` 域偏好里 | 沿用已有持久化通道，不新增存储实体 |

---

## 4. 交互形态总览

页面分四区（上→下，左→右）：

```
┌──────────────────────────────────────────────────────────┐
│ 顶栏：仓库 / 分支 / 上游 / ahead-behind                     │
├───────────────┬──────────────────────────────────────────┤
│ 侧栏面板组     │  主视图（Diff / 提交文件 / Staging）        │
│ status        │                                          │
│ files · wt ·  │                                          │
│ submodules    │                                          │
│ branches ·    │                                          │
│ remotes · tags│                                          │
│ commits ·     │                                          │
│ reflog        │                                          │
│ stash         │                                          │
├───────────────┴──────────────────────────────────────────┤
│ [键位输入栏]  Files ▸ c 提交变更        [打开][返回][复制]   │  ← 核心
├──────────────────────────────────────────────────────────┤
│ CLI 终端（可折叠）                                          │
└──────────────────────────────────────────────────────────┘
```

侧栏分组直接沿用 lazygit 默认布局（`user_config.go:882-888`），因为这套分组已被大量用户验证过：`files+worktrees+submodules` 是「本地工作区」，`branches+remotes+tags` 是「引用」，`commits+reflog` 是「历史」。

键位输入栏**常驻可见**（不做成按 `/` 唤出），因为它是本设计替代快捷键的核心载体，藏起来就等于退回纯鼠标操作。

---

## 5. 键位三分类（决定哪些能直搬）

对 `Keybindings_zh-CN.md` 的 294 条绑定按「Web 可表达性」分类（脚本解析，取每条绑定的**主键**）：

| 类别 | 定义 | 条数 | 占比 | 处理策略 |
|---|---|---|---|---|
| **A 可打印单字符** | 字母/数字/符号/空格，能直接打进输入框 | 215 | 73.1% | 输入栏直接支持（输入 + Enter） |
| **B 浏览器原生键** | 方向键、`<pgup>/<pgdown>`、`<home>/<end>`、鼠标滚轮 | 12 | 4.1% | 保留原生键位，不进输入栏；鼠标/滚动天然等价 |
| **C 必须控件化** | `<esc>` / `<enter>` / `<tab>` / `<ctrl+x>` / `<alt+x>` / `<shift+方向>` | 67 | 22.8% | 渲染为点击控件，并提供动词名别名 |

C 类 67 条按动作去重后约 **35 个不同动作**，且其中「复制 X 到剪贴板」（`<ctrl+o>`）在 6 个面板重复出现——在 Web 上统一收敛为一个行内复制按钮即可。真正需要单独设计的 C 类控件约 12 个。

> 完整分类明细与每面板的 C 类清单见 `01-lazygit-interaction-model.md`。

---

## 6. 键位输入栏（一句话版）

输入栏是**「上下文 + 词元」→ 动作**的解析器：

- 输入 `c` → 精确命中当前面板的别名 → 栏内显示 `c · 提交变更` → Enter 执行。
- 输入 `cop` → 无别名命中 → 退化为动作名前缀搜索 → 列出候选 → Enter 执行首项。
- 输入栏右侧常驻 3~5 个**不可见键控件**（由绑定表的 C 类自动派生），当前面板没有 C 类绑定时该区自动收起。

详细语义、数据结构、反馈时序、危险操作分级见 `02-keyinput-bar-design.md`。

---

## 7. CLI（一句话版）

页面底部常驻一个可折叠终端（xterm.js），背后是一个 **PTY 常驻 shell 会话**，`wf-shell` 已具备全部后端原语（`send_input` / `resize` / `kill` / PTY）。它既能跑普通 git 命令，也能承载 `git add -p`、`git rebase -i` 这类交互式命令。

GUI 动作则相反，走**批处理单命令 spawn**——动作集合封闭，需要危险分级、白名单与审计。两条链路按入口分流，不是二选一（见 03 文档 §2.2）。

CLI 与 GUI 共享同一份仓库状态：CLI 里执行了写操作，服务端推 `repo.changed`，上半部分面板自动刷新。

详细链路、协议、安全边界见 `03-cli-terminal-design.md`。

---

## 8. 后端形态（一句话版）

新增一个 `git` 域：

- `crates/app/wf-api/src/git.rs` + `git/` 子模块（领域逻辑与 git 命令封装）
- `crates/app/wf-server/src/api/git/*.rs`（REST routes，前缀 `/api/v1/git/*`）
- 所有写操作走统一入口 `POST /api/v1/git/action`，用 `actionId` 分发——这样前端新增动作不需要新增路由
- `WS /api/v1/git/terminal`：CLI 终端通道（独立于现有事件订阅 WS）

接口清单、执行策略、安全边界见 `04-backend-git-domain.md`。

---

## 9. 前端形态（一句话版）

新一级路由 `/git`，加入 `NAV_GROUPS`；新增 `components/git/*` 组件簇、`stores/git.svelte.ts`、`services/git.ts`；复用现有 `DiffView` / `SplitView` / `DataTable` / `Dialog` / `Segmented` / `Toaster`。

组件/状态/路由详细结构见 `05-frontend-architecture.md`。

---

## 10. 实施阶段（一句话版）

| 阶段 | 内容 | 可独立验收 |
|---|---|---|
| P1 | 后端 git 只读域（status / files / diff / commits / branches） | 有 API，前端先用 JSON 页面验证 |
| P2 | 前端工作台骨架：侧栏面板组 + 主视图 diff + 焦点上下文 | 能看、能选、能切换 |
| P3 | 键位输入栏 + 绑定表 + 控件派生 | 能用单键完成 stage/commit/discard |
| P4 | 后端写操作 + 统一 action 入口 + 危险分级 | GUI 能完成完整提交流程 |
| P5 | CLI 终端（批处理模式） | 能在页面内跑 git 命令并驱动 GUI 刷新 |
| P6 | 增强：hunk/行级 staging、stash、rebase 动作、可选 PTY 交互 | 进阶能力 |

详细任务分解与验收点见 `06-implementation-plan.md`。

---

## 11. 风险与开放问题

三个最需要先拍板的点：

1. **CLI 的会话模型验证**——`send_input` 要求会话中存在 `running` 状态的当前命令，因此需先起一个不设超时的常驻 shell 再喂输入；这一行为要在 P5 阶段实测确认（见 03 文档 §3、06 文档 P5 验收点）。
2. **键位表放在前端还是后端下发**——影响自定义键位的实现成本与前后端同步责任（见 02 文档 §6）。
3. **仓库作用域**——服务端固定单仓库，还是支持前端指定路径/多仓库切换（lazygit 有 recent repos，本方案首期不做，需确认）。

完整开放点清单（含影响面与建议默认值）见 `07-open-questions.md`。

---

## 附：事实核验方法

本目录所有 lazygit 行号引用均基于 `a3fae72`，可用以下命令复现：

```shell
cd /workspace/lazygit && git log -1 --format=%H
```

294 条键位的分类统计可用解析 `docs/keybindings/Keybindings_zh-CN.md` 的表格行复现（按「主键是否为可打印单字符 / 浏览器原生键 / 控制键」三分类）。

> 行号以克隆时刻为准；若后续重新拉取，引用前请重新核验。
