# atomcode TUI 设计借鉴指南（wf-mini 专用）

本文档详细列出 atomcode TUI 中可供 wf-mini 借鉴的设计，包括具体文件位置、实现模式和简化策略。

## 一、渲染子系统

### 1.1 PlainRenderer — printf 风格渲染器

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/render/plain.rs`（1220 行）

**核心设计：**
- 不使用 cell buffer，直接 `writeln!` + SGR 颜色码
- 使用 `\r` 实现 spinner 效果（不依赖帧循环）
- 行软换行：按终端宽度折行，跟踪消耗的行数
- 16 色 SGR 调色板，映射 `Role` → ANSI 颜色

**wf-mini 借鉴：**
```
文件: wf-cli-shared/src/mini/renderer.rs
参考: plain.rs 全文
简化: 去掉 image paste、QR code、mascot 渲染
保留: SGR 颜色映射、行软换行、flush 策略
```

**关键函数参考：**
- `PlainRenderer::new()` — 初始化 stdout writer
- `PlainRenderer::render()` — 遍历 `Vec<UiLine>` 逐行渲染
- `PlainRenderer::flush()` — `stdout().flush()`
- Role → SGR 映射表（约 line 400+）

### 1.2 UiLine — 语义行类型

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/render/mod.rs`（1158 行）

**核心设计：**
```rust
pub enum UiLine {
    Welcome { ... },
    User { text: String },
    Assistant { text: String, streaming: bool },
    ToolCall { name: String, args: String },
    ToolOutput { name: String, output: String },
    DiffBlock { ... },
    Error { text: String },
    InputPrompt { ... },
    StatusLine { ... },
    // ... 40+ variants
}
```

**wf-mini 简化：**
```rust
// wf-cli-shared/src/mini/render_line.rs
pub enum MiniLine {
    User { text: String },
    Assistant { text: String },
    Tool { name: String, summary: String },
    Error { text: String },
    System { text: String },
    InputPrompt,
}
```

只保留 5-6 个核心 variant，覆盖交互式会话的主要场景。

### 1.3 off-main-thread 渲染

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/render/worker.rs`（1175 行）

**核心设计：**
- `TaskRenderer` 包装任意 inner renderer
- 专用 OS 线程处理所有终端 I/O
- 事件循环通过 `mpsc::UnboundedSender<UiLine>` 发送渲染命令
- ACK oneshot 用于生命周期命令（pause/resume/shutdown）

**wf-mini 简化：**
- 初期不需要 off-main-thread 渲染（单屏 mini TUI 足够轻量）
- 保留 channel 模式作为未来扩展点
- 事件循环直接调用 renderer（降低复杂度）

## 二、输入子系统

### 2.1 专用线程输入读取

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/input/reader.rs`（1253 行）

**核心设计：**
- 专用 OS 线程阻塞读取 crossterm 事件
- `mpsc::UnboundedSender<InputEvent>` 转发到事件循环
- `ReaderHandle` 拥有生命周期：Pause/Resume/Shutdown
- 粘贴突发检测（PENDING 4ms → ACTIVE 4ms/15ms）
- Modifier+Enter 去重（40ms 窗口过滤 OS 自动重复）

**wf-mini 借鉴：**
```
文件: wf-cli-shared/src/mini/input/reader.rs
参考: reader.rs 的 spawn() 和 run() 函数
简化: 去掉粘贴突发检测、Pause/Resume 机制
保留: 专用线程 + channel 转发、基本事件过滤
```

**关键模式：**
```rust
pub fn spawn() -> (ReaderHandle, mpsc::UnboundedReceiver<InputEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let handle = ReaderHandle { ... };
    std::thread::spawn(move || {
        loop {
            if crossterm::event::poll(Duration::from_millis(100)).unwrap_or(false) {
                if let Ok(event) = crossterm::event::read() {
                    let input_event = convert_event(event);
                    if tx.send(input_event).is_err() { break; }
                }
            }
        }
    });
    (handle, rx)
}
```

