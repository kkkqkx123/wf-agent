# wf-cli 编译目标拆分设计方案

## 一、背景与动机

当前 `wf-cli` 是一个单体 crate，同时包含：
- 完整 TUI（ratatui + crossterm + 大量 UI 组件）
- 无头模式（headless run）
- 管理子命令（workflow、execution、llm-profile 等）

当用户只需要无头模式时，仍需拉入 ratatui、unicode-width、pulldown-cmark 等 TUI 依赖，导致：
1. 编译时间长（ratatui 及其依赖树较大）
2. 二进制体积膨胀
3. 无法在无终端环境下（CI/容器）独立分发轻量二进制

atomcode 项目已经实现了类似的拆分：`atomcode-cli`（无头/入口）+ `atomcode-tuix`（TUI 库）+ `atomcode-clix`（D 层独立驱动）。

### 目标

将 `wf-cli` 拆分为三个独立编译目标：

| 编译目标 | 二进制名 | 用途 | TUI 依赖 |
|---------|---------|------|---------|
| 无头模式 | `wf-headless` | CI/管道/脚本/容器 | 无 |
| 精简 TUI | `wf-mini` | 轻量交互（无 alt-screen） | crossterm only |
| 完整 TUI | `wf` | 全功能交互界面 | ratatui + crossterm |

## 二、当前架构分析

### 2.1 当前依赖关系

```
wf-cli (binary: wf)
├── ratatui 0.30        ← TUI 框架（重依赖）
├── crossterm 0.29      ← 终端事件/控制
├── pulldown-cmark      ← Markdown 渲染
├── unicode-width/segmentation
├── clap 4              ← CLI 参数解析
├── tokio               ← 异步运行时
├── reqwest             ← HTTP 客户端
├── wf-api              ← 应用层 API
├── wf-runtime          ← 运行时（embedded feature）
└── wf-common/wf-types/wf-core
```

### 2.2 当前模块职责

**无头模式所需模块：**
- `args.rs` — CLI 参数定义
- `mode.rs` — 模式解析（TTY 检测）
- `run.rs` — 无头会话驱动
- `output.rs` — 输出格式化（text/json/jsonl）
- `domain.rs` — 运行时适配器（embedded + remote）
- `remote.rs` — 远程服务器客户端
- `error.rs` — 错误类型
- `cmd/*` — 管理子命令

**Mini TUI 额外需要：**
- `terminal.rs` — 终端控制（TerminalGuard, CrosstermControl）
- `theme.rs` — 主题检测
- `interactive.rs` — 交互式会话控制器（简化版）
- `reducer.rs` — 会话状态管理
- `transcript.rs` — 消息历史渲染
- `sigint.rs` — 信号处理
- `ansi.rs` — ANSI 解析

**完整 TUI 额外需要：**
- `tui.rs` — 全屏事件循环
- `screens.rs` — 多屏数据模型
- `screen_draw.rs` — 屏幕绘制
- `modal.rs` — 模态对话框系统
- `animation.rs` — 动画控制器
- `footer.rs` / `bottom_pane.rs` / `panels.rs` — UI 组件
- `markdown.rs` — Markdown 渲染
- `history_cell.rs` — 历史行渲染
- `approval_overlay.rs` / `question_overlay.rs` — 覆盖层
- `keymap.rs` / `motion.rs` — 键盘映射/滚动
- `select.rs` — 选择列表
- `overlay.rs` — 覆盖模式
- `framer.rs` — 帧率限制

## 三、设计方案

### 3.1 Crate 结构

