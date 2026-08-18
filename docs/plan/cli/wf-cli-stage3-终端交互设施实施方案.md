# wf-cli Stage 3 实施方案：终端交互设施（通用 F8/F9）

> 状态：已完成（阶段 3A–3D，2026-08-18）
> 上游方案：`docs/plan/cli/wf-cli-分阶段实现方案.md`（Stage 3 任务定义）、`docs/cli/03-组件设计方案.md` §2.1/§六（TerminalGuard / with_restored / stderr 抑制 / OSC 探测）、`docs/cli/05-opencode-mini模式与无头模式设计.md` §3.3/§4.5（SIGINT 两按、SIGUSR2 热更新）
> 范围：UI 形态前置的终端安全设施——`terminal.rs`（TerminalGuard / with_restored / TerminalStderrGuard / SIGINT 两按）与 `theme.rs`（OSC 10/11 探测 / 调色板推导 / 缓存 / SIGUSR2）。**与渲染解耦、可独立验证**；正式引入 `crossterm` 依赖，但仍不引入 ratatui（Stage 4 才引入）。

## 一、现状与缺口

Stage 2 已交付 run 无头闭环（50 项测试全绿），无任何 UI 依赖。Stage 3 缺口：

| # | 缺口 | 说明 |
| :- | :--- | :--- |
| G1 | `terminal.rs` 不存在 | mini / TUI 都需要 raw mode / alt-screen / bracketed paste / cursor 的 RAII 保护，防止 panic / 早退留下残缺终端 |
| G2 | `with_restored` 缺失 | Settings/Workflow 屏 `[E] edit`、mini `/editor` 需要暂停视口运行 `$EDITOR` 再全量重绘（03 §2.1） |
| G3 | stderr 抑制缺失 | TUI 运行期间子进程/后端 stderr 会打花画面（03 §2.1）；headless 侧已有 `--log`，UI 侧需要 fd 级重定向设施 |
| G4 | SIGINT 两按无承载 | 05 §4.5：第一次清空 composer / 提示，第二次（5s 窗口内）中断 turn；需纯状态机供 Stage 6 事件循环消费 |
| G5 | `theme.rs` 不存在 | 03 §六：启动探测 OSC 10/11（100ms 超时）+ `COLORTERM` 色域判定，fallback 深色；05 §3.3：SIGUSR2 热更新；组件层需要 `Theme` 纯数据 |
| G6 | 无探针命令 | 验收要求 `with_restored`（启动 $EDITOR 后终端无残留）与主题探测降级可冒烟验证 |

## 二、外部最佳实践参考（context7 检索结论）

1. **crossterm（/crossterm-rs/crossterm）**：
   - `terminal::enable_raw_mode()` / `disable_raw_mode()` 是**手动开关**（0.14 起 `RawScreen` RAII 已移除）——"user is responsible for explicitly disabling raw modes"，正是需要自建 RAII Guard 的原因；
   - 官方 interactive-demo 模式：`EnterAlternateScreen → enable_raw_mode → 循环 → ResetColor + Show cursor + LeaveAlternateScreen → disable_raw_mode`，且**错误路径同样走清理**（`if let Err` 后恢复）——恢复顺序应为进入顺序的逆序；
   - `event::poll(Duration)` + `event::read()` 是带超时探测输入的标准组合（OSC 响应读取、resize 批量冲刷均用此模式）；异步侧用 `EventStream`。
2. **ratatui-crossterm（/websites/rs_ratatui-crossterm）**：`CrosstermBackend` **不自动**管理 raw mode / alt-screen（"Users are responsible for managing terminal modes"）——模式管理必须由应用层（即本阶段 Guard）承担；setup/teardown 标准序列与本方案 `enter`/`restore` 对齐。
3. **仓内既有范式**：`mode.rs` 的"注入 TTY 状态"、`run.rs` 的"DiagWriter 可注入缓冲"、`output.rs` 的"Sink 抽象 + MemorySink 测试"——本阶段沿用**控制平面注入**（`TerminalControl` trait + `FakeControl` 记录操作序列）实现无 TTY 单测。