### 2.2 按键分类（Action enum）

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/input/key_action.rs`（329 行）

**核心设计：**
```rust
pub enum Action {
    Submit,
    InsertNewline,
    Cancel,
    ClearLine,
    CursorLeft,
    CursorRight,
    CursorHome,
    CursorEnd,
    Backspace,
    Delete,
    // ... readline 风格绑定
}

pub fn classify(key: &KeyEvent) -> Action {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => Action::Submit,
        (KeyCode::Char('a'), KeyModifiers::CONTROL) => Action::CursorHome,
        (KeyCode::Char('e'), KeyModifiers::CONTROL) => Action::CursorEnd,
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => Action::ClearLine,
        (KeyCode::Char('w'), KeyModifiers::CONTROL) => Action::DeleteWordLeft,
        // ...
    }
}
```

**wf-mini 借鉴：**
```
文件: wf-cli-shared/src/mini/input/key_action.rs
参考: key_action.rs 全文
简化: 只保留核心 10-12 个 Action
保留: readline 风格绑定（Ctrl+A/E/U/W/K）、退格键标准化
```

### 2.3 InputEvent 类型

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/input/mod.rs`（58 行）

**核心设计：**
```rust
pub enum InputEvent {
    Key(KeyEvent),
    Paste(String),
    Eof,
    FocusChanged(bool),
    Resize(u16, u16),
    Pointer(PointerEvent),
}
```

**wf-mini 简化：**
```rust
pub enum InputEvent {
    Key(KeyEvent),
    Paste(String),
    Eof,
    Resize(u16, u16),
}
```

去掉 Pointer 和 FocusChanged。

## 三、终端管理

### 3.1 终端能力探测

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/terminal.rs`（1091 行）

**核心设计：**
```rust
pub struct TerminalCaps {
    pub tty: bool,
    pub colors: u16,
    pub spinner: bool,
    pub bracketed_paste: bool,
    pub raw_mode: bool,
    pub scroll_region: bool,
    pub unicode_symbols: bool,
    pub kitty_keyboard: bool,
    pub mouse_sgr: bool,
    pub osc52_clipboard: bool,
    pub tmux_passthrough: bool,
    // ... 更多字段
}
```

通过 `TERM`、`TERM_PROGRAM`、`WT_SESSION`、`KITTY_WINDOW_ID` 等环境变量探测。

**wf-mini 简化：**
```rust
pub struct MiniCaps {
    pub tty: bool,
    pub colors: u16,
    pub bracketed_paste: bool,
    pub unicode_symbols: bool,
}
```

只保留 4 个必需字段，通过环境变量检测。

### 3.2 RAII Terminal Guard

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/lib.rs`（1081 行）

**核心设计：**
```rust
struct TerminalGuard {
    modes: TerminalModes,
    control: CrosstermControl,
}

impl TerminalGuard {
    fn new(control: CrosstermControl) -> Self { ... }
    fn enter(&mut self, modes: TerminalModes) -> io::Result<()> { ... }
    fn restore(&mut self) -> io::Result<()> { ... }
    fn with_restored<F, R>(&mut self, ctx: Option<&str>, f: F) -> io::Result<R>
    where F: FnOnce() -> R { ... }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
```

**wf-mini 借鉴：**
```
文件: wf-cli-shared/src/mini/terminal_guard.rs
参考: lib.rs 的 TerminalGuard 实现
简化: 去掉 with_restored 窗口机制
保留: RAII drop、enter/restore、panic hook
```

### 3.3 async-signal-safe 终端恢复

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/signal_restore.rs`（122 行）

**核心设计：**
```rust
static mut RESTORE_SEQ: Option<Vec<u8>> = None;
static ARMED: AtomicBool = AtomicBool::new(false);

pub unsafe fn arm(restore_seq: &[u8]) {
    if ARMED.swap(true, Ordering::SeqCst) { return; }
    RESTORE_SEQ = Some(restore_seq.to_vec());
    // 安装 SIGTERM/SIGHUP/SIGINT handler
}

