# 前端 Git 页面：问题与任务清单

> 依据 `docs/plan/frontend-git/` 设计文档与当前实现（`apps/web-app` + `crates/app/wf-server`）的对照分析得出。
> 本清单为后续整改的唯一依据；旧计划文档不再回填、后续废弃。

## 背景

当前实现覆盖了主干契约：`/git` 路由与导航、面板 URL 持久化（`panel`）、绑定表与纯函数词元解析（含单测）、唯一写入口 `runAction` ↔ `POST /git/action`、后端确认校验（`confirmTarget`）、启动期 `checkDrift`、`GIT_CHANGED` 事件 + 指纹轮询刷新、DTO 转换隔离。

以下为差距项与整改任务，按优先级排列。

## 任务清单

### T1 终端安全加固（P0，安全项）

现状：Git 页面终端复用通用 `pty` 域（`/api/v1/pty/*` + `PtyPanel`），会话不区分 git 用途：无 cwd 锁定，无 git 安全环境变量注入。设计要求（03 文档 §5.3、06 文档 P5 5.3）：cwd 锁仓库根、注入 `GIT_TERMINAL_PROMPT=0` / `GIT_PAGER=cat`、拒绝清单。

- [ ] PTY spawn 支持 `cwd` 校验：只允许仓库根内路径，逃逸拒绝（后端）
- [ ] PTY spawn 支持按用途注入环境变量（前端从 git 页面传入 `GIT_TERMINAL_PROMPT=0`、`GIT_PAGER=cat`）
- [ ] `GitTerminalPanel` 向 `PtyPanel` 传递仓库根 cwd 与 git 安全 env

### T2 C 类不可见键控件条（P1，设计核心）

现状：`bindings.ts` 已导出 `visibleControls()`，但 `GitKeyInput` 未渲染控件条；`<esc>` / `<enter>` / `<ctrl+x>` 等不可见键无等价点击控件，鼠标用户无法零键位知识完成操作（02 文档核心目标）。

- [ ] `GitKeyInput` 渲染控件条：按当前面板 `visibleControls()` 派生点击控件，点击即触发对应动作
- [ ] 当前面板无 `nonPrintable` 项时控件区自动收起

### T3 soft 级撤销（P1，P4 验收点）

现状：workbench/store 均无 undo 状态；设计要求（06 文档 P4 4.7）写操作后 8 秒内 toast 提供撤销入口，基于 reflog 的 `undo` / `redo` 动作。

- [ ] 绑定表补 `git.undo` / `git.redo` 动作（`recoverable` 级）
- [ ] 写操作成功后发 toast，`soft` 级操作带撤销按钮（8 秒窗口）
- [ ] 撤销按钮调用 `git.undo` 动作

### T4 URL 持久化补 `tab` 键（P2，05 文档 §2）

现状：仅持久化 `panel`；设计要求 `panel` + `tab` 两键。

- [ ] 面板内 tab（如 branches 的 local/remote/tags）写入 URL 并在刷新后恢复

### T5 主视图 Staging / CommitFiles 模式（P2 评估，P6 6.1）

现状：`GitMainView` 仅 diff 展示；`stageHunk()` 接口已存在但无 Staging 主视图；设计要求 Diff / Staging / CommitFiles 三态（05 文档 §3）。

评估结论：hunk/行级 staging 需要 per-hunk 选区状态、diff 行定位与后端 `apply-patch` 支持，工作量相当于独立一个阶段（对齐 06 文档 6.1「独立评估」的定性），不在本轮落地。现有 `stageHunk` 按钮已覆盖单 hunk 暂存的最小闭环。

- [ ] 独立立项：Staging / CommitFiles 主视图三态 + 后端 hunk/行级 apply-patch

### T6 移动端降级（P3，05 文档 §9）

现状：仅 `xl:hidden` 分组面板替换；设计要求移动端底部输入栏变动作搜索框、控件条横滚。

- [ ] 依赖 T2 控件条落地后一并处理

## 非任务（确认不改）

- 组件命名/位置（`domain/git/`、`workbench.svelte.ts` 状态类）：与计划不同但职责等价，不回改。
- 双通道键盘输入（window 直接单键 + 输入栏 Enter）：比设计更高效，保留。
- 复用通用 `pty` 域而非专用 `WS /git/terminal`：保留，仅在 T1 中补安全约束。
- 面板覆盖 10 个（超出首期）：保留，属超出而非缺失。
