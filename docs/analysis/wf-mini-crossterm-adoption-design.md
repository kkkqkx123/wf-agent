# wf-mini crossterm 使用分析与改造设计

## 0. 文档信息

- 范围：`crates/app/cli/wf-mini`（含 `crates/app/cli/wf-mini/Cargo.toml` 与全部 `src/*.rs`）
- 依据：仓库当前主干代码，crossterm 版本取自根 `Cargo.toml` 工作区依赖
- 结论摘要：wf-mini 目前仅在 `editor.rs` 一个文件里使用 crossterm，且只用了 raw mode 开关、清屏和最基础的移动/着色命令；同文件里约 170 行手写按键解码器（`KeyDecoder`）与 `render.rs` 里全部手写 ANSI SGR 拼接，都是 crossterm 已有能力。建议按"渲染统一 → 输入迁移 → 能力增强 → 可选进阶"四个阶段推进，其中渲染统一与能力增强改动面小、收益明确，输入迁移改动面大、需单独拍板。

经第 12 章 stdio 契约复核后的结论修正：全部保留项均验证不影响 stdio 契约（TTY 门控、stderr 侧、stdout 零接触、管道路径无转义），且运行时开销可忽略；三项越界功能被移除——SetTitle（无收益的额外输出）、键盘增强与多行输入（改变原生按键上报并牵动协议，属 wf-tui 全屏 TUI 的职责域）、`/clear` Purge 升级（销毁滚动缓冲，直接破坏原生复制）。既有 autosuggest 功能（提交 58da92ee）复核通过，不需要调整。
- 约定：本文遵守 AGENTS.md，代码、注释一律英文，方案只写自然语言不贴完整代码；所有行号以当前代码为准，代码落地后须同步更新本文。

## 1. 背景与目标

wf-mini 的定位是"轻量级 crossterm TUI 二进制"（AGENTS.md 对 cli 层的描述），采用行纪律（line discipline）+ 追加式输出模型：stdout 只承载助手文本，stderr 承载诊断与工具生命周期，提示符在 raw 模式单行编辑器中运行，非 TTY 场景回退 cooked 行读取。

当前实现存在三类与定位不符的事实：

1. crossterm 只被用作"命令发射器"（清屏、移动光标、着色、raw mode 开关），而它最大的价值——跨平台事件解析与终端能力协商——完全没有使用。
2. 与 crossterm 能力重叠的手写实现散落在 `editor.rs`（按键解码）和 `render.rs`（SGR 拼接）两处，前者还带有已知缺陷（lone ESC 阻塞、单行假设）。
3. 工作区依赖已经为 crossterm 打开了 `event-stream` feature（根 `Cargo.toml` 166 行），但 wf-mini 一行未用。

目标是：明确"哪些现状应改为 crossterm 实现、哪些新功能可由 crossterm 补充"，并给出可分阶段落地、可独立回退的设计。

## 2. 现状事实：crossterm 使用盘点

### 2.1 依赖声明

- 根 `Cargo.toml:166`：`crossterm = { version = "0.29", features = ["event-stream"] }`，工作区统一管理，`event-stream` 已启用但无人使用。
- `crates/app/cli/wf-mini/Cargo.toml:22`：`crossterm.workspace = true`；同文件 19–20 行还声明了 `tui-terminal`、`tui-clock`，但全 src 目录 grep 不到任何引用，属于死依赖（见开放点 10.2）。

### 2.2 已使用的 crossterm API（全部集中在 editor.rs）

| 位置 | API | 用途 |
| --- | --- | --- |
| editor.rs:31–33 | `MoveToColumn`、`Color/ResetColor/SetForegroundColor`、`Clear/ClearType/enable_raw_mode/disable_raw_mode` | 全部导入面 |
| editor.rs:635 | `enable_raw_mode` | 进入 raw 模式 |
| editor.rs:641 | `libc::tcflush(0, TCIFLUSH)` | 清掉 cooked 阶段残留的半行输入（crossterm 无等价 API，保留） |
| editor.rs:667–671 | `execute!(Clear(All), MoveTo(0,0))` | Ctrl-L 清屏 |
| editor.rs:707 | `queue!(MoveToColumn(0), Clear(UntilNewLine))` | 整行重绘第一步 |
| editor.rs:712–714 | `SetForegroundColor(DarkGrey)` + `ResetColor` | ghost text 暗色渲染 |
| editor.rs:718 | `MoveLeft(back)` | 全行重绘后回移光标 |
| editor.rs:691–702 | `RawGuard` Drop 调 `disable_raw_mode` | panic/unwind 安全恢复 |