## 三、关键设计决策

| # | 决策 | 理由 |
| :- | :--- | :--- |
| D1 | **模式状态机 + 控制平面注入**：`TerminalModes`（raw/alt_screen/bracketed_paste/cursor_hidden 四开关位）+ `TerminalControl` trait（enable/disable 共 8 个操作）+ `TerminalGuard<C>` 泛型守护；真实实现 `CrosstermControl`，测试用 `FakeControl`（记录操作日志） | 双次进入/退出一致性可无 TTY 单测；对 crossterm 的依赖收敛在一个 impl 块；Stage 4 ratatui 接入时 Guard 不变 |
| D2 | **增量应用 + 逆序恢复**：`enter(new)` 只翻转与当前 tracked 状态的差量；`restore()` 回到全关基线，同样走差量；恢复顺序固定 cursor → alt-screen → paste → raw（进入的逆序） | 重复 enter/exit 幂等（验收项）；避免对未持有模式误发恢复序列 |
| D3 | **panic hook 双保险**：`TerminalGuard::install_panic_hook()` 安装全局 hook（包裹既有 hook），panic 时对**真实终端**执行 best-effort 全量恢复（disable raw / leave alt / show cursor / disable paste）再委托原 hook | 对齐 03 §2.1"Drop + panic hook 双保险"；hook 与 Guard 实例解耦（进程级），不可注入的部分仅此一处 |
| D4 | **`with_restored(op)` 暂停-运行-恢复**：保存当前 `TerminalModes` → `restore()`（顺带恢复被抑制的 stderr，若传入 guard）→ 运行外部命令（继承 stdio）→ 重入保存的模式；**重绘责任归调用方**（Guard 不持有渲染器），返回后调用方全量重绘 | 对齐 03 §2.1 与 05 §4.5 `/editor` 路径；签名带 `Option<&mut TerminalStderrGuard>` 覆盖"with_restored 期间恢复原 stderr" |
| D5 | **stderr 抑制 = fd 2 重定向**：`TerminalStderrGuard::suppress_to(path)`（unix：`dup` 保存 + `dup2` 文件 fd 到 2），`restore()` / Drop 回填；非 unix 返回 no-op guard | fd 级重定向对子进程同样生效（进程继承）；headless 落盘语义已有 `--log`，UI 侧复用文件目标即可 |
| D6 | **SIGINT 两按纯状态机**：`DoublePressTracker`（u64 毫秒时钟注入，5s 窗口）→ `PressOutcome::FirstPress/SecondPress`；附 `press_now()`（内部 Instant 起点）供运行时 | 对齐 05 §3.3/§4.5；合成信号单测（验收项）不依赖真实信号；Stage 6 事件循环接线 |
| D7 | **OSC 解析独立状态机**：`OscColorParser`（feed 字节块 → fg/bg 两个 `Option<Rgb>`，`\x1b]1x;rgb:RR/GG/BB\x1b\\` 或 BEL 结束，通道 1-4 位十六进制归一到 8bit）+ 真实探测 `probe_theme(timeout)`（/dev/tty + 临时 raw + `poll` 超时读） | 解析纯函数化可注入 `Cursor` 单测；探测失败（无 /dev/tty / 超时 / 无响应）**永不 panic**，走缓存 → 默认主题降级链 |
| D8 | **主题 = 纯数据 + 三级来源**：`Theme { kind, bg, fg, muted, accent, add, remove, warning, error, highlight, source }`（8 角色对齐 03 §六 ColorRole）；`source ∈ Probed/Cached/Default`；`derive_theme(bg, fg)` 由亮度（相对亮度 ITU-R BT.601）判 Dark/Light 并派生角色色 | 组件层（Stage 4+）只消费 `Theme`，与探测机制解耦；accent 从候选集选与背景对比度最大者 |
| D9 | **最后已知良好缓存**：`$XDG_CACHE_HOME/wf-cli/theme.json`（fallback `$HOME/.cache/...`），探测成功即写，失败时读；无缓存 → `Theme::dark_default()` | 对齐"最后已知良好主题缓存"；探测每会话一次 + SIGUSR2 重探测 |
| D10 | **SIGUSR2 热更新 watcher**：`theme_reload_signals()`（unix：tokio `signal(SignalKind::user_defined2)` → mpsc；非 unix 返回关闭通道），收到信号由调用方重新 probe + 推送新 Theme | 对齐 05 §3.3 主题热更新；Stage 6 事件循环 select 分支消费 |
| D11 | **探针子命令 `wf debug-terminal`**：安装 panic hook → 真实 Guard 进入模式（`--alt` 可选 alt-screen）→ 打印模式/主题探测结果 → `with_restored` 运行 `$EDITOR`（或 `--exec`，默认 `true`）→ 重入并打印状态；非 TTY 环境降级打印默认主题不报错 | 验收冒烟载体（G6），对齐既有 `debug-mode` 诊断命令风格；CI 无 TTY 也能跑（验证降级路径） |

