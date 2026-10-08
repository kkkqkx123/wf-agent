# 后端 `git` 域设计

> 本文回答：为支撑 Git 工作台，后端要新增什么、放在哪、接口怎么切、git 怎么执行、安全边界在哪。
>
> 分层约定依据：`AGENTS.md`（`crates/` 四层与严格 DAG）、`crates/app/wf-server/src/router.rs:44-104`（域路由注册）。

---

## 1. 现状：后端完全没有 git 域

| 检索 | 结果 | 结论 |
|---|---|---|
| `git2` / `gix` 依赖 | 全仓零引用 | 无任何 git 库绑定 |
| `Cargo.toml` workspace 依赖 | 无 git 相关条目 | 需新增时走集中管理（`AGENTS.md`：Rust deps 集中在根 `Cargo.toml`） |
| `"git"` 字符串出现位置 | 仅 shell 白名单（`wf-sandbox/src/command_policy.rs:962-1007`）、命令安全测试（`wf-shell/src/command_safety.rs:266-333`）、审批策略示例（`wf-runtime/src/tool_approval.rs:275`） | 只是测试/示例数据 |
| `wf-api` 域清单 | agent / analysis / audit / checkpoint / entity / infra / llm / template / trigger / web / workflow | 无 git 或 vcs 域 |
| `wf-server` 域路由 | 同上 11 个域 + ws | 无 git 路由 |

**结论**：这是从零新增一个域，但所需的基础设施（域路由注册点、WS、PTY、命令安全、偏好持久化）全部现成。工作量是「填一个域」，不是「搭一套设施」。

---

## 2. 放置位置与分层

按 `AGENTS.md` 的四层约定，git 能力属于**应用层的领域逻辑**（它不强依赖 agent/workflow 引擎，是被前端直接消费的能力面），因此：

| 层 | 新增内容 | 说明 |
|---|---|---|
| `app` / `wf-api` | `crates/app/wf-api/src/git.rs` + `crates/app/wf-api/src/git/` 子模块 | 领域逻辑：动作注册表、git 命令封装、仓库状态快照 |
| `app` / `wf-server` | `crates/app/wf-server/src/api/git/`（含 `git.rs` 与若干 `*.rs`） | HTTP 传输层：routes、请求/响应模型、utoa OpenAPI 标注 |
| — | **不新增独立 crate** | 避免为一组 REST 接口撑出一个新 crate；`wf-api` 的域划分（`agent.rs` / `web.rs` / `workflow.rs`）已是既有惯例 |

文件命名遵守 `AGENTS.md`：禁止 `mod.rs`，子模块一律 `<module_name>.rs` 平铺，`lib.rs` 只做 `pub mod` + `pub use`。

路由注册只需在 `router.rs` 的域 merge 链中加 git 段（位置参考 `router.rs:72-75` 的 web 域写法）。

---

## 3. 接口清单

前缀统一 `/api/v1/git`。只读用 `GET`，写操作统一走 `POST`（见 §4）。

### 3.1 元信息与仓库

| 方法 | 路径 | 用途 |
|---|---|---|
| `GET` | `/git/repo` | 仓库根路径、当前分支、上游、ahead/behind、是否处于变基/合并/二分等中间态 |
| `GET` | `/git/actions` | **动作注册表**：全部 `actionId` + `danger` 级别 + `params` 声明。前端启动期用它校验绑定表（见 02 文档 §2.3） |

### 3.2 只读数据（对齐 lazygit 面板）

| 方法 | 路径 | 对应 lazygit 面板 |
|---|---|---|
| `GET` | `/git/status` | Status + 顶栏（分支/上游/ahead-behind/中间态） |
| `GET` | `/git/files` | Files（含 staged/unstaged/conflict 三态，支持扁平/树形两种视图） |
| `GET` | `/git/diff` | 主视图：工作区 diff / 暂存 diff / 提交 diff，按 scope 参数区分 |
| `GET` | `/git/commits` | Commits / Reflog / SubCommits（分页 + 过滤 + 图数据） |
| `GET` | `/git/commits/{sha}/files` | CommitFiles |
| `GET` | `/git/branches` | Branches（本地） |
| `GET` | `/git/remotes` | Remotes |
| `GET` | `/git/remote-branches` | RemoteBranches |
| `GET` | `/git/tags` | Tags |
| `GET` | `/git/stash` | Stash |
| `GET` | `/git/submodules` | Submodules |
| `GET` | `/git/worktrees` | Worktrees |

### 3.3 写操作：统一动作入口

**不按动作开路由**，只开一个：

```
POST /api/v1/git/action
body: { actionId, params?, selection? }
```

理由：