extern "C" fn handler(sig: libc::c_int) {
    // 仅使用 async-signal-safe 函数
    if let Some(seq) = unsafe { RESTORE_SEQ.as_ref() } {
        let _ = libc::write(libc::STDERR_FILENO, seq.as_ptr() as *const _, seq.len());
    }
    // tcsetattr 恢复 cooked mode
    // re-raise signal
}
```

**wf-mini 借鉴：**
```
文件: wf-cli-shared/src/mini/signal_restore.rs
参考: signal_restore.rs 全文
直接复用设计，仅调整 restore 序列格式
```

## 四、状态管理

### 4.1 Phase 状态机

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/state.rs`（3678 行）

**核心设计：**
```rust
pub enum UiPhase {
    Idle,
    Streaming,
    Approval,
    UserInput,
    RoundCap,
    Suspended,
}
```

每个 phase 有独立的 key handler 和渲染路径。

**wf-mini 简化：**
```rust
pub enum MiniPhase {
    Idle,
    Streaming,
    Approval,
}
```

只保留 3 个核心 phase。

### 4.2 Grapheme-boundary 文本编辑

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/state.rs`（line 5-41）

**核心设计：**
```rust
pub fn previous_grapheme_boundary(s: &str, pos: usize) -> usize { ... }
pub fn next_grapheme_boundary(s: &str, pos: usize) -> usize { ... }
pub fn insert_at_cursor(s: &mut String, cursor: &mut usize, ch: char) { ... }
pub fn backspace_at_cursor(s: &mut String, cursor: &mut usize) { ... }
pub fn delete_at_cursor(s: &mut String, cursor: &mut usize) { ... }
```

**wf-mini 借鉴：**
```
文件: wf-cli-shared/src/mini/text_edit.rs
参考: state.rs line 5-41
直接复用，作为文本编辑的基础工具函数
```

## 五、Markdown 渲染（简化版）

### 5.1 行级状态机

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/markdown.rs`（3115 行）

**核心设计：**
```rust
pub struct MdState {
    in_code_block: bool,
    fence_char: char,
    fence_len: usize,
    table_buf: Option<String>,
    code_buf: Option<String>,
}

impl MdState {
    pub fn render_line(&mut self, line: &str, width: usize) -> Vec<RenderedSegment> { ... }
}
```

逐行处理，状态机跟踪 code block 上下文。

**wf-mini 简化（~200 行）：**

```rust
pub struct MiniMdState {
    in_code_block: bool,
}

impl MiniMdState {
    pub fn render_line(&mut self, line: &str, width: usize) -> MiniRenderedLine {
        // 简化策略：
        // 1. fenced code block: ``` 开始/结束，内部缩进 2 空格
        // 2. **bold** → SGR bold
        // 3. *italic* → SGR italic
        // 4. `inline code` → SGR dim
        // 5. [text](url) → 只显示 text
        // 6. 长行软换行
        // 7. 其他: 原样输出
    }
}
```

**借鉴的关键点：**
- 行级处理模式（流式友好）
- code block 状态跟踪
- 换行策略

## 六、ANSI 清理

### 6.1 控制字符清理

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/sanitize.rs`（204 行）

**核心设计：**
```rust
/// 严格模式：清理所有控制字符（用于不可信的 LLM 输出）
pub fn scrub_controls(s: &str) -> Cow<str> { ... }

/// SGR 保留模式：只清理非 SGR 的控制字符
pub fn scrub_controls_keep_sgr(s: &str) -> Cow<str> { ... }
```

**wf-mini 借鉴：**
```
文件: wf-cli-shared/src/mini/sanitize.rs
参考: sanitize.rs 全文
直接复用设计，LLM 输出必须经过清理
```

## 七、主题检测