```
crates/app/
├── wf-cli-shared/       ← 公共库 crate（新增）
│   └── src/
│       ├── lib.rs
│       ├── args.rs      ← 从 wf-cli 迁移
│       ├── mode.rs      ← 从 wf-cli 迁移
│       ├── error.rs     ← 从 wf-cli 迁移
│       ├── output.rs    ← 从 wf-cli 迁移
│       ├── run.rs       ← 从 wf-cli 迁移
│       ├── domain.rs    ← 从 wf-cli 迁移
│       ├── remote.rs    ← 从 wf-cli 迁移
│       ├── config.rs    ← 从 wf-cli 迁移
│       ├── app_config.rs← 从 wf-cli 迁移
│       ├── sanitize.rs  ← 从 wf-cli 迁移
│       ├── ansi.rs      ← 从 wf-cli 迁移
│       ├── mini/        ← Mini TUI 核心（新增）
│       │   ├── mod.rs
│       │   ├── renderer.rs   ← crossterm 渲染器
│       │   ├── event_loop.rs ← 输入+渲染循环
│       │   ├── buffer.rs     ← 滚动缓冲区
│       │   ├── input.rs      ← 输入处理（借鉴 atomcode）
│       │   ├── markdown.rs   ← 简化 Markdown
│       │   └── render_line.rs← 语义行类型
│       └── cmd/         ← 管理子命令（从 wf-cli 迁移）
│
├── wf-cli/              ← 原有 crate（改造为薄壳）
│   └── src/
│       ├── main.rs      ← 完整 TUI 入口
│       └── lib.rs       ← 重新导出 wf-cli-shared
│
├── wf-headless/         ← 无头模式二进制（新增）
│   └── src/
│       └── main.rs
│
└── wf-mini/             ← 精简 TUI 二进制（新增）
    └── src/
        └── main.rs
```

### 3.2 依赖关系 DAG

```
wf-cli-shared (library)
├── clap
├── tokio
├── serde/serde_json/toml
├── reqwest
├── wf-api
├── wf-runtime (optional: embedded)
└── wf-common/wf-types/wf-core

wf-headless (binary)
├── wf-cli-shared
└── tokio

wf-mini (binary)
├── wf-cli-shared
├── crossterm        ← 仅终端控制，无 ratatui
└── tokio

wf (binary, 原 wf-cli)
├── wf-cli-shared
├── crossterm
├── ratatui          ← 完整 TUI
├── pulldown-cmark
├── unicode-width/segmentation
└── tokio
```

### 3.3 wf-cli-shared 的 Feature 设计

```toml
[features]
default = ["embedded"]
embedded = ["dep:wf-runtime"]
remote = ["reqwest/json", "reqwest/stream"]
```

共享 crate 不引入任何 UI 相关依赖，仅包含业务逻辑和输出格式化。

### 3.4 各编译目标详解

#### 3.4.1 wf-headless（无头模式）

**功能范围：**
- `wf run "prompt"` — 无头会话
- 所有管理子命令（workflow, execution, llm-profile, skill, ...）
- stdin 管道输入
- JSON/JSONL/Text 输出格式
- 远程服务器模式

**零 UI 依赖：** 不依赖 crossterm 或 ratatui。

**入口文件 `wf-headless/src/main.rs`：**

```rust
use clap::Parser;
use wf_cli_shared::{Cli, run_headless_only};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(err) = run_headless_only(cli).await {
        eprintln!("wf-headless: {err}");
        std::process::exit(i32::from(err.exit_code()));
    }
}
```

**`wf-cli-shared` 新增导出函数：**

```rust
/// Headless-only entry point: routes subcommands and headless sessions.
/// Returns error if TUI mode is requested.
pub async fn run_headless_only(cli: Cli) -> CliResult<()> {
    // 现有 run() 中的子命令分发逻辑
    // + 强制 CliMode::Run
    // + 禁止 --tui 标志
}
```

#### 3.4.2 wf-mini（精简 TUI）

**设计原则：** 最小化依赖，借鉴 atomcode 的轻量级渲染模式。

**功能范围：**
- 交互式 agent 会话（流式输出）
- 终端 raw mode + 基本输入处理
- 单屏滚动历史（无 alt-screen 切换）
- Ctrl-C / Ctrl-Z 信号处理
- 主题检测（深色/浅色）
- 工具审批提示（行内文本选择）

**不包含（相比完整 TUI 的简化）：**
- 多屏导航（Workflow/Executions/Checkpoints 等）
- 模态对话框系统
- Sidebar overlay
- Markdown 富文本渲染（简化为纯文本 + 基本格式化）
- 动画/帧率限制（无 spinner 动画，使用静态提示符）
- 语法高亮
- 鼠标支持

