# 命令行界面（CLI）设计

> 本文回答：GUI 之上的命令行界面该做成什么形态、走什么链路、如何与上半部分的 Git 面板共享状态。
>
> 上游依据：`00-overview.md` §3（核心决策）、`02-keyinput-bar-design.md` §5（参数化动作交给 CLI）。

---

## 1. 需求定位

CLI 不是 lazygit 的 `:`（那条只是「执行单条 shell 命令」的提示符）。本方案的 CLI 要承担三件事：

1. **兜底 GUI 未覆盖的能力**：任何没做进面板的 git 能力都能在这里完成，用户不必离开页面。
2. **承担参数化/复杂表达**：输入栏不做内联参数（见 02 文档 §5），所有带复杂参数的完整 git 表达走 CLI。
3. **承接交互式 git 命令**：`git add -p`、`git rebase -i`、`git commit`（唤起编辑器）这类需要真实终端的命令。

第 3 条是选型的决定性约束——它直接排除了「纯批处理伪终端」方案。

---

## 2. 形态选型

### 2.1 两种候选

| 方案 | 描述 | 交互式命令 | 安全控制 | 实现成本 |
|---|---|---|---|---|
| **A 批处理伪终端** | 每条命令 spawn 一个进程，共享 cwd/env，输出以块回显 | **不支持**（`add -p` / `rebase -i` 会挂死） | 逐条走命令白名单，天然可审计 | 低 |
| **B PTY 常驻 shell** | 起一个真实 shell 会话（PTY），前端用终端组件连接 | **支持** | 只能在会话级约束（cwd、env），无法逐条审计 | 中 |

### 2.2 结论：CLI 用 B，后端动作仍用 A

这不是二选一，而是**按入口分流**：

| 入口 | 走哪条 | 理由 |
|---|---|---|
| GUI 动作（输入栏 / 控件 / 右键菜单）→ `POST /git/action` | **A 批处理** | 需要危险分级、白名单、可审计；动作集合封闭 |
| CLI 终端 | **B PTY 常驻 shell** | 用户自由输入，必须支持交互式命令 |

选 B 的底气来自 `wf-shell` 已经具备全部能力（见 §3），不需要引入新依赖或新 crate。

### 2.3 B 方案的风险与缓解

| 风险 | 缓解 |
|---|---|
| 无法逐条审计命令 | 会话级约束：cwd 锁仓库根、env 注入受控、会话超时回收；记录完整输出流到操作日志 |
| 常驻 shell 可能残留后台进程 | 复用 `sweep_idle_sessions`（`store.rs:485`）与 `kill`（`store.rs:426`）；页面关闭时显式 kill |
| 用户误执行破坏性命令 | CLI 区显眼提示「此终端直接作用于服务端仓库」；首次打开需确认；`destructive` 级 git 命令可在前端做二次确认拦截（见 §6） |
| 多人同时打开页面的写冲突 | 首期不解决，标注为已知限制（见 `07-open-questions.md` Q8） |

---

## 3. 后端能力现状（可直接复用）

`wf-shell` 已经提供了构建 Web 终端所需的**全部**后端原语：

| 能力 | 接口 | 依据 |
|---|---|---|
| 创建会话，可选 PTY | `SessionCreateOptions { cwd, env, interactive, force_pty, pty_size }` | `crates/infra/wf-shell/src/store.rs:50-60` |
| 会话内执行命令 | `execute_in_session(session_id, command, timeout_ms)` | `store.rs:342-347` |
| 向运行中命令写输入 | `send_input(session_id, input, enter)` | `store.rs:388-399` |
| 调整终端尺寸 | `resize(session_id, rows, cols)` | `store.rs:410` |
| 终止 / 回收 | `kill` / `kill_with(graceful)` / `sweep_idle_sessions(idle_timeout_ms)` | `store.rs:426,432,485` |
| 增量输出与按行事件 | `OutputBuffer` + `OutputLineDispatcher` + `EventDispatcher` | `session.rs:1-6`、`terminal_session.rs:1-8` |
| PTY 实现 | `portable-pty`，**始终编译进**（非 feature-gated），由 `pty_enabled` 开关 | `crates/infra/wf-shell/src/lib.rs:18-30` |
| 命令安全策略 | 白名单 / 前缀匹配 / 拒绝规则 | `crates/infra/wf-shell/src/command_safety.rs:266`；`crates/infra/wf-sandbox/src/command_policy.rs:962-1007` |
| 交互式会话高级封装（参考实现） | prompt 模式检测、轮次上限、workspace capture | `crates/engine/wf-execution-shared/src/interactive_script_session/driver.rs:89,290` |

