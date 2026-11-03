# 前端 Git 页面：问题与任务清单

> 依据 `docs/plan/frontend-git/` 设计文档与当前实现（`apps/web-app` + `crates/app/wf-server`）的对照分析得出。
> 本清单为后续整改的唯一依据；旧计划文档不再回填、后续废弃。

## 背景

当前实现覆盖了主干契约：`/git` 路由与导航、面板 URL 持久化（`panel`）、绑定表与纯函数词元解析（含单测）、唯一写入口 `runAction` ↔ `POST /git/action`、后端确认校验（`confirmTarget`）、启动期 `checkDrift`、`GIT_CHANGED` 事件 + 指纹轮询刷新、DTO 转换隔离。

以下为差距项与整改任务，按优先级排列。

## 任务清单

### T1 终端安全加固（P0，安全项）

现状：Git 页面终端复用通用 `pty` 域（`/api/v1/pty/*` + `PtyPanel`），会话不区分 git 用途：无 cwd 锁定，无 git 安全环境变量注入。设计要求（03 文档 §5.3、06 文档 P5 5.3）：cwd 锁仓库根、注入 `GIT_TERMINAL_PROMPT=0` / `GIT_PAGER=cat`、拒绝清单。

- [x] PTY spawn 支持 `cwd` 校验：只允许仓库根内路径，逃逸拒绝（后端）
- [x] PTY spawn 支持按用途注入环境变量（前端从 git 页面传入 `GIT_TERMINAL_PROMPT=0`、`GIT_PAGER=cat`）
- [x] `GitTerminalPanel` 向 `PtyPanel` 传递仓库根 cwd 与 git 安全 env
- [x] 手动新建终端按钮也继承 git 安全 env（集成补全）

### T2 C 类不可见键控件条（P1，设计核心）

现状：`bindings.ts` 已导出 `visibleControls()`，`GitKeyInput` / `GitMainView` / `GitStatusBar` 均已渲染控件条；`<space>` / `<enter>` / 空别名键均有等价点击控件（02 文档核心目标）。

- [x] `GitKeyInput` 已渲染 `visibleControls()` 派生控件条，点击触发对应动作
- [x] 控件区在无控件时自动收起（`GitKeyInput` 条件渲染，`GitMainView` footer 条件收起）
- [x] 已引入 `nonPrintable` / `controlPriority` / `legacyKey` 元数据；`visibleControls` 按非可见键优先级取前 4 再用高权重可见键补足至 6
- [x] 词元解析已修复空格截断问题，支持多字符别名与动作 ID 后缀匹配

### T3 soft 级撤销（P1，P4 验收点）

现状：基于 reflog 的 `undo` / `redo` 动作已落地；写操作后 8 秒内 toast 提供撤销入口（06 文档 P4 4.7）。

- [x] 历史移动类 `recoverable` 操作带撤销按钮（8 秒窗口，`UNDO_TOAST_ACTIONS` 白名单避免误导）
- [x] 撤销按钮调用 `undo` 动作，`undo` 成功后同窗口提供 `redo` 入口
- [x] 绑定表已补 `redo`；保留 `undo` / `redo` 命名（与后端注册表一致，不改名为 `git.undo` / `git.redo`）
- [x] 后端 `undo` 基于 reflog 真实撤销（`reset --soft` 恢复历史移动，`checkout` 恢复分支切换），`redo` 基于 git 目录持久化栈实现；合并/变基/cherry-pick 中间态拒绝；其他写操作成功后清空 redo 栈

### T4 URL 持久化补 `tab` 键（P2，05 文档 §2）

现状：仅持久化 `panel`；设计要求 `panel` + `tab` 两键。

- [x] `panel` + `tab` 已写入 URL 并在刷新后恢复（当前 `tab` 对应 diffSource）

### T5 主视图 Staging / CommitFiles 模式（P2 评估，P6 6.1）

现状：`GitMainView` 仅 diff 展示；`stageHunk()` 接口已存在但无 Staging 主视图；设计要求 Diff / Staging / CommitFiles 三态（05 文档 §3）。

评估结论：hunk/行级 staging 需要 per-hunk 选区状态、diff 行定位与后端 `apply-patch` 支持，工作量相当于独立一个阶段（对齐 06 文档 6.1「独立评估」的定性），不在本轮落地。现有 `stageHunk` 按钮已覆盖单 hunk 暂存的最小闭环。

- [ ] 独立立项：Staging / CommitFiles 主视图三态 + 后端 hunk/行级 apply-patch（含行级选区、补丁构建、冲突解决视图）
- [x] 清理 fixture 中遗留的 `stage.lines` 占位契约
- [x] 本轮最小加固：`stage.hunk` 后端按 `@@` 行切分 hunk 并保留 `diff --git` 头，修复旧的 `split("@@")` 错位与临时文件命名冲突

### T6 移动端降级（P3，05 文档 §9）

现状：`xl:hidden` 分组面板替换保留；输入区与控件条已做移动端横滚降级。

- [x] 控件条移动端横向滚动（`GitKeyInput` / `GitMainView` footer / `GitStatusBar` 均已 `overflow-x-auto` + 按钮 `shrink-0`）
- [x] 输入区移动端纵向堆叠（`flex-col sm:flex-row`），触屏以候选点击为主


## 当前进度补充（基于代码审计）

### 已完成/基本完成
- T1：后端 cwd 校验、env 注入、前端传递已落地；手动新建终端 env 遗漏已修复。
- T4：`panel` / `tab` URL 持久化已落地。
- T3 前端交互：`recoverable` 写操作 8 秒 Undo toast 已落地。
- 前端/后端 available action ID 基本对齐，启动期 `checkDrift` 可用。
- 后端写操作发布 `GIT_CHANGED`，前端 live 订阅并做指纹轮询刷新。

### 部分完成
- T5：三态主视图仍为独立立项，仅完成 `stage.hunk` 补丁构造加固。

### 集成与清理已完成
- `PtyPanel` 新建终端按钮现在继承 git 安全 env。
- preview fixture registry 已与后端 available registry 对齐（含 `redo` 可用）。
- preview mock 已补齐 `/api/v1/git/reflog`。
- preview fixture 已删除遗留 `stage.lines`。
- 前端 bindings 已补齐 `redo`，`visibleControls` 已按新元数据派生。

### 仍需后续处理
- T5 的 Staging / CommitFiles 主视图三态与行级 apply-patch（独立立项）。
- preview fixture/mock 与真实后端的持续同步。

## 非任务（确认不改）

- 组件命名/位置（`domain/git/`、`workbench.svelte.ts` 状态类）：与计划不同但职责等价，不回改。
- 双通道键盘输入（window 直接单键 + 输入栏 Enter）：比设计更高效，保留。
- 复用通用 `pty` 域而非专用 `WS /git/terminal`：保留，仅在 T1 中补安全约束。
- 面板覆盖 10 个（超出首期）：保留，属超出而非缺失。