**依赖：** 仅 crossterm，不依赖 ratatui。

**核心组件架构（借鉴 atomcode）：**

```
wf-cli-shared/src/mini/
├── mod.rs              ← pub async fn run() 入口
├── renderer.rs         ← MiniRenderer (借鉴 atomcode PlainRenderer)
├── event_loop.rs       ← MiniEventLoop (借鉴 atomcode run_loop)
├── buffer.rs           ← ScrollBuffer (借鉴 atomcode Screen 滚动)
├── input/
│   ├── mod.rs          ← InputEvent enum (借鉴 atomcode InputEvent)
│   ├── reader.rs       ← 专用线程读取 crossterm 事件
│   └── key_action.rs   ← 按键分类 (借鉴 atomcode Action)
├── markdown.rs         ← 简化行级 Markdown (借鉴 atomcode MdState)
├── render_line.rs      ← UiLine 语义行类型 (借鉴 atomcode UiLine)
├── terminal.rs         ← 终端能力探测 (借鉴 atomcode TerminalCaps)
├── theme.rs            ← 主题检测 (复用现有 wf-cli theme.rs)
└── sanitize.rs         ← ANSI 清理 (借鉴 atomcode scrub_controls)
```

**借鉴 atomcode 的关键设计：**

| 组件 | atomcode 实现 | wf-mini 借鉴方式 |
|-----|-------------|----------------|
| 渲染器 | `PlainRenderer` (printf 风格) | 实现 `MiniRenderer`，直接 writeln + SGR |
| 输入读取 | 专用 OS 线程 + mpsc channel | 复用模式，简化粘贴处理 |
| 按键分类 | `Action` enum (readline 风格) | 借鉴 Ctrl+U/W/K/A/E 绑定 |
| 语义行 | `UiLine` enum (40+ variants) | 简化为 5-6 个核心 variant |
| Markdown | `MdState` 行级状态机 | 简化：仅处理代码块和粗体/斜体 |
| 终端探测 | `TerminalCaps` 结构体 | 简化为必需的 5-6 个字段 |
| ANSI 清理 | `scrub_controls()` | 直接复用设计 |
| 信号恢复 | async-signal-safe restore | 借鉴 signal_restore.rs 模式 |

**简化 Markdown 渲染策略：**

atomcode 的 `markdown.rs` 有 3115 行，包含完整的 fenced code block、table、list 处理。wf-mini 只需：

```rust
// wf-mini markdown.rs: ~200 行
pub struct MiniMdState {
    in_code_block: bool,
    fence_char: char,
    fence_len: usize,
}

impl MiniMdState {
    /// 处理一行 Markdown，返回格式化后的文本
    /// 简化策略：
    /// - fenced code block: 保留原文，添加缩进
    /// - **bold** → ANSI bold
    /// - *italic* → ANSI italic
    /// - `code` → ANSI dim
    /// - 链接 [text](url) → text
    /// - 其他: 原样输出
    pub fn render_line(&mut self, line: &str, width: usize) -> RenderedLine { ... }
}
```

**简化动画策略：**

atomcode 有完整的 `AnimationController` + spinner 动画。wf-mini 使用：
- 静态提示符 `>` 替代动画 spinner
- 流式输出时在行尾显示 `...` 闪烁（通过光标可见/隐藏切换，无需帧循环）
- 工具调用显示静态 `[tool] tool_name` 前缀

**Mini TUI 布局：**

```
┌─────────────────────────────────────┐
│                                     │
│  History (scrollable buffer)        │  ← 滚动区域（占满剩余空间）
│  - User: 原样显示                   │
│  - Assistant: 简化 Markdown         │
│  - Tool: [tool] name → summary      │
│  - Error: ANSI red 前缀             │
│                                     │
├─────────────────────────────────────┤
│ model-name | tokens | status       │  ← 状态行（固定底部）
│ > 用户输入行 (_cursor)              │  ← 输入行（固定底部）
└─────────────────────────────────────┘
```