**不需要做的事**：不引入 `git2` / `gix`、不引入新的 PTY 库、不新写进程管理。

**需要验证的一点**：`send_input` 要求会话中存在处于 `running` 状态的当前命令（`store.rs:388-399`）。因此 PTY 常驻 shell 的用法是：先 `execute_in_session(sid, "<shell>", None)` 起一个不设超时的常驻 shell，之后所有输入走 `send_input`。这一行为需在 P5 阶段用实测确认（见 `06-implementation-plan.md` P5 验收点）。

---

## 4. 链路设计

```
┌──────────────┐   WS /api/v1/git/terminal    ┌────────────────┐
│ GitTerminal  │ ───────────────────────────► │  terminal.rs   │
│ (xterm.js)   │ ◄─────────────────────────── │  (axum ws)     │
└──────────────┘   output / exit / error      └───────┬────────┘
                                                      │
                                              wf-shell BackgroundShellStore
                                              (PTY session, cwd=repo root)
```

### 4.1 为什么不复用现有 WS

现有 `GET /api/v1/ws`（`crates/app/wf-server/src/ws.rs:1-30`）是**事件订阅模型**：客户端发 `subscribe` / `unsubscribe` / `ping`，服务端推事件。它是广播式的，与「一个连接绑定一个终端会话、双向流式」的需求形态不同。

硬塞进去会让 `ws.rs` 长出第二种互不兼容的语义。**结论：新建独立端点 `WS /api/v1/git/terminal`**，与现有 WS 并存，各自保持单一语义。

### 4.2 协议帧（描述）

客户端 → 服务端：

| 帧 | 载荷 | 说明 |
|---|---|---|
| `start` | `{ cols, rows }` | 建立会话（幂等；已存在则复用） |
| `input` | `{ data }` | 原始字节流（含控制字符），转发 `send_input` |
| `resize` | `{ cols, rows }` | 转发 `resize` |
| `kill` | — | 终止当前会话 |
| `ping` | — | 保活 |

服务端 → 客户端：

| 帧 | 载荷 | 说明 |
|---|---|---|
| `ready` | `{ sessionId, cwd, shell }` | 会话就绪 |
| `output` | `{ data }` | 增量输出（UTF-8 分块，前端交给 xterm 渲染） |
| `exit` | `{ code }` | 当前前台命令结束 |
| `repo_changed` | `{ revision }` | 仓库状态变化（见 §5） |
| `error` | `{ message, stage }` | 失败（启动/写入/尺寸/回收） |

`repo_changed` 直接复用现有 WS 的事件名风格（`ws.rs` 中事件均带 `timestamp` 与 `type`），保持一致。

### 4.3 前端组件

新增运行时依赖 `@xterm/xterm`（+ `@xterm/addon-fit` 用于自适应尺寸）。这是本方案唯一需要新增的前端运行时依赖。

`GitTerminal.svelte` 职责：

- 挂载 xterm 实例，处理 `onData`（用户输入 → `input` 帧）与 `onResize`（→ `resize` 帧）。
- 连接生命周期与页面/面板绑定：折叠时不销毁会话（避免丢失 cwd 与历史），离开 `/git` 路由时销毁。
- 提供「清空」「终止当前命令」「重启会话」三个按钮。
- 输出做基本的 ANSI 着色（xterm 原生支持）。

不自己做终端渲染库——xterm.js 是事实标准，自研不划算。

---

## 5. 与 GUI 的状态联动

这是 CLI 设计的价值所在：在终端里改了仓库，上半部分的面板要跟着变。

### 5.1 触发方式三选一

| 方案 | 描述 | 评价 |
|---|---|---|
| 前端猜测 | 提交命令时用正则判断是否 git 写命令，延迟拉状态 | 不可靠（别名、复合命令、`&&` 链） |
| 后端命令钩子 | 拦截 CLI 的每条命令做判断 | PTY 下拿不到「一条命令」的边界，不可行 |
| **状态快照比对** | 维护仓库状态指纹，命令结束后比对，变化则推 `repo_changed` | **推荐** |