### 7.1 OSC 背景色探测

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/terminal_bg.rs`（381 行）

**核心设计：**
- 打开 `/dev/tty` → 临时 raw mode → 写 OSC 11 查询 → 读响应 → 恢复
- 超时 100ms，失败时降级到缓存/默认主题

**wf-mini 借鉴：**
```
文件: wf-cli-shared/src/mini/theme.rs
参考: terminal_bg.rs 的 detect_light() 函数
简化: 去掉文件缓存，只保留探测 + 默认值
复用: 现有 wf-cli theme.rs 的 OSC 探测逻辑
```

## 八、Unicode 降级

### 8.1 Glyph-to-ASCII 映射

**atomcode 位置：** `ref/atomcode/crates/atomcode-tuix/src/glyph.rs`（161 行）

**核心设计：**
```rust
pub fn downgrade_glyphs(s: &str) -> Cow<str> {
    // ~50 个装饰性 Unicode 字符 → ASCII 替代
    // 返回 Cow::Borrowed 当无需映射（零拷贝）
}

pub fn ascii_for(ch: char) -> Option<&'static str> {
    match ch {
        '─' => Some("--"),
        '│' => Some("|"),
        '┌' => Some("+"),
        // ...
    }
}
```

**wf-mini 借鉴：**
```
文件: wf-cli-shared/src/mini/glyph.rs
参考: glyph.rs 全文
直接复用核心映射表，支持非 Unicode 终端
```

## 九、布局模板

### 9.1 atomcode PlainRenderer 布局

atomcode 的 PlainRenderer 使用 printf 风格：

```
[welcome banner]
[user message]
[assistant message with streaming ...]
[tool call: name → result]
> [input prompt]
```

### 9.2 wf-mini 布局

借鉴 atomcode 的分层思路，状态行和输入框绑定在底部：

```
┌─────────────────────────────────────┐
│                                     │
│ History (滚动缓冲区)                │ ← 可滚动，占满上方空间
│                                     │
├─────────────────────────────────────┤
│ model-name | tokens | status        │ ← 状态行（固定底部）
│ > Input (输入行)                    │ ← 输入行（固定底部）
└─────────────────────────────────────┘
```

渲染策略：
- History: 最新 N 行可见，支持 PageUp/PageDown，滚动时底部区域不动
- 状态行: `modelName | token-count | approval-status`，固定在输入框上方
- Input: 行内编辑，光标跟随，固定在最底部

## 十、借鉴优先级

### P0 — 必须借鉴（核心功能）

| 组件 | atomcode 位置 | 理由 |
|-----|-------------|------|
| TerminalGuard RAII | lib.rs:80-247 | 终端安全恢复是必须的 |
| signal_restore | signal_restore.rs | async-signal-safe 恢复 |
| InputEvent + reader | input/mod.rs, reader.rs | 专用线程输入是必须的 |
| Action 分类 | input/key_action.rs | readline 绑定是标准交互 |
| PlainRenderer 模式 | render/plain.rs | printf 风格渲染是 mini 核心 |
| UiLine 语义行 | render/mod.rs | 解耦内容与渲染 |
| sanitize | sanitize.rs | LLM 输出必须清理 |

### P1 — 建议借鉴（提升体验）

| 组件 | atomcode 位置 | 理由 |
|-----|-------------|------|
| TerminalCaps 探测 | terminal.rs | 颜色支持检测 |
| glyph 降级 | glyph.rs | 非 Unicode 终端兼容 |
| text_edit 工具 | state.rs:5-41 | 正确的 Unicode 编辑 |
| MdState 行级处理 | markdown.rs | 流式 Markdown |

### P2 — 可选借鉴（锦上添花）

| 组件 | atomcode 位置 | 理由 |
|-----|-------------|------|
| OSC 主题探测 | terminal_bg.rs | 自动深色/浅色 |
| mascot | render/mascot.rs | 欢迎界面 |
| tip 选择 | render/welcome_tips.rs | 用户引导 |

### 不借鉴（复杂度过高）

| 组件 | atomcode 位置 | 理由 |
|-----|-------------|------|
| cell-diff 渲染 | render/cell.rs, screen.rs | mini 不需要 |
| off-main-thread | render/worker.rs | mini 足够轻量 |
| 粘贴突发检测 | input/reader.rs | 简化处理 |
| 模态对话框 | modals/ | mini 无模态 |
| 多屏导航 | event_loop/ | mini 单屏 |