### 2.3 与 crossterm 重叠的手写实现

**手写按键解码器 `KeyDecoder`（editor.rs:90–257，约 167 行 + 755–806 行测试）**：跨 chunk 的字节缓冲、ESC/CSI/SS3 序列解析（editor.rs:165–198）、CSI modifier "5" 判 Ctrl（editor.rs:210）、`~` 终结键映射（editor.rs:218–223）、多字节 UTF-8 重同步（editor.rs:231–256）。这正是 crossterm `event` 模块的核心职责，且 crossterm 版本覆盖的序列集、终端矩阵远超手写版（手写版只绑了 Alt-f 一种 Alt 键，editor.rs:189–192）。

**手写 ANSI SGR（render.rs）**：`RESET/DIM` 常量（24–25 行）、styled 渲染中 `\x1b[1m`（243 行）、DIM 包裹的代码围栏与引用符（259–262、271–273）、代码块青色 `\x1b[36m`（327–329、340–342）、分隔线 DIM（359–361）、`inline_params` 手工拼参数串（159–181、207–213）。这些全部可由 `crossterm::style` 的 `Attribute`/`Color`/`Stylize` 表达。

**两套 Ctrl-C 语义通道**：raw 提示符路径把 0x03 字节解码为 `Key::CtrlC`（editor.rs:120–123），cooked 读取路径与 turn 泵循环则用 `tokio::signal::ctrl_c()`（session.rs:243、590），approval 等待又是一处（approval.rs:121）。三条通道语义最终都归到 `PromptEvent::Interrupt` / turn 中断，但来源分裂。

### 2.4 输入管线现状

单一 stdin 泵（input.rs:290–308）是全进程唯一 stdin 读者，把原始字节块推进 unbounded channel；两个消费者轮换：cooked 的 `LineReader`（input.rs:315–428，含 drain 语义 397–411）与 raw 编辑器（editor.rs:628–648 经 `spawn_blocking` 借走 receiver，session.rs:210–230）。该管线的"单一读者"纪律是设计核心，任何输入层改造必须保住它。

### 2.5 渲染管线现状

stdout 侧三种 markdown 模式（raw/plain/styled）经 `StreamRenderer`（render.rs:441–510）整行差分输出，进入 `TurnRenderer`（session/turn_renderer.rs:65–264）按事件类型分流；stderr 侧诊断全走 `output.rs` 的纯文本函数，无任何着色。安全层 `strip_terminal_escapes`（render.rs:33–83）负责剥离模型输出中的 CSI/OSC，防止模型驱动终端——这是安全边界，不属于本方案替换范围。

## 3. 差距分析：应改用 crossterm 实现的现有功能

| # | 现状 | 问题 | 改造方向 | 改动面 |
| --- | --- | --- | --- | --- |
| 3.1 | 手写 `KeyDecoder` 解析按键 | 序列覆盖面窄（仅 Alt-f）；lone ESC 只能干等下一字节（editor.rs:25–26、166–169）；CSI 解析需自行维护终端矩阵 | TTY 输入路径整体切到 crossterm 事件源，删除 `KeyDecoder` | 大（见第 5 章） |
| 3.2 | render.rs 手写 SGR 常量与拼接 | 转义串散落 6 处以上；`inline_params` 自拼参数串；新增样式需手写裸字节 | 全部换成 `crossterm::style` 的 `Attribute`/`Color`/`Stylize`，输出前经 `queue!`/字符串化 | 小 |
| 3.3 | raw 模式 Ctrl-C 走字节 0x03、cooked 走 SIGINT，三处入口分裂 | 语义分散、测试三条线 | 统一：raw 下 Ctrl-C 即 `KeyCode::Char('c') + CONTROL` 键事件，cooked 仍 SIGINT，两路都收敛到既有 `PromptEvent::Interrupt` | 中 |
| 3.4 | tcflush 直接调 libc（editor.rs:641） | crossterm 无等价 API | 保留，但补 `#[cfg(unix)]` 门控与注释（Windows 无此语义） | 极小 |
| 3.5 | 重绘无 Hide/Show 光标包裹 | 整行重绘在慢终端上可见光标跳动 | `cursor::Hide`/`Show` 包裹 redraw | 极小 |
| 3.6 | 声明了 `tui-terminal`/`tui-clock` 但零使用 | 死依赖误导读者 | 删除或启用（见开放点 10.2） | 极小 |

