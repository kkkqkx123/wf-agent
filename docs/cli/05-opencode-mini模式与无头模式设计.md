# wf-cli mini 模式与无头模式设计（opencode mini 参考引入）

> 本文档基于对 opencode `--mini` 模式（`/workspace/opencode/packages/opencode/src/cli/cmd/run/`，分析见 `/workspace/docs/analysis/opencode-mini-*.md`）
> 的源码复核，结合 `docs/cli/01-04` 既有设计（全屏 alt-screen TUI + headless 管理命令面）与 Rust 侧领域层
> （`wf-runtime` / `wf-api` / `wf-agent` / `wf-types`）现状，给出 wf-cli 的两种新形态设计：
>
> 1. **mini 模式**：轻量会话式交互（inline split-footer），对齐 opencode `--mini`；
> 2. **无头模式（非交互会话）**：单次 prompt 执行后退出（stdout 输出），对齐 opencode `run` 非交互形态。
>
> 本文档是 01-04 的**补充设计**，不推翻既有决策；三、四、五节分别回答"opencode 技术栈如何映射到 Rust"、
> "mini 模式怎么设计"、"无头模式怎么设计"。

---

## 一、背景与目标

### 1.1 opencode 的交互分层（参考基准）

opencode CLI 提供三档交互形态，按进入路径由轻到重：

| 形态 | 触发 | 渲染 | 体验特征 |
| :--- | :--- | :--- | :--- |
| `run` 非交互 | `opencode run "prompt"` / stdin 管道 / stdout 非 TTY | 无界面，stdout 流式打印 | 单次执行，脚本友好 |
| `--mini` 轻量交互 | `opencode --mini` | **inline split-footer**（normal screen 底部输入 + 上方滚动区） | 不占全屏、退出后对话留在终端 scrollback |
| 完整 TUI | `opencode`（默认） | inline 渲染 + 多面板（OpenTUI） | 全功能交互 |

opencode 与 codex 的 TUI 均为 **inline 渲染**（不切 alt-screen），对话历史写入终端 scrollback，退出后可用终端原生滚动/搜索查看——这是其"轻量"体验的核心。

### 1.2 wf-cli 现状与缺口

- `docs/cli/01` 已定义两种形态：Headless CLI（子命令面）与全屏 alt-screen TUI，未实现（`wf-cli` crate 尚不存在，workspace 仅依赖 clap）。
- 缺口 1：**缺少轻量交互形态**——全屏 TUI 的 8 屏导航 + 模态框栈对"只想跑一次 agent 会话"的场景过重。
- 缺口 2：**缺少非交互会话形态**——现有 headless 是资产管理命令面（CRUD），没有"`wf run "prompt"` 式单次会话执行、流式输出到 stdout"的形态。

### 1.3 引入目标

1. 提供三种清晰形态：**无头会话（run）/ mini 会话 / 全屏 TUI**，共享同一领域调用层（对齐 01 文档"TUI 只是交互外壳"）。
2. mini 模式作为**默认 TTY 交互**的候选与全屏 TUI 的可选入口，降低日常会话成本。
3. 明确 opencode mini 技术栈（OpenTUI/SolidJS 组件体系）在 Rust/ratatui 生态中的对应物，避免重复造轮子。

---

## 二、模式矩阵与判定

### 2.1 形态矩阵

| 模式 | 触发条件 | 渲染 | 适用场景 |
| :--- | :--- | :--- | :--- |
| **无头会话**（run，非交互） | `wf run "<prompt>"`；stdin 管道；stdout 非 TTY；显式 `--no-tui` | stdout 文本 / JSON / silent | 单次执行、CI、脚本、管道 |
| **无头管理命令面**（既有设计） | 显式子命令（`wf workflow list` 等） | 表格 / JSON / silent | 资产 CRUD、运维 |
| **mini 会话**（新） | TTY 且显式 `--mini`（推荐）；或 TTY 默认（见 2.3） | **inline split-footer** | 日常轻量 agent 会话、workflow 前台执行 |
| **全屏 TUI**（既有设计 01-04） | `--tui`；或 TTY 默认 | alt-screen 8 屏 | 完整管理 + 交互 |