## 四、模块落点

```
crates/wf-cli/src/
├── terminal.rs       ← 新增：TerminalModes/TerminalControl/TerminalGuard
│                      + CrosstermControl/FakeControl
│                      + with_restored + install_panic_hook
│                      + TerminalStderrGuard（fd 2 重定向）
│                      + DoublePressTracker（SIGINT 两按 5s 窗口）
├── theme.rs          ← 新增：Rgb/ThemeKind/Theme（8 角色 + source）
│                      + OscColorParser + derive_theme + luminance
│                      + probe_theme（OSC 10/11 + poll 超时）
│                      + 主题缓存（XDG）/ theme_reload_signals（SIGUSR2）
├── args.rs           ← 扩展：DebugTerminal 子命令（--alt/--exec）
└── lib.rs            ← 接线：pub mod + run() 分发 debug-terminal

Cargo.toml（root）    ← workspace.dependencies 新增 crossterm = "0.29"
crates/wf-cli/Cargo.toml ← crossterm、libc
```

## 五、分阶段任务与验收

### 阶段 3A：依赖引入与 Guard 状态机（G1 主体）

- [x] root `Cargo.toml` workspace 依赖新增 `crossterm = "0.29"`（与未来 ratatui 0.30 的 crossterm 版本对齐）；wf-cli 增 `crossterm` + `libc`。
- [x] `terminal.rs`：`TerminalModes`（四开关位，`Copy + PartialEq`）、`TerminalControl` trait（8 操作）、`TerminalGuard<C>`（`enter` 差量应用 / `restore` 逆序差量恢复 / Drop 兜底 / `modes()` 只读）、`CrosstermControl`（crossterm 真实现，stdout 为 writer）与 `FakeControl`（`ops: Vec<String>` 操作日志）。
- [x] 单测（FakeControl）：进入→恢复→再进入→再恢复 的操作序列断言；重复 enter 同模式零操作；部分模式差量只翻转变化位；restore 幂等（第二次零操作）。

**验收**：`cargo check -p wf-cli` 通过；状态机单测全绿（无真实 TTY 参与）。✅

### 阶段 3B：with_restored / stderr 抑制 / panic hook / SIGINT 两按（G2/G3/G4）

- [x] `TerminalGuard::with_restored(&mut self, stderr: Option<&mut TerminalStderrGuard>, op)`：保存模式 → restore（+ 恢复 stderr）→ op() → 重入模式；返回 op 结果；重入失败映射 `CliError::Io`。单测（FakeControl）：操作序列 = 全量恢复 + 全量重入，op 闭包执行于"干净终端"窗口。
- [x] `TerminalStderrGuard`：`suppress_to(path)`（unix dup/dup2；非 unix no-op）、`restore()`、Drop 兜底；抑制期间 `io::stderr()` 写入进入文件。单测（unix，tempfile + 串行锁）：抑制 → 写 stderr → 恢复 → 文件内容断言 + 再次写入不进文件。
- [x] `TerminalGuard::install_panic_hook()`：包裹既有 hook，先对真实终端 best-effort 恢复（顺序同 restore）再委托；幂等（重复安装只包一层）。
- [x] `DoublePressTracker`：`new(window)` / `press(now_ms)` / `pending(now_ms)` / `reset()`；`PressOutcome` 两态。单测：首次 FirstPress；窗口内二次 SecondPress；窗口过期后二次 FirstPress；pending 过期翻转；`press_now()` 冒烟。
- [x] 常量：`SIGINT_DOUBLE_PRESS_WINDOW = 5s`（对齐 05 §附）。