不改的项及理由：

- `strip_terminal_escapes`（render.rs:33–83）：安全清洗层，防模型驱动终端，crossterm 无对应能力，保留并继续作为 stdout 前置必经。
- `LineReader` 的 drain/stale-line 语义（input.rs:397–411）：这是行纪律 REPL 的正确性机制，输入迁移时以事件适配层形式保留等价语义，不删除。
- `EditorCore` 纯状态机（editor.rs:273–600）：单元测试资产的核心，迁移只换事件来源与渲染外壳，状态机不动。

## 4. 可通过 crossterm 补充实现的功能

按收益/成本排序：

| # | 功能 | 依赖的 crossterm 能力 | 价值 | 成本 |
| --- | --- | --- | --- | --- |
| 4.1 | 长 prompt 折行重绘（修复 editor.rs:24–26 已知限制） | `terminal::size()` | 中文/长路径输入不再错位 | 中：需把"单行整绘"升级为按列宽的段落式重绘与光标换算 |
| 4.2 | 粘贴整段插入 | `EnableBracketedPaste` + `Event::Paste` | 粘贴多行不再被逐键回放（当前每个字符触发一次重绘、粘贴中的 Enter 会误提交） | 小 |
| 4.3 | stderr 诊断着色 | `style::Stylize` + `PrintStyledContent` | approval 提示、tool failed 红色、tool ok 暗色，扫读效率提升；门控复用 `display.color` 思路但需补 stderr TTY 判定（display.rs:148–165 目前只判 stdout） | 小 |
| 4.4 | lone ESC 消歧（按超时判定裸 ESC） | 事件源内部解析 | 顺手修复已知限制，无需额外代码 | 随 3.1 免费 |
| 4.5 | 完整 Alt/修饰键绑定 | `KeyModifiers` | Alt-b/Alt-d/Alt-Backspace 词级编辑、Alt-. 历史尾词，与 zsh 对齐 | 小 |
| ~~4.6~~ | ~~窗口标题 SetTitle~~ | ~~`terminal::SetTitle`~~ | **移除**：一次性的额外输出，无功能必要性，违背 mini 极简原则（见 12.3） | — |
| ~~4.7~~ | ~~键盘增强上报/多行输入~~ | ~~`PushKeyboardEnhancementFlags`~~ | **移出路线图**：改变终端原生按键上报、多行牵动 history/turn 协议，与 mini 定位冲突，属 wf-tui 职责域（见 12.3） | — |
| ~~4.8~~ | ~~`/clear` 升级清滚动缓冲~~ | ~~`ClearType::Purge`~~ | **明确不做**：清滚动缓冲销毁用户可复制的历史输出，直接破坏原生复制（见 12.3） | — |
| 4.9 | Windows 可构建性 | crossterm 跨平台 raw mode/事件 | libc 调用 cfg 化后理论上可跑 Windows 终端 | 中，属附带收益不做承诺 |

不建议引入：鼠标捕获、焦点报告、窗口标题、键盘增强/多行输入、滚动缓冲清除。理由与 `docs/plan/tui-input-capabilities.md` 的结论一致——鼠标捕获会挡住终端原生文本选中，而行纪律 REPL 没有自己的滚动区；焦点报告对追加式输出模型没有可消费的场景；SetTitle 是无收益的额外输出；键盘增强改变原生按键上报且多行牵动协议；Purge 销毁可复制内容。需要这类富终端能力的场景属于 wf-tui 全屏 TUI 的职责域，mini 不跟进。

## 5. 设计方案

### 5.1 总体架构：双输入源、单读者纪律不变

输入侧保持"任一时刻至多一个 stdin 消费者"的铁律，但按 stdin 是否 TTY 在进程启动时分流为两条管线，运行中不切换：