### 2.2 判定顺序（`wf` 入口）

```
显式 --tui        → 全屏 TUI
显式 --mini       → mini 会话
有子命令参数       → 无头管理命令面（既有）
无参数：
  stdout 非 TTY    → 无头会话（读 stdin 全文为 prompt，无则进命令面 help）
  stdout 是 TTY    → mini 会话（或全屏 TUI，见 2.3）
```

### 2.3 默认交互形态决策（二选一，建议 A）

- **方案 A（推荐，对齐 opencode）**：TTY 默认进**全屏 TUI**，`--mini` 显式进入轻量形态。优点：与 01 文档既有决策一致、语义清晰。
- **方案 B**：TTY 默认进 **mini**，`--tui` 显式进全屏。优点：轻量优先、实现更快（mini 是 TUI 的子集）。缺点：改变 01 文档默认决策。
- 折中：**CLI 阶段先行实现 mini（方案 B 起点），全屏 TUI 就绪后切回方案 A**。本文档按"mini 与 TUI 共享组件、入口可配置"设计，默认行为最终由 config 的 `cli.default_mode` 决定。

---

## 三、opencode mini 技术栈在 Rust 中的对应

### 3.1 渲染底座

| opencode mini | Rust 对应 | 说明 |
| :--- | :--- | :--- |
| OpenTUI `CliRenderer`，`screenMode: "split-footer"`，**inline 渲染**（不切 alt-screen），30/60 FPS 逐格 diff | **ratatui 0.30 `Terminal` + `Viewport::Inline(n)`**（normal screen 底部渲染 n 行视口，溢出自动滚入终端 scrollback）+ 自研 split-footer 布局 | `Viewport::Inline` 即 ratatui 对 inline 渲染的官方支持，天然把历史写入 scrollback；`Terminal::draw` 自带双缓冲 diff，对应 OpenTUI 的 diff 输出 |
| `externalOutputMode: "capture-stdout"`（捕获外部 stdout 同一帧内渲染） | crossterm 管道捕获 stdout → **ansi-to-tui** 解析为 `Text` 入 scrollback | 对齐 03 文档 2.3 ANSI 解析方案 |
| `consoleMode: "disabled"`（禁控制台写屏） | stderr 抑制（`TerminalStderrGuard`，03 文档 2.1 已有） | 对齐 |
| 关闭时 `screenMode` 切回 main-screen、exit splash | 退出时禁用 inline 视口，终端回归正常 | 退出后历史保留于 scrollback |

> **与 02 文档"不采用 codex inline 视口"的关系**：02 文档的决策对象是**全屏 TUI**（alt-screen 语义下 scroll region 写 scrollback 无意义）。mini 模式是**独立形态**，采用 inline 渲染正是其价值所在（轻量、退出后内容可回溯）。两者不冲突：全屏 TUI 保持 alt-screen，mini 用 `Viewport::Inline`。

### 3.2 组件对应

| OpenTUI 组件 | Rust 对应 | 说明 |
| :--- | :--- | :--- |
| `TextareaRenderable`（多行、word wrap）+ **extmarks**（mention part 区域标记） | P0 单行自研 Input（03 文档 4.2）；P1 `ratatui-textarea` / `tui-textarea-2`；extmark 无直接对应，用**自研区间高亮**（维护 `Vec<(Range, MentionKind)>`，绘制时对 @file 文本着色） | mention 标记简化实现，不做虚拟文本 |
| `MarkdownRenderable`（`streaming: true`，**按 top-level block 增量提交**，未完结 block 不固化） | pulldown-cmark 分词 + **自研 streaming markdown 渲染器**（按段落/列表/代码块粒度增量追加到 scrollback；表格列分类对齐 03 文档 2.3） | 这是 mini 模式最核心的自研件 |
| `CodeRenderable`（`streaming: true` + tree-sitter 高亮，settle 后提交） | **syntect**（P2）+ 自研 streaming code 渲染（语法解析稳定后一次性提交整块） | 对齐 03 文档 2.3 语法高亮（含超限保护） |
| retained surface / `ScrollbackStream`（滚动保留面） | 自研 `Vec<HistoryLine>`（03 文档 7 已有：`display_lines(width)` / `desired_height(width)`） | 已设计 |
| `ScrollBoxRenderable` | ratatui `Paragraph::scroll` | 视口内滚动 |
| `RunFooterMenu`（8 行滚动列表、grouped 分组） | 自研 SelectList（03 文档 4.1） | 已设计 |