**验收**：`cargo test -p wf-cli`（terminal 模块）全绿；stderr 抑制在测试进程内自恢复（不影响其余测试输出）。✅（13 项）

### 阶段 3C：theme.rs 主题探测（G5）

- [x] `Rgb` / `ThemeKind` / `Theme`（serde Serialize/Deserialize，含 `source`）；`dark_default()` / `light_default()`。
- [x] `OscColorParser`：`feed(chunk)` / `is_complete()` / `finish() -> (fg, bg)`；容错：非 OSC 字节跳过、截断响应丢弃、1-4 位/通道十六进制归一。单测：标准 4 位响应、2 位响应、BEL 结束、10/11 并存、乱序到达（分块 feed）、噪音夹杂、无响应（空 feed）。
- [x] `luminance` / `derive_theme(bg, fg)`：亮度阈值 0.5 判 Dark/Light；muted = fg/bg 混合；accent 从候选集（cyan/violet/amber/teal）取与 bg 最大对比；add/remove/warning/error/highlight 按主题明暗选深浅变体。单测：黑白背景判定、角色色在 Dark/Light 下不同、对比度断言。
- [x] `probe_theme(timeout)`（unix）：打开 /dev/tty（读写）→ 临时 raw（记先态）→ 写 OSC 11/10 查询 → `poll` + 读循环喂 parser（完成或超时止）→ 恢复先态 → `derive_theme`；任一步失败走降级链（缓存 → 默认）。`probe_theme` 不 panic、不返回 Result（`Theme.source` 表达来源）。
- [x] 缓存：`theme_cache_path()`（`XDG_CACHE_HOME` / `HOME` 回退）+ `save_theme_cache` / `load_theme_cache`。单测（tempdir + 环境锁）：写后读回一致。
- [x] `theme_reload_signals()`（unix：SIGUSR2 → mpsc::Receiver<()>；非 unix 关闭通道）。单测（unix）：启动 watcher → `kill(getpid(), SIGUSR2)` → 超时内收到一次。

**验收**：`cargo test -p wf-cli`（theme 模块）全绿；无 OSC 响应环境（CI）探测回退 `Default` 来源不 panic。✅（18 项）

### 阶段 3D：探针接线与冒烟（G6）

- [x] `args.rs`：`Command::DebugTerminal { alt_screen: bool, exec: Option<String> }`；`lib.rs` 分发到 `debug_terminal(cli)`。
- [x] `debug_terminal`：安装 panic hook → 真实 Guard 进入（raw + paste + hide cursor [+ alt]）→ 主题探测并打印（含 source）→ `with_restored` 运行 `$EDITOR` / `--exec` / `true` → 重入后打印"no residual modes" → restore 退出；stdout 非 TTY 时打印降级说明（默认主题 + Guard 未激活），exit 0。
- [x] 冒烟：`cargo run -p wf-cli -- debug-terminal`（无 TTY CI 环境）→ 降级路径 OK；真实终端人工冒烟清单（alt-screen 进出无残留、$EDITOR 退出后画面重绘、SIGUSR2 重探测）记录于本文档。
- [x] 勾选总方案 Stage 3 任务项，补完成记录；生成 patch（排除构建产物）。

**验收**：`cargo test -p wf-cli` 全绿（Stage 2 的 50 项无回归 + Stage 3 新增 31 项，合计 81 项）；探针命令在 CI 降级路径 exit 0。✅

