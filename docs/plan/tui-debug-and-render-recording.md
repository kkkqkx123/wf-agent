# TUI 调试与渲染录制方案

## 背景

当前 `wf-cli` 已有完整的 TUI 体系（full TUI / mini / headless 三模式），但缺少：
- 独立的调试 example（无法隔离复现 UI 问题）
- 渲染 diff 录制能力（无法定位 SSH 卡顿、过度重绘）
- 按键绑定的手工测试工具（keymap 改动后只能靠人工肉眼验证）

本方案基于 `docs/analysis/调试.txt` 和 `docs/analysis/渲染录制.txt` 的分析结论，结合现有代码结构落地。

---

## 一、整体架构

```
crates/app/wf-cli/
├── src/
│   ├── lib.rs              # 新增 pub mod tui_debug（cfg feature）
│   ├── tui_debug/
│   │   ├── mod.rs           # 模块入口
│   │   ├── diff_recorder.rs # DiffRecorderBackend 实现
│   │   └── key_receiver.rs  # 按键接收 example 的 UI 组件
│   └── ...（现有代码不动）
├── examples/
│   ├── tui_key_receiver.rs  # 按键接收测试
│   ├── tui_animation.rs     # 流式动画循环预览
│   └── tui_diff_record.rs   # 渲染 diff 录制
├── tests/
│   └── tui_snapshot.rs      # 离屏快照测试
└── Cargo.toml               # 新增 dev-dependencies、features、[[example]]
```

---

## 二、DiffRecorderBackend（渲染录制）

### 2.1 实现位置

`crates/app/wf-cli/src/tui_debug/diff_recorder.rs`

包装 `CrosstermBackend`，拦截 `Backend::draw()` 的 diff 单元格迭代器：
1. 将 diff 内容渲染到内存 `CrosstermBackend<Vec<u8>>`，得到 ANSI 字节流
2. 写入录制输出（文件），附带帧号和字节大小标记
3. 透传给真实终端后端

### 2.2 关键设计

```rust
pub struct DiffRecorderBackend<B: Backend> {
    inner: B,
    recorder: Box<dyn Write + Send>,
    frame_count: u64,
}
```

- 实现 `Backend` trait，所有方法转发 `inner`，仅 `draw()` 拦截
- `into_inner()` 用于退出时取出原始 backend 做终端恢复
- Feature gate: `diff-record`，仅 example/dev 构建启用

### 2.3 使用方式

```rust
// examples/tui_diff_record.rs
let real_backend = CrosstermBackend::new(stdout);
let record_file = File::create("diff_log.txt")?;
let rec_backend = DiffRecorderBackend::new(real_backend, record_file);
let mut terminal = Terminal::new(rec_backend)?;
// ... 事件循环 ...
// 退出时：terminal.into_inner() → rec_backend.into_inner() → ratatui::restore()
```

### 2.4 分析能力

- 每帧字节大小统计：正常打字 <200B，异常重绘 >1KB
- 流式输出每 token 的 diff 开销
- resize 那一帧的整屏重绘字节
- 可用 `ansi-to-html` 转 HTML 可视化

### 2.5 Cargo.toml 变更

```toml
[features]
diff-record = []

[dev-dependencies]
insta = "1"
```

代码中用 `#[cfg(feature = "diff-record")]` 控制编译。

---

## 三、按键接收 Example（tui_key_receiver）

### 3.1 目的

提供一个专用的按键测试工具：
- 进入 raw mode + alternate screen
- 上方显示按键历史列表（最近 N 条）
- 下方固定一个输出框，实时显示当前按下的按键信息
- 显示 `Key` 结构体的完整解析结果（code、ctrl、alt、shift）
- 显示对应的 `KeyAction`（通过 Keymap resolve）

### 3.2 UI 布局

```
┌─ Key Receiver ──────────────────────────────────┐
│ Press any key... (q to quit)                     │
│                                                  │
│ History:                                         │
│  1. Ctrl+C        → Interrupt    (context: Global)│
│  2. Char('j')     → MoveNext     (context: List) │
│  3. Enter         → Select       (context: List) │
│  4. Esc           → Back         (context: Global)│
│                                                  │
├─ Current Key ────────────────────────────────────┤
│  Code: Char('a')  Ctrl: false  Alt: false        │
│  Action: None (context: Global)                  │
│  Raw event: KeyEvent { code: Char('a'), ... }    │
└──────────────────────────────────────────────────┘
```

### 3.3 实现要点