- **TTY 管线**：crossterm 事件源成为唯一 stdin 读者。工作区已启用 `event-stream`，用 `EventStream`（futures Stream）直接接 tokio，替代现在的"泵线程 + channel 借还 + spawn_blocking"三件套。raw 提示符期间编辑器消费 `KeyEvent`；cooked 期间（如 stderr 非 TTY 但 stdin 是 TTY）由事件→行适配层把 `Key(Enter)` 前的字符序列拼成行，替代 tty 线纪律的行组装。
- **管道管线**（stdin 非 TTY）：保留现有字节泵 + `LineReader` 原样。crossterm 的解析器会把管道数据中的 0x1b 序列当按键吞掉，不能用于任意管道输入，这条边界必须硬编码在启动分流处（复用 main.rs / session.rs 现有的 `is_terminal` 判定，session.rs:128–130）。

事件→编辑器的映射层：crossterm `KeyEvent` 翻译为现有 `Key` 枚举（或直接改 `EditorCore::handle_key` 签名），`EditorCore` 状态机与其全部单元测试保持可运行；Ctrl-C 键事件与 `KeyCode::Enter`、Ctrl-D 的映射集中在这一个函数里。

### 5.2 渲染层统一

`render.rs` 的所有裸 SGR 换成 crossterm style 构造：Bold/Italic/Underlined/CrossedOut/Dim 对应 `inline_params` 的五种 span，Cyan 对应代码与行内码，标题加粗走 `Attribute::Bold`。输出形态两种取一：继续产出字符串（用 `StyledContent` 的 Display 实现），或改为写入 `impl Write` 时经 `queue!` 发命令。倾向前者——`StreamRenderer` 的差分算法（render.rs:468–490）依赖"已产出字符串"与"目标字符串"做前后缀 diff，保持字符串产出可让差分逻辑零改动；迁移后需快照验证差分输出不受 SGR 表示形式变化影响（crossterm 可能发出同义的参数组合，如 `\x1b[0m` 与更长的复位序列）。

编辑器 redraw（editor.rs:706–721）已用 crossterm 命令，仅补 Hide/Show 包裹。

stderr 着色：`TurnRenderer` 的 `stderr_line` 调用点按事件类型附加样式（失败红、成功暗、usage 暗色），门控新增 stderr TTY 判定，不复用 stdout 的 `color` 标志；`NO_COLOR`/`TERM=dumb` 检查逻辑从 display.rs 提取为共用函数。

### 5.3 能力增强接线

- 折行重绘（4.1）：prompt 启动时 `terminal::size()` 取列宽存入 `EditorSnapshot`；重绘从"清一行 + 单行缓冲"升级为"按 prompt 宽度 + 列宽计算行数，清除 N 行，重绘 N 行，光标按显示宽度换算到正确行列"。宽度计算已有 `unicode-width` 基础（editor.rs:331–336）。resize 处理首版从简：提示符期间收到 resize 只在下次重绘生效。
- 粘贴（4.2）：进入 raw 提示符前发 `EnableBracketedPaste`，`RawGuard` Drop 时对称 Disable；`Event::Paste` 整体插入缓冲（含换行字符按现有拍平规则处理），不走逐键 refetch——插完后 refetch 一次。该功能不捕获鼠标、不改变终端选中复制，粘贴内容原样到达（含换行），使 raw 模式的粘贴行为向 cooked 模式的原生行为对齐而非偏离，同时消除粘贴中 Enter 误提交的缺陷。
- 键盘增强与多行输入：已移出路线图（见第 4 节与第 12 章），若未来确有需求，按"与 tui 重叠"归入 wf-tui 域评估，不在 mini 落地。

## 6. 关键设计决策与取舍