### 3.3 架构与数据流对应

| opencode mini | Rust 对应 | 说明 |
| :--- | :--- | :--- |
| SDK 事件 → `session-data` reducer（纯函数）→ `StreamCommit[]` + `FooterOutput` | `AgentStreamEvent` / `ExecutionEvent` → **mini reducer**（纯函数：事件归约为 `MiniCommit[]` + `FooterState`） | 见 3.4 映射表 |
| `RunFooter` 组件（动态高度 `applyHeight`、视图路由、statusline） | `Footer` 结构（路由枚举 + `apply_height()` + statusline 组件） | 见四、 |
| SolidJS signals / effects（响应式） | UiState + 事件驱动（03 文档七已有） | 对齐 |
| `append` 微任务队列合并（连续同 part progress 合并） | tokio mpsc 接收端批量 drain + 帧调度合并 | 对齐 FrameRequester |
| 30/60 FPS 帧调度 | FrameRequester（120FPS 上限，03 文档 3.2 已有） | 对齐 |
| entry group（按 partID 分组、分隔行、spacer） | commit 分组键：`execution_id + iteration + tool_call_id` | 流式增量合并依据 |
| 主题：终端调色板推导 + PALETTE/THEME_MODE/OSC/SIGUSR2 热更新 | OSC 10/11 探测（03 文档六已有）+ 主题事件热更新 | 已设计 |
| 两按退出（Ctrl-C 计数，5s 窗口） | SIGINT 计数状态机（crossterm） | 新增 |
| turn summary `▣ agent · model · duration` | 执行摘要行：`▣ exec_id · type · iterations · duration` | 对齐 |
| `--demo` 合成事件模式 | 合成 `AgentStreamEvent` 序列驱动完整管线 | 新增 |
| 会话重放（resume / resize 重建 scrollback） | 从存储重放执行事件/会话消息重建 scrollback | 新增 |

### 3.4 领域事件 → UI commit 映射（mini 核心对接点）

| `AgentStreamEvent` / `ExecutionEvent` | mini UI 元素 |
| :--- | :--- |
| `IterationStart { iteration }` | 状态栏 iteration 更新 |
| `LlmDelta { content }` | assistant 文本增量 → 流式 markdown 渲染 |
| `ToolStart { tool_name }` | 工具行 `▲ name` |
| `ToolEnd { tool_name, success, duration_ms }` | 工具完成行 `✓ name (ms)` / `✗ name`（失败着色） |
| `IterationEnd` | 迭代边界（滚动区缓冲提交点） |
| `Completed { result }` | turn summary + 状态行回落 Idle |
| `Failed { error }` / `Interrupted` | 错误/中断行 + 状态行 |
| `ExecutionEvent::StateChanged` | 阶段图标（○/▶/⏸）驱动 |
| `ExecutionEvent::ToolExecuted` | 工具摘要行（执行树过滤） |
| `ExecutionEvent::ErrorOccurred` | 错误行（stderr 风格着色） |

审批与追问（领域层已有）：

- `wf-runtime::approval_tool`（approve_changes，approval policy `llm`/`manual`）→ mini 下走**审批 footer 视图**（对齐 codex approval 键位：`y` 本次 / `a` 会话 / `d` 拒绝 / `n`+`Esc` 拒绝本次 / `c` 取消）；无头下按 5.3 策略。
- `wf-api::agent::agent_user_interaction::UserInteractionHandler` → mini 下走**问题 footer 视图**（单选/多选/自定义）；无头下拒绝或取预设。

---

## 四、mini 模式设计

