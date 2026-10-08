# VSCode 风格 Git 前端操作分析

> 所有 `file:line` 引用：VSCode git 扩展见 `04c0d99f`（`extensions/git/src/`）。与 lazygit 的对比见末尾速览表。

## 0. 核心模型：SCM 统一收件箱

VSCode 用单一 **Source Control** 面板聚合所有仓库，每个仓库展开为 **4 个资源分组**：

```ts
// extensions/git/src/repository.ts:1011-1014
this._mergeGroup        = this._sourceControl.createResourceGroup('merge',        l10n.t('Merge Changes'));   // 冲突中的文件
this._indexGroup        = this._sourceControl.createResourceGroup('index',        l10n.t('Staged Changes'));  // 已暂存
this._workingTreeGroup  = this._sourceControl.createResourceGroup('workingTree',  l10n.t('Changes'));        // 工作区修改
this._untrackedGroup    = this._sourceControl.createResourceGroup('untracked',    l10n.t('Untracked Changes')); // 未跟踪
```

- 这 4 组**直接对应 `git status --porcelain` 的 XY 状态码**（`M/A/D/R/C/U` 与 `??` 等），比 lazygit 的扁平列表更结构化、可递归。
- 状态机极简：`RepositoryState { Idle, Disposed }`（`repository.ts:44-46`），空闲才刷新（`eventuallyUpdateWhenIdleAndWait`，`repository.ts:3192-3202`）。

## 1. 顶栏 commit 编辑器（一等公民）

- 多行输入框**常驻 SCM 顶部**：Commit / Commit & Push / 签名 / Amend / 空提交 / `--no-verify` 等。
- 一组 `git.commit*` 命令（`commands.ts:2606-2746`）：`git.commit` / `git.commitAmend` / `git.commitSigned` / `git.commitAll` / `git.commitStaged*` / `git.commitEmpty` / `git.commitNoVerify` ……
- **与 lazygit 区别**：lazygit 把 commit 放进专用面板、用单键触发；VSCode 把它做成**始终可见的 composer**。
- 借鉴：commit message 是多行文本，**不适合塞进单字符输入栏**——应如 `frontend/02` 所述，commit 用专用 composer，而输入栏只承载离散动作（215 个可打印键动作）。

## 2. 逐文件 / 逐 hunk 内联操作（最大亮点）

- 每个文件项可 `stage` / `unstage` / `discard` / `compare`。
- **行 / hunk 级暂存**（`commands.ts`）：
  - `git.diff.stageHunk` —— `commands.ts:1695`
  - `git.diff.stageSelection` —— `commands.ts:1704`
  - `git.stageSelectedRanges` —— `commands.ts:1769`
  - `git.unstageSelectedRanges` —— `commands.ts:2055`
  - `git.revertSelectedRanges` —— `commands.ts:1966`
- 底层靠 `git apply --cached` / `git add -p` 的 patch 输入。
- **借鉴**：hunk 级暂存是 VSCode 相对 lazygit 的体验优势；且它**要求后端支持 patch 输入**——而这正是 `01-backend-integration.md` §2.2 的 askpass 带外通道能支撑的交互式 `add -p`。

## 3. 状态栏作为分支 / 同步命令面

- `statusbar.ts` 的 `CheckoutStatusBar`：显示当前分支，可点 `git.checkout`（`:53`）。
- `SyncStatusBar`：显示 ahead/behind，提供 `git.publish` / `git.sync`（pull+push）（`:209-247`）。
- **借鉴**：分支切换与一键同步应放在**顶栏 chip**，而不是埋在面板里（lazygit 把它们做成面板 + 单键，VSCode 证明顶栏常驻更顺手）。

## 4. 命令面板 = 全动作兜底

- `commands.ts` 共 **5780 行、约 200 个 `git.*` 命令**（`clone`/`init`/`merge`/`rebase`/`reset`/`stash`/`push`/`pull`/`revert`/`clean` ……）。
- 用 `@command` 装饰器统一注册（`:796` 的 `registerCommand` 封装）。
- **借鉴**：命令面板是 GUI 没覆盖动作的统一出口，等价于 lazygit 的键位表 + wf-agent 已有的 CommandPalette。可复用一个 **action 注册表**同时驱动面板按钮 / 命令面板 / 输入栏候选（与 `frontend/02` 的"动作 ID 寻址"一致，三者永不脱节）。

## 5. 冲突解决内联化

- `git.acceptMerge` —— `commands.ts:1829`
- `git.runGitMerge` / `git.runGitMergeDiff3` —— `commands.ts:1887` / `:1892`
- 冲突文件进 **Merge Changes** 组，提供 Accept Current / Incoming / Both。
- `GitEditorDocumentLinkProvider`（`gitEditor.ts:71-117`）：commit message 中的改动文件可点击跳转，把文本变成可操作链接。

## 6. 多仓库 / 子模块 / 分支保护

- `model.ts:186` `export class Model` 管理多个 `OpenRepository`，并有 parent / unsafe / closed 仓库管理器（`:64` / `:100` / `:143`）；`:764` `onDidDisappearRepository` 处理仓库消失。
- `branchProtection.ts`：阻止向受保护分支 push，弹确认。
- **借鉴**：多仓库聚合视图对应 `frontend` 的 `Q11`（仓库作用域）；分支保护映射到 `destructive` 分级的**后端强制校验**（绕过 UI 直接调 API 也不能强制 push）。

## 7. 与 lazygit 风格对比（速览）

| 维度 | lazygit | VSCode SCM |
|---|---|---|
| 操作入口 | 单键（输入栏） | 鼠标 + 命令面板 + 状态栏 |
| 变更视图 | 扁平列表 + 子面板 | 4 资源分组 + 递归 |
| 暂存粒度 | 文件 / hunk（子面板） | 文件 / hunk / selection / ranges（内联） |
| 交互式 git | 直接进子流程 | 带外通道（askpass / editor） |
| 冲突 | 合并面板 | Merge Changes 组 + 内联接受 |
| 分支 / 同步 | 面板 + 键 | 状态栏 chip + 一键 sync |
| commit | 专用面板 + 单键 | 顶栏常驻 composer |

> 结论：VSCode 的强项在**结构化（四分组）、细粒度（hunk/selection 暂存）、常驻入口（状态栏 chip + commit composer）**；lazygit 的强项在**键盘流与输入栏式的快速动作触发**。二者可互补——这正是 `frontend/02` 输入栏方案 + 本分析前端范式结合的基础。