1. **为什么不在管道输入上用 crossterm 事件源**：crossterm 解析器面向按键序列设计，任意二进制/文本管道经它解析会丢字节（0x1b 被当序列引导符消费）。管道是 wf-mini 脚本化使用的一等公民（headless 形态），正确性优先。
2. **为什么保留 `Key` 枚举映射层而不让 `EditorCore` 直接吃 `KeyEvent`**：`EditorCore` 的 30 余个状态机测试（editor.rs:754–951）是重构安全网，保持纯状态机与终端类型解耦，测试不依赖 crossterm 类型即可继续运行；映射函数本身用表驱动测试覆盖。
3. **为什么渲染层保持字符串产出**：`StreamRenderer` 的差分-只增不重发契约（render.rs:588–597 测试锁定）是追加式输出的正确性核心，改走命令流会破坏"已发出字节不可回收"模型下的 diff 前提。
4. **契约优先是最高裁决准则**：任何新功能先过第 12 章的三问——是否只在 TTY 分支生效、是否触碰 stdout/stdin 数据面、是否改变终端原生选中/粘贴行为——任一不过即不进 mini，属 wf-tui 域。
5. **Ctrl-C 统一方向**：向"按键即事件"收敛而不是向 SIGINT 收敛。raw 模式下 SIGINT 不会触发（ISIG 已关），cooked 模式下 tokio signal 已工作良好；两路各自存在但都终止于同一 `PromptEvent`，文档化即可，不强行合并。
6. **分阶段而非一步到位**：输入迁移（阶段二）涉及管线重构，风险最高；渲染统一（阶段一）与能力增强（阶段三）互相独立且都能独立回退。先做低风险高确定性部分，输入迁移单独评审。

## 7. 实施步骤（分阶段）

- **阶段一：渲染统一**（低风险，先行）
  1. render.rs 裸 SGR → crossterm style，快照测试确认 styled 输出语义等价；
  2. editor.rs redraw 补 Hide/Show；
  3. tcflush 加 `#[cfg(unix)]`；
  4. 清理 `tui-terminal`/`tui-clock` 死依赖（按 10.2 决定）。
- **阶段二：输入迁移**（需拍板，见 10.1）
  1. 启动处按 stdin TTY 分流双管线；
  2. TTY 管线用 `EventStream` 替换泵线程与 `KeyDecoder`，实现 KeyEvent→Key 映射层并补表驱动测试；
  3. cooked-TTY 事件→行适配层（含 drain 等价语义）；
  4. 删除 `KeyDecoder` 及其专属测试，`EditorCore` 测试全量保留并通过。
- **阶段三：能力增强**
  1. 粘贴整段插入（bracketed paste 开关随 RawGuard 生命周期）；
  2. 折行重绘（size 进快照，N 行重绘与光标换算）；
  3. stderr 着色（新增 stderr TTY 门控）。
- **阶段四：可选进阶**
  Alt 键族补齐（Alt-b/Alt-d/Alt-Backspace/Alt-.，纯按键语义补充，契约复核通过）。键盘增强、多行输入与 `/clear` Purge 已从路线图移除（见 12.3），不再排期。

每阶段独立可验证、独立可回退：阶段一回退即还原 SGR 常量；阶段二回退即恢复双管线合并为字节泵单管线；阶段三各增强点均由独立的进入开关控制，关闭后字节行为与现状一致。

## 8. 风险与回退

| 风险 | 影响 | 缓解/回退 |
| --- | --- | --- |
| EventStream 与现有 tokio 结构集成不当（内部线程、唤醒） | 提示符卡死或事件丢失 | EventStream 是官方 async 入口、直接进 `tokio::select!`；阶段二上线前用假 TTY 手动矩阵覆盖 split-chunk 序列 |
| 双管线分流判定错误（TTY 判定与实际管线不一致） | 管道输入丢字节或 TTY 下无行编辑 | 分流只在启动处做一次，判定复用现有 `is_terminal` 结果；管道管线代码路径零改动 |
| crossterm 发出的 SGR 与手写版本字节不同 | styled 模式下差分输出漂移、快照测试失败 | 阶段一先锁快照基线，迁移后逐字节比对；必要处在 crossterm 命令层固定参数形式 |
| 折行重绘宽列计算错误（CJK、零宽字符） | 光标错位 | 沿用 `unicode-width` 并补宽字符/emoji 表驱动测试；超出能力时整行降级为现状单行行为 |
| 粘贴中 Enter 语义变化 | 老用户习惯粘贴后手动确认被跳过 | bracketed paste 作为独立开关，默认行为可配置回退 |
| raw 模式下 Ctrl-C 键事件与并发 SIGINT 竞争 | 重复中断 | 与现状相同的收敛点（`PromptEvent::Interrupt`），pump 循环幂等处理不变 |

## 9. 验收标准