### 4.1 CLI 触发与选项

```text
wf --mini [--session <id>] [--resume] [--demo] [-p <prompt>] [--agent <name>]
```

- `--mini` 要求 stdout 是 TTY；stdin 非 TTY 时打开 `/dev/tty`（对齐 opencode `runtime.stdin.ts`，失败报错退出）。
- 无头管理命令面选项（`--output` 等）与 mini 互斥校验。

### 4.2 屏幕布局（split-footer）

```
┌────────────────────────────────────────────┐
│ Scrollback（normal screen，inline 渲染）    │
│  · splash（logo + session 信息）            │
│  · 用户消息  › ...                          │
│  · assistant markdown 流式渲染             │
│  · tool 行（▲ / ✓ / ✗）                    │
│  · ▣ exec · iterations · duration（摘要行）  │
├────────────────────────────────────────────┤
│ Footer（动态高度，Viewport::Inline(n)）      │
│  · 主区：composer / 面板 / 审批 / 问题       │
│  · 补全 popup（@ / /）                     │
│  · statusline（1 行）                      │
└────────────────────────────────────────────┘
```

- **渲染**：ratatui `Terminal` + `Viewport::Inline(footer_height)`；footer 高度变化时重建视口；scrollback 由终端滚动缓冲承载（溢出行自然离开视口）。
- **scrollback 数据**：`Vec<HistoryLine>`（03 文档 7 已有），提交型行 + 在途 streaming 行并存；**宽度变化 reflow**（`display_lines(width)` 重算，对齐 02 文档五 3）。

### 4.3 Footer 动态高度（对齐 opencode `applyHeight`）

| 主区视图 | 高度 |
| :--- | :--- |
| composer（单行，P0） | base + 1 |
| composer（多行，P1） | base + clamp(rows, 1, 6) |
| 面板（模型/技能/排队） | base + 16（列表 10 + 边框 6） |
| 审批视图 | base + 12 |
| 问题视图 | base + 14 |
| 子代理监视器（P2，后台执行监视） | base + 12 |

- `base` = statusline 1 行 + 装饰；仅高度变化时重建视口（对齐 opencode 减少重绘）。
- 宽度响应式（对齐 opencode `footerWidthPolicy` 与 02 文档状态行回落）：<80 列隐藏可选项；statusline 内容用 ANSI 感知截断。

### 4.4 Footer 子视图

**4.4.1 Composer**

- P0：自研单行 Input（03 文档 4.2）；P1：`ratatui-textarea` 多行（≤6 行）。
- 历史：↑/↓（P0，100 条，对齐 03 文档 4.2）；`Ctrl+R` 反向搜索（P1）。
- `@` mention：**文件**（find_files + 行号 `#10-20`）、**技能**（SkillLoader）、**工作流**（workflow registry）；选中区间高亮（3.2 的简化 extmark）。
- `/` 命令 palette：`/new /resume /model /skills /workflows /executions /quit`（对齐 04 文档九）。

**4.4.2 面板**（占用 footer 主区，`Clear` 覆盖 composer）

| 面板 | 数据源 | 说明 |
| :--- | :--- | :--- |
| 模型选择 | llm-profile / provider 列表 | 对齐 codex ModelPicker |
| 技能选择 | `SkillLoader` | 选择即 `/技能名` |
| 排队管理 | mini 排队队列 | `Enter`/`ctrl+e` 编辑、`Delete`/`ctrl+d` 删除 |
| 后台执行监视（P2） | 执行事件流 | 对齐 opencode 子代理监视器（wf 无 subagent 概念，映射为后台 executions） |

**4.4.3 审批与问题**：见 3.4；阶段感知：`Streaming` 期间 type-ahead 缓冲，`Approval` 期间键盘归审批视图。

**4.4.4 Statusline（1 行，宽度响应式）**

```
[○/▶/⏸] [BUILD]  wf agent · iter:3 · msgs:42    ▣ exec-9f21 · 2.1s    <80 列隐藏右侧
```

- 左：阶段图标 + 模式标签；中：当前 exec 信息（type/id/iteration）；右：执行摘要/模型（≥120 列显示）。