**布局要点：**
- 状态行和输入框绑定在底部，始终可见，不随历史滚动
- 历史区域占满上方所有空间，自动滚动到最新消息
- 状态行显示当前模型、token 用量、审批状态等上下文信息

**`MiniRenderer` 核心接口：**

```rust
pub trait MiniRenderer {
    fn init(&mut self) -> io::Result<()>;
    fn render_history(&mut self, lines: &[HistoryLine]) -> io::Result<()>;
    fn render_input(&mut self, prompt: &str, cursor: usize) -> io::Result<()>;
    fn render_status(&mut self, status: &StatusInfo) -> io::Result<()>;
    fn restore(&mut self) -> io::Result<()>;
}
```

#### 3.4.3 wf（完整 TUI，现有）

保持现有功能不变，重构为从 `wf-cli-shared` 导入公共模块。

### 3.5 Cargo.toml 配置

#### wf-cli-shared/Cargo.toml

```toml
[package]
name = "wf-cli-shared"
version.workspace = true
edition.workspace = true
description = "Shared logic for wf-cli targets (headless, mini, full TUI)"

[dependencies]
clap.workspace = true
async-trait.workspace = true
serde.workspace = true
serde_json.workspace = true
toml.workspace = true
chrono.workspace = true
tokio.workspace = true
tracing.workspace = true
thiserror.workspace = true
reqwest.workspace = true
eventsource-stream.workspace = true
tokio-util.workspace = true
regex.workspace = true
globset.workspace = true
futures.workspace = true
wf-common = { path = "../../foundation/wf-common" }
wf-types = { path = "../../foundation/wf-types" }
wf-core = { path = "../../foundation/wf-core" }
wf-api = { path = "../wf-api" }
wf-runtime = { path = "../wf-runtime", optional = true }

[features]
default = ["embedded"]
embedded = ["dep:wf-runtime"]
remote = ["reqwest/json", "reqwest/stream"]
lua = ["wf-runtime/lua"]
native = ["wf-runtime/native"]
```

#### wf-headless/Cargo.toml

```toml
[package]
name = "wf-headless"
version.workspace = true
edition.workspace = true
description = "wf-agent headless CLI: agent sessions and management commands without TUI"

[dependencies]
clap.workspace = true
tokio.workspace = true
wf-cli-shared = { path = "../wf-cli-shared" }

[features]
default = ["embedded"]
embedded = ["wf-cli-shared/embedded"]
remote = ["wf-cli-shared/remote"]

[[bin]]
name = "wf-headless"
path = "src/main.rs"
```

#### wf-mini/Cargo.toml

```toml
[package]
name = "wf-mini"
version.workspace = true
edition.workspace = true
description = "wf-agent lightweight TUI: crossterm-based interactive agent without ratatui"

[dependencies]
clap.workspace = true
crossterm.workspace = true
tokio.workspace = true
wf-cli-shared = { path = "../wf-cli-shared" }

[features]
default = ["embedded"]
embedded = ["wf-cli-shared/embedded"]
remote = ["wf-cli-shared/remote"]

[[bin]]
name = "wf-mini"
path = "src/main.rs"
```

#### wf-cli/Cargo.toml（改造后）

```toml
[package]
name = "wf-cli"
version.workspace = true
edition.workspace = true
description = "wf-agent full TUI: complete interactive interface with ratatui"

[dependencies]
crossterm.workspace = true
ratatui.workspace = true
unicode-width.workspace = true
unicode-segmentation.workspace = true
pulldown-cmark.workspace = true
libc.workspace = true
tokio.workspace = true
wf-cli-shared = { path = "../wf-cli-shared" }

[features]
default = ["embedded"]
embedded = ["wf-cli-shared/embedded"]
remote = ["wf-cli-shared/remote"]
lua = ["wf-cli-shared/lua"]
native = ["wf-cli-shared/native"]
diff-record = []

[[bin]]
name = "wf"
path = "src/main.rs"
```