- `cargo clippy --all-targets --all-features` 全绿（AGENTS.md 规定的完整编译检查）。
- 阶段一：raw/plain/styled 三模式 stdout 字节快照与迁移前语义等价；`strip_terminal_escapes` 对 styled 输出清洗后与 plain 输出一致（现有测试 styled_resets 系列 + 新增快照）。
- 阶段二：`EditorCore` 全部既有测试不改语义通过；映射层表驱动测试覆盖方向键/修饰键/UTF-8/SS3/CSI；管道形态 `echo | wf-mini` 回归不变；TTY 手动矩阵覆盖 ghost text、前缀历史浏览、Ctrl-C 双击退出、Ctrl-D、跨 chunk 粘贴。
- 阶段三：CJK 长 prompt 折行后光标位置正确；多行粘贴不触发误提交；`NO_COLOR` 与非 TTY stderr 下无任何转义字节。
- 每阶段关闭全部新开关后，进程终端输入输出字节行为与主线一致（回退线）。

## 10. 待拍板开放点

1. **阶段二是否进入本迭代**：输入迁移收益（序列覆盖、lone ESC 修复、删 170 行代码）明确，但管线重构是全部阶段里唯一动"单一读者"纪律实现的，建议单独评审后再排期；阶段一+三可先行。
2. **死依赖处置**：`tui-terminal`/`tui-clock`（Cargo.toml:19–20）当前零引用。删除最干净；若倾向复用，则 sigint/能力探测应替换本方案中对应的手写部分，需指定范围。
3. **stderr 着色门控**：新增 stderr TTY 判定并独立于 `display.color`，还是扩展 `DisplayConfig::color` 语义为"双流判定"？建议前者，避免 stdout 关色连带 stderr。
4. **折行重绘的 resize 语义**：提示符期间终端 resize 首版不即时响应（下次重绘生效），是否可接受。

已关闭（经第 12 章复核裁决）：多行输入是否进路线图——**不进**（改变原生按键上报、牵动协议，属 tui 域）；`/clear` 是否升级 Purge——**不升级**（销毁滚动缓冲破坏原生复制，维持 "---" 约定）。

## 11. 后续演进

- 阶段二落地后，评估把 KeyEvent→Key 映射层与事件→行适配层下沉到 `wf-cli-shared` 或 `tui-terminal`，供未来 `app/desktop/` 之外的轻量前端复用（AGENTS.md 允许 cli 层直接依赖底层 tui crate）。
- 键盘增强、鼠标、焦点等富终端输入能力统一归 wf-tui 域（见 `docs/plan/tui-input-capabilities.md`）；wf-mini 只在其产出属于"纯渲染且 TTY 门控"的结论时（如粘贴整段插入）选择性对齐，不做模式上报类改造。
- 本文落地过程中产生的行号漂移、阶段增删，须随代码同步更新本文，不采用标记过期的方式。

## 12. stdio 契约与开销复核

本章是设计裁决的最高依据：wf-mini 的核心定位是尽力符合 stdio 要求、压低额外开销、保持复制/粘贴与终端原生行为一致。任何功能的存废以此为准。复核对象包括本方案的全部功能与既有 autosuggest 提交（58da92ee，新增 suggest.rs/editor.rs、改造 input.rs/session.rs 共约 1610 行）。

### 12.1 stdio 契约的代码事实基线

1. stdout 只承载助手业务文本：唯一的 stdout 写入点经 `output::write_stdout`（output.rs:43–48，注释明确 "stdout carries only business output"），由 `TurnRenderer` 的 sink 分流保证（turn_renderer.rs:39–51）；非 TTY stdout 直接走 headless 形态（main.rs:32–40），管道重定向下 stdout 永远干净。
2. stderr 承载提示符、诊断、工具生命周期：提示符写入 stderr（input.rs:419–427），raw 编辑器全部渲染输出走 stderr（editor.rs:655）。
3. raw 编辑器仅在 stdin 与 stderr 双 TTY 时启用（session.rs:130），非 TTY 全链路回退 cooked 行读取，零转义字节。
4. 追加式输出：字节进 scrollback 不可回收；模型输出中的转义序列一律清洗（render.rs:33–83），应用自身只在 TTY 分支产生样式转义（display.rs:148–165 的 color_enabled：非 TTY stdout、NO_COLOR、TERM=dumb 全关）。
5. 原生复制/粘贴：进程不捕获鼠标、不改键盘上报模式，选中与复制完全由终端完成，粘贴就是普通按键流。