### 4.5 领域集成

- **会话串行**：一次一个 turn；活跃时新 prompt 入排队队列（对齐 opencode 串行 turn），队列管理见 4.4.2。
- **中断/退出**：`Ctrl+C` 两按（5s 窗口）：第一次清空 composer / 提示"again to interrupt"，第二次中断 turn；composer 空时第二次退出。退出打印 exit 提示（`wf --mini --session <id>` 续跑命令）。
- **重放**：`--resume`/`--replay` 时从存储重放执行事件与消息重建 scrollback（对齐 opencode `session-replay`）；resize 后按新宽度 reflow。
- **外部程序**：`/editor` 走 `with_restored`（03 文档 2.1 已有）——暂停 inline 视口 → 恢复终端 → 运行 `$EDITOR` → 重绘。

### 4.6 与全屏 TUI 的共享与差异

**共享**（组件/机制下沉到公共层，两形态复用）：Keymap（上下文回退，04 文档）、UiState/phase、HistoryLine、主题（OSC 探测 + 热更新）、`with_restored`、stderr 抑制、FrameRequester、审批/问题 handler、Command Palette、Output 路由（`CLIError`/退出码）。

**差异**：

| 维度 | mini | 全屏 TUI（01-04） |
| :--- | :--- | :--- |
| 渲染 | `Viewport::Inline`（normal screen） | `Viewport::Fullscreen`（alt-screen） |
| 导航 | 单屏（无屏幕栈） | 8 屏 + 屏幕栈 |
| 覆盖层 | footer 面板（不占屏栈） | 模态框栈（Modal trait + oneshot） |
| 列表驱动 | 无（面板内列表） | list/detail drill-down |
| 领域面 | 会话聚焦（agent + workflow 前台） | 全领域（含资产浏览/设置） |

---

## 五、无头模式（非交互会话）设计

### 5.1 触发与形态

```text
wf run "<prompt>" [--agent <name>] [--model <profile>] [--session <id>] [--output text|json|silent]
```

- **stdin 管道**：stdin 非 TTY 时读取全文作为 prompt（对齐 opencode：`stdin.isTTY ? undefined : await Bun.stdin.text()`）。
- 显式 `--no-tui`：TTY 下强制无头（与 `run` 子命令等价）。
- 无头模式下 `foreground/background` 执行模式自动降级为 `blocking`（对齐 01 文档 4.4"模式降级"）。

### 5.2 输出路由

| 输出 | 内容 |
| :--- | :--- |
| `text` | LLM 文本**流式打印到 stdout**；工具摘要行（`▲ name` / `✓ name (ms)`）打印到 **stderr**（不污染主输出）；结束打印 `▣ exec_id · iterations · duration` 摘要行 |
| `json` | 01 文档 4.4 信封 `{success, type, entity, data, message, timestamp}`，`data` 含完整消息记录（含每轮消息、工具调用）；流式场景下 `data` 为最终执行结果 |
| `silent` | 无输出，退出码表达结果 |

- 流式打印以"行"为粒度（LLM delta 按换行/固定缓冲合并后输出，避免逐 token 系统调用）。

### 5.3 审批与追问策略（非交互）

| 场景 | 策略 | 说明 |
| :--- | :--- | :--- |
| 敏感工具审批（approve_changes 等） | **默认拒绝**（deny）并打印拒绝原因 | 对齐 opencode 非交互 deny 语义 |
| 低危工具 | 允许 | 白名单（config 配置） |
| 预授权 | `--approve-prefix <cmd>` / config `approval` 规则 | 可选，明确授权时放行 |
| 追问（UserInteractionHandler） | 无 TTY 无法交互 → **拒绝该工具调用**并报错退出（exit code 1） | 或 config 预设答案 |

### 5.4 退出码

对齐 01 文档 4.4：0 成功、1 业务失败（agent Failed）、2 参数错误、3 配置错误、4 执行中被打断（SIGINT）。

### 5.5 与既有 headless 管理命令面的关系