- 复用 `wf_cli::keymap` 的 `Key`、`KeyAction`、`Keymap`、`KeymapContext`
- 复用 `wf_cli::terminal` 的 `TerminalGuard` + `CrosstermControl` 做终端管理
- 事件循环：`crossterm::event::poll` + `read`，转换为 `Key`，resolve action
- 按 `1-9` 切换 `KeymapContext`，实时观察同一按键在不同 context 下的 action
- 按 `q` / `Esc` 退出
- 按 `c` 清空历史

### 3.4 运行

```bash
cargo run --example tui_key_receiver
```

### 3.5 独立性

此 example 仅依赖 `wf-cli` crate 的公开类型，不引入真实 agent/runtime/domain 逻辑。
mock 数据完全在 example 内部，不污染主程序。

---

## 四、流式动画 Example（tui_animation）

### 4.1 目的

模拟 LLM token 流式输出，循环播放，调试：
- 滚动行为
- 渲染节流效果
- diff 输出大小
- 长消息自动滚动

### 4.2 设计

- 复用 `wf_cli::screens` / `wf_cli::scrollback` 的渲染逻辑
- 模拟生成器定时产生假 token，追加到消息列表
- 按空格暂停/播放，`q` 退出
- 可选开启 `diff-record` feature 录制 diff

### 4.3 运行

```bash
cargo run --example tui_animation
cargo run --example tui_animation --features diff-record  # 带 diff 录制
```

---

## 五、离屏快照测试（tui_snapshot）

### 5.1 位置

`crates/app/wf-cli/tests/tui_snapshot.rs`

### 5.2 实现

利用 ratatui 的内存 Buffer 渲染，不启动真实终端：
- 构造 `AppState` 的各种 mock 状态（空对话、长对话、thinking 状态等）
- 调用渲染函数写入 `Buffer`
- 用 `insta` crate 做快照对比

### 5.3 覆盖场景

| 快照名 | 测试内容 |
|--------|---------|
| `chat_empty` | 空对话界面 |
| `chat_short_message` | 短消息渲染 |
| `chat_long_scroll` | 长消息滚动区域 |
| `chat_streaming` | 流式输出中的中间状态 |
| `footer_composer` | 底部输入框 |
| `footer_approval` | 审批视图 |
| `dashboard` | 仪表盘布局 |

### 5.4 运行

```bash
cargo test -p wf-cli --test tui_snapshot
cargo insta review  # 审查快照变化
```

---

## 六、日志调试约定

### 6.1 统一日志输出

所有 TUI 调试日志写入文件，**禁止 println/eprintln 到 stdout/stderr**（会污染 alternate screen）。

### 6.2 实现

在 example 和主程序中统一使用 `tracing` + `tracing_appender`：
- Full-TUI / Mini 模式：日志写入 `./tui_debug.log`
- Headless 模式：可选输出到 stderr

### 6.3 关键日志点

- 滚动偏移 `scroll_offset`
- 消息数量 `message_count`
- 重绘标记 `need_redraw`
- resize 事件 `terminal_size`
- 每帧状态 tick

---

## 七、实施步骤

### Phase 1: 基础设施
1. 在 `wf-cli/src/` 创建 `tui_debug/` 模块（mod.rs + diff_recorder.rs）
2. 在 `Cargo.toml` 添加 `diff-record` feature 和 `insta` dev-dependency
3. 在 `wf-cli/src/lib.rs` 添加 `pub mod tui_debug`（cfg feature gate）

### Phase 2: 按键接收 Example
1. 创建 `examples/tui_key_receiver.rs`
2. 实现按键捕获、Key 解析、KeyAction resolve、UI 渲染
3. 支持 context 切换和历史记录

### Phase 3: 渲染 Diff 录制
1. 实现 `DiffRecorderBackend`
2. 创建 `examples/tui_diff_record.rs` 集成录制
3. 验证 diff_log.txt 输出

### Phase 4: 动画 Example
1. 创建 `examples/tui_animation.rs`
2. 实现模拟 token 生成器 + 循环播放
3. 可选集成 diff-record feature

### Phase 5: 快照测试
1. 创建 `tests/tui_snapshot.rs`
2. 为关键 UI 状态编写快照测试
3. 生成基线快照

---

## 八、注意事项

1. **example 复用 src 模块，不复制渲染代码**——否则 example 和产品代码会分叉
2. **example 动画仅限调试，禁止移植到产品代码**——产品流式由真实 LLM channel 驱动
3. **TUI 代码严禁 println**——全部走文件日志
4. **快照测试只能测静态渲染输出**——交互逻辑仍需 example 人工验证
5. **example 退出时必须调用 `ratatui::restore()`**——加 panic hook 作为兜底
6. **DiffRecorderBackend 仅用于 example/dev**——用 feature flag 控制，不打包进正式二进制