## 四、迁移步骤

### Phase 1：创建 wf-cli-shared crate

1. 创建 `crates/app/wf-cli-shared/` 目录
2. 将以下模块从 `wf-cli` 迁移到 `wf-cli-shared`：
   - `args.rs`, `mode.rs`, `error.rs`, `output.rs`
   - `run.rs`, `domain.rs`, `remote.rs`
   - `config.rs`, `app_config.rs`, `sanitize.rs`, `ansi.rs`
   - `cmd/` 目录（所有管理子命令）
3. 在 `wf-cli-shared/src/lib.rs` 中声明 `pub mod` 和 `pub use`
4. 更新 `wf-cli` 为从 `wf-cli-shared` 重新导出

### Phase 2：创建 wf-headless 二进制

1. 创建 `crates/app/wf-headless/` 目录
2. 实现 `main.rs`：调用 `wf_cli_shared::run_headless_only()`
3. 验证：`cargo build -p wf-headless` 不拉入 crossterm/ratatui
4. 验证：所有管理子命令和 `wf run` 均可用

### Phase 3：创建 wf-mini 二进制

1. 创建 `crates/app/wf-mini/` 目录
2. 在 `wf-cli-shared` 中新增 `mini/` 模块
3. 实现核心组件（参考 `docs/ref/atomcode-mini-borrow-guide.md`）：
   - `renderer.rs` — crossterm 直接渲染
   - `event_loop.rs` — 输入+渲染循环
   - `buffer.rs` — 滚动缓冲区
   - `input/reader.rs` — 专用线程输入
   - `markdown.rs` — 简化 Markdown
4. 实现 `wf-mini/src/main.rs`：调用 `wf_cli_shared::mini::run()`
5. 验证：`cargo build -p wf-mini` 不拉入 ratatui
6. 验证：交互式会话可用，流式输出正常

### Phase 4：更新 wf-cli（完整 TUI）

1. 更新 `wf-cli` 的 `Cargo.toml` 依赖 `wf-cli-shared`
2. 删除已迁移到 `wf-cli-shared` 的模块文件
3. 更新 `lib.rs` 为重新导出 `wf-cli_shared::*` + TUI 专属模块
4. 验证：`cargo build -p wf-cli` 功能不变

### Phase 5：更新 workspace

在根 `Cargo.toml` 中添加新 crate：

```toml
members = [
    # ... existing ...
    "crates/app/wf-cli-shared",
    "crates/app/wf-headless",
    "crates/app/wf-mini",
]
```

## 五、编译产物对比

| 指标 | wf (当前) | wf-headless | wf-mini | wf (拆分后) |
|-----|----------|-------------|---------|------------|
| ratatui 依赖 | ✅ | ❌ | ❌ | ✅ |
| crossterm 依赖 | ✅ | ❌ | ✅ | ✅ |
| pulldown-cmark 依赖 | ✅ | ❌ | ❌ | ✅ |
| 预估编译时间 | 基准 | -40% | -30% | 基准 |
| 预估二进制大小 | 基准 | -35% | -20% | 基准 |
| TUI 功能 | 完整 | 无 | 精简 | 完整 |

## 六、风险与注意事项

1. **模块可见性**：迁移后需确保 `wf-cli-shared` 中的类型对三个二进制均可见（`pub`）
2. **Feature 传播**：embedded/remote/lua/native 需要通过 `wf-cli-shared` 传播到各二进制
3. **测试覆盖**：迁移期间需保持 `cargo test --all` 通过
4. **向后兼容**：`wf` 命令名不变，用户无感知
5. **CI 集成**：可在 CI 中只构建 `wf-headless` 加速流水线

## 七、参考

- atomcode 架构：`atomcode-cli`（入口）+ `atomcode-tuix`（TUI 库）+ `atomcode-clix`（D 层）
- atomcode TUI 借鉴指南：`docs/ref/atomcode-mini-borrow-guide.md`
- 当前 wf-cli 源码：`crates/app/wf-cli/`
- 设计参考文档：`docs/ref/codex-tui-architecture.md`