- `wf run` 是**会话式单次执行**（对话闭环），管理命令面是**资产操作**（CRUD/查询）——两者并列，共享 Adapter 层与 Output 路由。
- `wf agent run --stream`（01 文档 4.2 已列）即本设计"无头会话"在 agent 领域的落地，二者合并为同一实现（`agent run` 无 TTY 时自动走无头输出）。

---

## 六、实施阶段（与 01 文档 CLI-1~4 对齐）

| 阶段 | 内容 | 依赖 |
| :--- | :--- | :--- |
| **CLI-A** | `wf-cli` crate 骨架 + 模式判定（2.2）+ `wf run` 无头会话（流式 stdout + 输出路由 + 审批降级 + 退出码） | clap、wf-api、wf-runtime（已有） |
| **CLI-B** | mini 渲染底座（ratatui `Viewport::Inline` + split-footer + 状态栏）+ composer（单行）+ scrollback（HistoryLine + 流式 markdown 增量） | ratatui 0.30、crossterm、pulldown-cmark；自研 streaming markdown |
| **CLI-C** | mini 面板（模型/技能/排队）+ 审批/问题视图 + 历史/`@`mention + 两按退出 + turn summary + `--resume` 重放 + `--demo` | 对齐 3.4 事件映射 |
| **CLI-D** | 全屏 TUI（01-04 设计落地），与 mini 共享公共层；`--tui`/`--mini` 入口分流 | 复用 CLI-B/C 组件 |

## 七、风险与决策记录

| 项 | 决策 | 理由 |
| :--- | :--- | :--- |
| mini 渲染模式 | **inline（`Viewport::Inline`）**，与全屏 TUI 的 alt-screen 并存 | 轻量体验 = 退出后内容留终端 scrollback；两形态语义不同，不冲突 |
| 默认交互入口 | 推荐方案 A（`--mini` 显式）；CLI 阶段可暂以 mini 为默认 | 保持 01 文档决策一致性，具体由 config `cli.default_mode` 收敛 |
| streaming markdown | **自研**按 top-level block 增量提交（不用现成 markdown widget） | 对齐 opencode 行为：未完结 block 不固化、避免闪烁；是 mini 核心件 |
| mention 区域标记 | **简化区间高亮**（不做 extmark 虚拟文本） | extmark 无 ratatui 对应，完整虚拟文本成本高 |
| 多行输入 | P0 单行自研，P1 外购 `ratatui-textarea` | 对齐 03 文档决策（外购优先） |
| 无头审批 | 默认 deny + 白名单 + `--approve-prefix` 预授权 | 对齐 opencode 非交互语义，安全默认 |
| 事件粒度 | `LlmDelta` 按 token 流 → 帧调度合并后渲染 | 对齐 03 文档 3.2 FrameRequester 限帧，防逐 token 重绘 |
| 执行事件统一 | agent 与 workflow 统一为 `ExecutionType`（无头与 mini 的摘要行/状态栏同一套） | 对齐 01 文档"执行跟踪统一"概念模型 |

---

## 附：opencode mini 关键常量与 wf-cli 建议对照

| opencode mini 常量 | 值 | wf-cli 建议 |
| :--- | :--- | :--- |
| `targetFps` / `maxFps` | 30 / 60 | FrameRequester 120FPS（既有设计，渲染帧实际 30-60 即可） |
| `FOOTER_HEIGHT`（初始） | 4 | 4 |
| `TEXTAREA_MIN/MAX_ROWS` | 1 / 6 | 1 / 6 |
| `PANEL_LIST_ROWS` | 10 | 10 |
| `PERMISSION_ROWS` / `QUESTION_ROWS` | 12 / 14 | 12 / 14 |
| 宽度断点 | 80 / 66 / 120 / 150 | 80 / 120（对齐 02 文档窄终端回落） |
| 历史上限 | 200 | 100（03 文档 4.2） |
| 两按确认窗口 | 5s | 5s |
| resize 防抖 | 250ms | 75ms（02 文档五 3，既有决策） |
| 退出提示 | `opencode --mini -s <id>` | `wf --mini --session <id>` |
