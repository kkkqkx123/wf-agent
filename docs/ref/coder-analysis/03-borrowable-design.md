# 对 wf-agent 的可借鉴设计（综合决策）

> 本文把 `01-backend-integration.md` / `02-vscode-git-frontend.md` 的结论收敛为**可落地决策**，并映射回既有 `docs/plan/frontend/*` 方案。行号引用同前两份文档。

## 0. 与现有 frontend 方案的关系

| 本分析的结论 | 对接的既有方案 |
|---|---|
| CLI 编排 + 串行化 + readOnly 分层 | `frontend/04-backend-git-domain.md` |
| 四分组 SCM + 专用 commit composer + 动作 ID 寻址 | `frontend/02-keyinput-bar-design.md`、`frontend/05-frontend-architecture.md` |
| 带外 askpass/editor 通道 → **修正 P5** | `frontend/03-cli-terminal-design.md` |
| 多仓库聚合 / 分支保护 | `frontend/07-open-questions.md` 的 `Q11`（仓库作用域） |

## 1. 后端决策

### 决策 1.1：git 走子进程 CLI，不内嵌库
- **依据**：`git.ts:676` 永远 `cp.spawn(this.path, args, options)`；wf-agent 已有 `wf-shell`（`portable-pty`），可直接 `exec('git', [...])`。
- **动作**：`frontend/04` 的后端 git 域用 `exec('git', [...])`，`cwd=仓库根`，复用 `wf-shell` 的进程/PTY 能力，**不要引入 `git2`/`gix`**。

### 决策 1.2：交互式 git 用 askpass / editor 带外通道，而非强塞 PTY ★本次最重要
- `frontend/03` 把 `rebase -i` / `add -p` 的可用性押在 **PTY 真交互**上，并把它列为 `P5` 阻塞未知项（`send_input` 需常驻 shell）。askpass 范式给出更稳的替代路径：
  1. 后端 spawn git 时注入 `GIT_ASKPASS` / `GIT_EDITOR` 指向**我们自己的助手脚本**（或等价于 env 的 WS/HTTP 回调地址）。
  2. 脚本把"需要凭据 / 编辑"事件经 **WS 发到浏览器**，浏览器弹模态框收集，答案经 WS 回传、写入管道文件，git 继续。
  3. **无应答时走 empty 兜底（超时返回）**，绝不挂起。
- **影响**：CLI 终端（`frontend/03`）可以**默认批处理 + 仅在确有必要（如 `add -p` 的可视化选择）时升格为带外交互**。`P5` 风险从"send_input 常驻 shell 可行性"**降级为"实现带外助手脚本"**，不再是开工阻塞项。

### 决策 1.3：操作串行化 + 空闲刷新
- **依据**：`repository.ts:2709-2748`（要求 `Idle`、`_operations` 串行化）、`repository.ts:3192-3202`（空闲才刷）。
- **动作**：git 动作端点按 `repoId` 加**操作队列（互斥）**，每次动作后刷新状态；只读动作（`status`/`log`/`diff`）不进危险队列。

### 决策 1.4：窄 API 门面 + 边界鉴权
- **依据**：`IVSCodeServerAPI`（`vscode.ts:27-32`）、路由前置中间件（`vscode.ts:240-253` vs `routes/index.ts:171`）。
- **动作**：git 的 WS / HTTP 端点统一在边界做 `ensureOrigin` + 鉴权；`destructive` 级别作为**后端属性强制校验**（与 `frontend/04` 一致，绕过 UI 直接调 API 也不能强制 push）。

## 2. 前端决策

### 决策 2.1：变更面板改四分组
- **依据**：`repository.ts:1011-1014`。
- **动作**：把 `frontend/02` 的变更区从 lazygit 扁平列表改为 **暂存 / 修改 / 未跟踪 / 合并（冲突）**，直接映射 `git status --porcelain` 的 XY 码。

### 决策 2.2：commit message 用专用 composer，不占单键输入栏
- **依据**：`commands.ts:2606-2746` + `02` 文档结论（多行文本应常驻顶部）。
- **动作**：单键输入栏只承载 215 个可打印键的离散动作；commit 走 SCM 顶部专用多行编辑器。

### 决策 2.3：状态栏常驻分支 chip + 一键 sync
- **依据**：`statusbar.ts`（`CheckoutStatusBar` / `SyncStatusBar`）。
- **动作**：顶栏显示分支 / ahead-behind，点击触发 `checkout` / `sync`（pull+push），不埋进面板。

### 决策 2.4：hunk / selection 级暂存作为差异化亮点
- **依据**：`commands.ts:1695-1966`（stageHunk / stageSelection / stageSelectedRanges / revertSelectedRanges）。
- **动作**：在文件项提供"暂存选中行 / 区块"，后端用 `git apply --cached` 接 patch。这是相对 lazygit 的体验升级，且被**决策 1.2 的带外通道支撑**（`add -p` 的可视化选择可经 WS 回流）。

### 决策 2.5：统一 action 注册表驱动三处 UI
- **依据**：`commands.ts` 的 `@command` 装饰器（`:796`）。
- **动作**：一份 git 动作注册表同时生成——面板按钮 + 命令面板 + 输入栏候选，三者永不脱节（与 `frontend/02` 的"动作 ID 寻址"一致）。

## 3. 推荐落地顺序（对齐 `frontend/06`）

1. **后端 git 域**：`exec git` CLI + `repoId` 操作队列 + 只读/危险分级（对应 `frontend/04`）。
2. **前端四分组 SCM + 专用 commit composer + 状态栏 chip**（对应 `frontend/05/06`）。
3. **带外 askpass / editor 助手 + WS 凭据回传**（消除 `P5`，替代部分 PTY 依赖）。
4. **hunk / selection 暂存**（需 patch 输入）。
5. **命令面板兜底 + 动作注册表**。
6. **多仓库聚合 + 分支保护**（对应 `Q11`）。

## 4. 更新后的开放点（合并进 `frontend/07`）

| 编号 | 开放点 | 状态 / 处置 |
|---|---|---|
| `P5`（原） | `send_input` 常驻 shell 可行性 | **降级**：改用 askpass 带外助手，不再作为开工阻塞项 |
| `Q-NEW-1` | 带外助手脚本的部署位置（随服务打包 vs 注入 env 路径） | 待拍板 |
| `Q-NEW-2` | WS 凭据回传的超时 / 取消语义（用户关弹窗 = 取消 git 操作？） | 待拍板 |
| `Q-NEW-3` | hunk 暂存 patch 的构造与校验（防注入） | 待拍板 |
| `Q-NEW-4` | 多仓库视图的 `repoId` 粒度（对应 `Q11`） | 待拍板 |
| `Q11`（原） | 仓库作用域 | 参考 `model.ts:186` 多仓库管理模型设计 |

## 5. 一句话总结

> **不要重写 git，把它当外部子系统用窄门面包一层；交互式 git 走带外通道（助手脚本 + WS 回流）而非 PTY 硬撑；前端用"四分组 SCM + 顶栏 commit composer + 状态栏 chip + 命令面板"承接 VSCode 的结构化与细粒度优势，同时保留 lazygit 输入栏式的快速动作触发。**