### 12.2 逐项复核：不影响契约、开销可忽略，保留并附说明

| 功能 | 契约影响 | 开销 | 复制/粘贴影响 | 说明 |
| --- | --- | --- | --- | --- |
| autosuggest（58da92ee，既有） | 无：ghost text 画在 stderr 提示符行且双 TTY 才启用（session.rs:130）；提交行只含 typed buffer，建议文本不进 buffer、不进 history/turn（editor.rs:41–43）；stdout 零接触 | 每按键一次 ≤200 条内存前缀扫描 + 一次整行重绘（编辑器本来就整行重绘），broker 为单个 AtomicU64 token，可忽略 | 与 zsh-autosuggestions 同模型；选中提示行会把 ghost 一起选中是终端原生选中行为的自然结果，与 zsh 一致 | 保留，无需调整 |
| 渲染统一（3.2） | 无：仅改 styled 字符串的构造方式，产出语义等价，TTY 门控不变 | 编译期类型替换，运行时零增量 | 无 | 保留 |
| Hide/Show（3.5） | 无：stderr、raw 提示符期间 | 每次重绘多两组 2 字节序列 | 无 | 保留 |
| 输入迁移（3.1/阶段二） | 无：TTY-only 路径，stdin 仍是单一读者，管道路径零改动 | crossterm 内部线程 + channel 对比现有泵线程 + channel，开销持平；event-stream 转发一跳，微增 | 无（不碰鼠标与上报模式） | 保留，排期待 10.1 |
| 折行重绘（4.1） | 无：stderr、编辑器激活期间 | 重绘 1 行变 N 行 + 快照内缓存一次 size 调用 | 与 bash/zsh 提示符折行行为一致 | 保留 |
| 粘贴整段插入（4.2） | 无：TTY-only | 进入/退出各一组开关序列 | **正向**：不捕获鼠标、不改选中；粘贴内容原样到达，raw 粘贴向 cooked 原生行为对齐，消除粘贴中 Enter 误提交 | 保留 |
| stderr 着色（4.3） | 无：新增 stderr TTY 门控 + NO_COLOR/TERM=dumb 检查，管道 stderr 无转义 | 每诊断行多几十字节，仅 TTY | 无 | 保留 |
| Alt 键族（4.5） | 无：纯按键语义 | 映射层几行 | 无 | 保留，低优先级 |
| lone ESC 消歧（4.4）、tcflush cfg 化（3.4）、死依赖清理（3.6） | 无 | 无（删除/中性） | 无 | 保留 |

### 12.3 逐项复核：触碰契约或违背定位，移除

| 功能 | 触碰点 | 裁决 |
| --- | --- | --- |
| 键盘增强 / 多行输入（原 4.7） | 改变终端原生按键上报方式（push enhancement flags 后部分终端按键行为改变）；多行输入牵动 history 存储（input.rs:186 拍平换行）与 turn 协议；"需要 richer input"本身是 wf-tui 全屏 TUI 的职责域 | 移出路线图，归 wf-tui 域 |
| `/clear` Purge（原 4.8） | 清滚动缓冲直接销毁用户可复制的历史输出，与"复制行为与终端原生一致"正面冲突，也违背追加式输出模型 | 明确不做，维持 "---" 分隔约定 |
| SetTitle（原 4.6） | 不破坏契约，但属无功能必要性的额外输出，违背"尽力减少额外开销"的定位 | 移除 |
| 鼠标捕获 / 焦点报告 | 捕获挡住原生选中复制（破坏复制）；焦点报告在行纪律模型下无可消费场景（纯开销） | 维持排除（本方案首版已排除，此处记录裁决依据） |

### 12.4 复核后的总体结论

本方案保留项全部通过三问检验（TTY 分支生效、不碰 stdout/stdin 数据面、不改原生选中/粘贴），运行时开销增量均在"每次交互几十字节、常数次内存扫描"量级，可忽略；autosuggest 既有提交（58da92ee）同样通过，不需回退或调整。被移除的四类功能均因触碰契约或违背极简定位，与 tui 的功能重叠区由此划清：**mini 只做"渲染与解析"层面的对齐，不做"模式上报与屏幕所有权"层面的改造**——后者是 wf-tui 的领地。