**冒烟记录（2026-08-18，`script` PTY）**

- `wf debug-terminal --exec 'echo inside-window'`：模式进入（raw+paste）→ `[with_restored]` 窗口内外部命令输出可见 → 重入后重绘帧 → 恢复全关，exit 0。
- `wf debug-terminal --alt-screen --exec true`：alt-screen 进出无残留状态，exit 0。
- 无 TTY（CI）：`theme #0f141a kind Dark (default fallback)`，guard 未激活，exit 0，无 panic。
- SIGUSR2：`sigusr2_delivers_a_reload_signal` 单测（注册 handler 后向自身发信号，2s 内收到）。

## 六、与方案的偏差（实施期决策）

| # | 偏差 | 原因 |
| :- | :--- | :--- |
| P1 | panic hook 与 Guard 实例解耦：hook 直接对真实终端执行 best-effort 恢复后委托原 hook（进程级、幂等），不按实例登记快照 | 简化所有权（hook 无法借用可能已部分 drop 的 guard）；"Drop + panic hook 双保险"语义不变 |
| P2 | `CrosstermControl<W>` 泛化 writer（生产 stdout / 测试 `Vec<u8>`），`TerminalControl: Debug` 约束经 `W: Write + Debug` 满足 | 与 mode.rs/run.rs 的注入式测试范式一致 |
| P3 | `TerminalStderrGuard::restore()` 保留目标文件句柄，`re_suppress()` 以当前 fd 2 重新 dup/dup2 实现真实重抑制 | 方案初稿 re_suppress 为 no-op，无法满足"with_restored 结束后恢复抑制"；保留句柄即可支持多次暂停/恢复循环 |
| P4 | theme 增加 `ColorDomain::detect`（COLORTERM/TERM 纯函数） | 03 §六色域判定的纯数据部分零成本前置，Stage 4 组件直接消费 |
| P5 | `probe_theme` 只需 OSC 11（bg）命中即派生主题（fg 缺省按明暗取默认前景）；bg 未命中才走缓存 → 默认链 | bg 决定 Dark/Light 与全部角色派生，fg 缺失不应丢弃整次探测 |
| P6 | 3 位/通道十六进制按 12-bit → 8-bit 归一（`v*255/4095`） | OSC 响应存在 3 位变体（xterm 派生终端），补齐 1/2/4 位之外的档位 |
| P7 | `mode.rs` 的 Command match 同步扩展 `DebugTerminal`（归 headless 诊断类）；实际派发在 `lib.rs` 预解析（与 `DebugMode` 同法） | 避免 `ModeResolver` 对新子命令的非穷尽匹配；诊断命令不进入形态判定 |

## 七、风险与边界

| 风险 | 缓解 |
| :--- | :--- |
| crossterm 版本与 ratatui 0.30 不匹配 | 固定 workspace `crossterm = "0.29"`（ratatui 0.30 系 ratatui-crossterm 同版本），Stage 4 引入 ratatui 时统一 |
| OSC 探测期间吞掉用户按键（probe 窗口内输入被读走丢弃） | 探测仅在启动/热更新瞬间（≤100ms + 读取余量），非 OSC 字节被丢弃；文档明示探测窗口内输入不回放（codex 同款行为） |
| stderr fd 重定向是进程全局（并发测试互扰） | 测试用串行锁 + 立即恢复；运行期仅 UI 形态持有（headless 不抑制） |
| panic hook 内 IO 再失败 | hook 内 best-effort（忽略二次错误），Drop 仍是第一道保险 |
| /dev/tty 不可用（CI/容器无控制终端） | `File::open` 失败即降级缓存/默认主题，不 panic（验收项） |
| SIGUSR2 测试向自身发信号 | 先启动 watcher（handler 已注册）再 kill；超时 2s 保护，非 unix 跳过 |
| Windows / 非 unix 平台 | stderr 抑制与 SIGUSR2 no-op/关闭通道；OSC 探测依赖 /dev/tty 仅 unix 生效；主开发目标 linux（对齐仓库现状） |