### 5.2 推荐方案（状态快照比对）

- 后端维护一个仓库状态指纹：`git status --porcelain=v2` + `HEAD` + 分支/上游信息的摘要。
- 时机：CLI 输出流出现「前台命令结束」信号（`exit` 帧）后触发一次比对；同时对长命令做节流（最少间隔 1 秒）。
- 指纹变化 → 通过 WS 推 `repo_changed { revision }` → 前端按 scope 局部刷新（文件面板 + 提交面板 + 分支面板）。
- 指纹未变 → 不推。

这个方案同时解决两个场景：CLI 里跑了 `git checkout`（git 命令）和 `rm foo.txt`（非 git 命令但改变了工作区）。

### 5.3 反向联动（可选增强）

GUI 动作也可回显到 CLI：每次 `POST /git/action` 执行后，把等价的 git 命令以回显行形式写入终端（形如 `> git add src/a.rs`）。价值是**教学**——用户能从 GUI 操作反向学到 git 命令。成本低（一个输出帧），建议纳入 P6。

---

## 6. 安全边界

| 层 | 措施 |
|---|---|
| 工作目录 | 会话 cwd 固定为服务端配置的仓库根；允许用户在仓库内 `cd`，但拒绝逃逸到仓库外（后端校验解析后的真实路径必须在仓库根之内） |
| 环境变量 | 继承服务端进程环境 + 注入受控 overlay（如 `GIT_TERMINAL_PROMPT=0` 避免卡在凭据交互、`GIT_PAGER=cat`、`GIT_EDITOR` 指向可用的非阻塞编辑器） |
| 命令策略 | 复用 `command_safety` / `command_policy` 的拒绝规则（`rm -rf /`、管道执行远程脚本等）；PTY 模式下无法逐条白名单，故只施加**拒绝清单**而非允许清单 |
| 危险 git 命令 | 前端在 `input` 提交前对明文命令做一次匹配，`destructive` 级（`push --force`、`reset --hard`、`branch -D`、`clean -fd` 等）弹二次确认；属便利提醒，不作为唯一防线 |
| 会话回收 | 空闲超时（建议 30 分钟）自动 kill，复用 `sweep_idle_sessions`；页面 `beforeunload` 主动发 `kill` |
| 审计 | 输出流完整记录到操作日志，供后续回溯 |

`GIT_TERMINAL_PROMPT=0` 这条很重要：不设置的话，一个需要凭据的 `git fetch` 会让 PTY 永远卡在等待输入，前端表现为终端假死。

---

## 7. CLI 与输入栏的分工边界

两者都能「用文本执行动作」，必须划清边界，否则用户不知道该用哪个。

| 场景 | 用哪个 | 理由 |
|---|---|---|
| 高频动作（暂存、提交、放弃变更） | **输入栏** | 有危险分级、有确认、有撤销、有参数引导 |
| 复杂/罕见 git 表达 | **CLI** | 输入栏不覆盖全集 |
| 需要交互的命令（`add -p`、`rebase -i`） | **CLI** | 只有 PTY 能承载 |
| 需要脚本化/批量（`for` 循环、`git log --graph --oneline --all`） | **CLI** | |
| 想看命令到底怎么执行的 | **CLI** | 配合 §5.3 的 GUI 回显 |

输入栏的 `:` 前缀（02 文档 §3.1）直接把输入转发给 CLI——这条设计让两者不是割裂的两个世界，而是「输入栏是快捷方式，CLI 是完整表达」。

---

## 8. 待拍板项

| # | 问题 | 建议默认值 | 影响 |
|---|---|---|---|
| Q6 | CLI 是否首期就上 PTY（B） | 是（基础设施已完备） | 若改走 A，交互式命令需标注「请用本地终端」 |
| Q7 | 是否新增 `@xterm/xterm` 依赖 | 是 | 前端体积增加约 400KB（gzip 后约 130KB）；可接受 |
| Q8 | 多人并发写仓库 | 首期不处理，仅文档标注 | 影响是否需要仓库级锁 |
| Q9 | 空闲回收时长 | 30 分钟 | 太短会丢失用户上下文 |
| Q10 | 是否做 GUI → CLI 命令回显 | P6 做 | 成本低，教学价值高 |

完整清单见 `07-open-questions.md`。