- 前端新增动作不需要后端新增路由，绑定表加一行即可（02 文档的核心收益得以兑现）。
- `danger` 分级、`params` 校验、审计日志都收敛在一处，不会因路由分散而漏掉。
- lazygit 本身也是「键位 → handler」的分发模型（`pkg/gui/keybindings.go:393-412`），语义一致。

后端维护一张 `actionId → handler` 的注册表，handler 签名统一（接收 params + selection，返回仓库状态增量或操作结果）。未知 `actionId` 返回明确错误，不猜测。

### 3.4 终端

| 方法 | 路径 | 用途 |
|---|---|---|
| `WS` | `/api/v1/git/terminal` | CLI 终端（PTY 会话），协议见 03 文档 §4.2 |

### 3.5 首期动作清单（P1–P4）

按 lazygit 中文键位表的高频项选取，覆盖即可完成日常闭环：

**Files**：`files.toggle-stage`（别名空格）、`files.toggle-stage-all`（`a`）、`files.commit`（`c`）、`files.commit-no-verify`（`w`）、`files.amend`（`A`）、`files.discard`（`d`）、`files.stash-all`（`s`）、`files.ignore`（`i`）、`files.refresh`（`r`）、`files.fetch`（`f`）、`files.open-diff`（Enter）、`files.toggle-tree-view`（`` ` ``）

**Branches**：`branches.checkout`（空格）、`branches.checkout-by-name`（`c`）、`branches.create`（`n`）、`branches.delete`（`d`）、`branches.rebase`（`r`）、`branches.merge`（`M`）、`branches.rename`（`R`）、`branches.fast-forward`（`f`）

**Commits**：`commits.checkout`（空格）、`commits.reword`（`r`）、`commits.squash`（`s`）、`commits.fixup`（`f`）、`commits.drop`（`d`）、`commits.revert`（`t`）、`commits.cherry-pick`（`C`）、`commits.paste`（`V`）、`commits.create-branch`（`n`）、`commits.tag`（`T`）、`commits.move-up` / `commits.move-down`（原 `<ctrl+k>` / `<ctrl+j>`，C 类，靠动作名触达）

**Stash**：`stash.apply`（空格）、`stash.pop`（`g`）、`stash.drop`（`d`）、`stash.to-branch`（`n`）

**全局**：`global.push`（`P`）、`global.pull`（`p`）、`global.refresh`（`R`）、`global.undo`（`z`）、`global.redo`（`Z`）、`global.focus-main`（`0`）、`global.filter`（`/`）

**首期不实现**（在 `/git/actions` 中返回为不可用，前端置灰并说明）：外部 difftool（`e` / `<ctrl+t>` 的部分语义）、PR 相关（`o` / `O` / `G` / `<ctrl+y>`）、git-flow（`i`）、自定义补丁构建（`<ctrl+p>` 与 PatchBuilding 面板）、bisect（`b`）、worktrees / submodules 的写操作。

---

## 4. Git 执行策略

### 4.1 用 `git` 二进制，不用 git2 / gix

| 维度 | `git` 二进制 | git2 |
|---|---|---|
| 与 lazygit 行为一致性 | 完全一致（lazygit 即此方案） | 需逐项对齐，易出现行为差异 |
| SSH / credential helper | 全支持（走用户环境） | 需自行实现，覆盖不全 |
| 交互式命令（`add -p`） | 支持（配合 PTY） | 不支持 |
| 依赖体积 | 无新增 | 引入 C 绑定，编译变慢 |
| 注入风险 | 可控（见 §4.2） | 天然无 |

选 `git` 二进制。代价是要自己处理输出解析，但 git 的 `--porcelain` 系列输出格式就是为此设计的。

### 4.2 防注入：argv 数组，不经 shell

**强制约定**：所有 git 调用以 `Command::new("git").args([...])` 形式发起，**永不拼接成字符串交给 `sh -c`**。用户提供的路径、分支名、提交信息一律作为独立 argv 元素传入。

由此派生的约束：

- 需要管道/重定向的地方，改用 git 原生能力（`git log --graph` 而非 `git log | graph-tool`；`git diff --stat` 而非 `git diff | wc`）。
- 确实需要组合时，在 Rust 侧做数据组合，不借助 shell。
- 路径参数统一先做**仓库内校验**：解析为绝对路径后必须位于仓库根之内，否则拒绝（防 `../` 逃逸与绝对路径注入）。

约定应写成一条 lint 级或 review 级的硬性规则，因为这是本域最主要的安全风险面。

### 4.3 输出解析

| 用途 | 命令与格式 |
|---|---|
| 文件状态 | `git status --porcelain=v2 --branch`（机器可读，含分支与 ahead/behind） |
| diff | `git diff` / `git diff --cached` / `git diff <rev>`，按 hunk 解析；冲突文件用 `--diff-filter=U` 识别 |
| 提交列表 | `git log --pretty=format:` 配自定义分隔符 + `--graph`（可选） |
| 分支 | `git branch --format=` / `git for-each-ref --format=`（后者更稳） |
| stash | `git stash list --format=` |
| 状态指纹 | 见 03 文档 §5.2：`status --porcelain=v2` + `HEAD` + 分支/上游的摘要 |

一律使用 `--porcelain` / `--format` / `-z` 等稳定机器格式，不解析面向人类的默认输出。解析逻辑集中在 `wf-api/src/git/` 内，不散落到 server 层。

### 4.4 并发与超时

- 命令执行设超时（建议 15 秒；`push` / `fetch` / `clone` 类放宽到 120 秒），超时返回明确错误而非悬挂。
- 同一仓库的写操作串行化（一把仓库级异步锁），避免并发 git 命令互相踩 `.git/index.lock`。
- 只读命令不串行。

---

## 5. 安全边界

| 层 | 措施 | 依据/落点 |
|---|---|---|
| 仓库作用域 | 服务端配置指定仓库根路径；**所有路径参数必须落在仓库根内** | 新增配置项，走 `wf-config` |
| 命令构造 | argv 数组，不经 shell | §4.2 硬性约定 |
| 写操作白名单 | `POST /git/action` 只接受注册表中的 `actionId` | §3.3 |
| 动作前置校验 | handler 内校验选中项类型与仓库状态，不满足返回带原因的错误 | 对齐 lazygit 的 `GetDisabledReason`（`pkg/gui/keybindings.go:393-412`） |
| 危险分级 | `danger` 由后端注册表声明，前端不得自行降级 | 02 文档 §6 |
| 破坏性动作 | `destructive` 级要求客户端传入确认标识（目标分支名/短哈希），后端二次校验后才执行 | 防止前端绕过 UI 直接调 API |
| CLI 终端 | 会话 cwd 锁仓库根、拒绝路径逃逸、注入受控 env、拒绝清单、空闲回收 | 03 文档 §6 |
| 鉴权/限流 | 沿用 `middleware` 现有配置（git 域与 `/api/v1/*` 一致） | `crates/app/wf-server/src/middleware.rs` |

「`destructive` 级要求客户端传确认标识并在后端校验」这一条值得强调：**危险级别是后端属性，不是前端装饰**。前端的确认弹窗只是 UX，后端必须独立校验，否则 API 可被直接绕过。

---

## 6. 状态刷新与事件

| 事件 | 通道 | 触发 |
|---|---|---|
| `repo_changed` | **复用现有 WS**（`GET /api/v1/ws`） | 仓库状态指纹变化；来源可以是 GUI 动作，也可以是 CLI 命令（03 文档 §5.2） |
| 终端输出/退出 | **独立 WS** `/api/v1/git/terminal` | PTY 会话（03 文档 §4.2） |

`repo_changed` 走现有 WS 的理由：它是全局广播型状态，与现有执行/agent/workflow 事件的形态一致，且前端已有一套订阅与重连（`cursor`）机制可复用（`ws.rs:1-30`）。终端流是「一连接绑一会话」的双向流，形态不同，故独立端点。

两者都不需要新增基础设施，只是各自注册一种通道。

---

## 7. OpenAPI 与前端契约

| 事项 | 做法 |
|---|---|
| Schema 标注 | 新增路由按现有域的写法加 utoa 标注，保证能进 `/api-docs/openapi.json` |
| 前端类型 | 沿用 `apps/web-app/openapi.json` 离线快照 + `tools/openapi-codegen` 生成 `src/lib/api/schema.d.ts`（`AGENTS.md` 描述的代码gen 流程） |
| 契约漂移 | 前端绑定表的 `actionId` 与后端 `/git/actions` 启动期比对（02 文档 §2.3） |
| 状态字段 | 与既有约定一致：状态类字段在契约中是 `string` 而非枚举，前端需为未知值提供中性兜底（`docs/plan/web/web-app-ui-implementation-design.md` §1） |

---

## 8. 待拍板项

| # | 问题 | 建议默认值 | 影响 |
|---|---|---|---|
| Q11 | 仓库根路径如何配置（服务端固定 / 请求参数 / 多仓库） | 服务端配置固定单仓库 | 影响是否要做 recent-repos 与仓库切换 UI |
| Q12 | 写操作超时值 | 常规 15s，push/fetch 120s | 慢网络下 push 可能被误判超时 |
| Q13 | 是否对同一仓库的写操作加串行锁 | 是 | 影响并发体验，但防 `.git/index.lock` 冲突 |
| Q14 | `destructive` 确认标识的形式 | 目标分支名/短哈希字符串 | 影响前端弹窗文案与后端校验实现 |
| Q15 | 是否需要把 git 动作纳入审计域（`wf-api/audit`） | 建议纳入 | 现有 audit 域已存在，接入成本低 |

完整清单见 `07-open-questions.md`。
