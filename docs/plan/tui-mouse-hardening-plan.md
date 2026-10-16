# TUI 鼠标点击加固与能力补齐方案

来源：与 `ref/atomcode`、`ref/opencode` 的鼠标实现对比分析。
约束：只改原有模块，不新增替代模块；不改配置缺省；保留 Shift 原生选择逃逸；世代守卫机制不降级。

## 现状确认

- 鼠标捕获默认关闭，滚轮经 alternate-scroll 走按键，按压拖拽仅左键且需显式开启（`crates/app/cli/wf-tui/src/tui/input_classify.rs:76`）。
- 命中世代守卫已落地：`MousePress` 记录按压格与世代，`press_matches_release` 要求同格同世代（`crates/app/cli/wf-tui/src/interactive/mouse_hit.rs:58`）。
- 滚动条边槽单击与滑块拖拽已可用（`crates/app/cli/wf-tui/src/interactive/render.rs:281`）。
- 逐块点击展开与子行跳转已可用（`crates/app/cli/wf-tui/src/interactive/mouse_hit.rs:198`）。
- 双击选词、三击选行已可用（`crates/app/cli/wf-tui/src/tui.rs:1040`、`crates/app/cli/wf-tui/src/tui.rs:1066`）。
- `handle_mouse_button` 单函数约 270 行，嵌套最深 4 层（`crates/app/cli/wf-tui/src/tui.rs:1091`）。
- `handle_mouse` 返回 `bool` 仅表示"是否滚动"（`crates/app/cli/wf-tui/src/tui.rs:986`）。
- composer（输入框）无鼠标光标定位能力。
- 右键、中键被静默忽略，无语义。
- 无 hover 高亮反馈。
- `wf-mini` 对所有非按键事件统一丢弃（`crates/app/cli/wf-mini/src/input.rs:546`）。

## 对比差异摘要

| 能力 | wf-agent | atomcode | opencode |
|------|----------|----------|----------|
| 默认开启 | 否 | 否 | 是 |
| composer 点击定位 | 无 | 有 | 有 |
| 拖拽文本选择 | 无 | 有（含 OSC52 复制） | 有（renderer 内置） |
| 右键语义 | 无 | 无 | 复制/Hover |
| Hover 反馈 | 无 | 有 | 有 |
| 列表点击导航 | 无 | 有 | 有 |
| 滚轮 | 有 | 有 | 有 |

## 改动项

### 一、拆分 handle_mouse_button 为三个独立函数

目标：降低单函数复杂度，使 press/release/drag 三条路径各自可独立测试。

改动：

- 提取 `handle_mouse_press(&mut self, mouse: MouseEvent) -> bool`：处理 Down 事件，记录 `MousePress`，执行 gutter 命中与 thumb 判定。
- 提取 `handle_mouse_release(&mut self, mouse: MouseEvent) -> bool`：处理 Up 事件，执行世代校验、徽标命中、多击分发、内容点击。
- 保留 `handle_mouse_drag(&mut self, mouse: MouseEvent) -> bool`：处理 Drag/Moved 事件，仅 gutter thumb 拖拽生效。
- 原 `handle_mouse_button` 变为薄分发器，按 `MouseEventKind` 路由到上述三个函数。

引用位置：

- `crates/app/cli/wf-tui/src/tui.rs:1091`（handle_mouse_button 入口）
- `crates/app/cli/wf-tui/src/tui.rs:1150`（MousePress 记录点）
- `crates/app/cli/wf-tui/src/tui.rs:1315`（Drag/Moved 分支起点）

测试：

- 每个子函数的纯逻辑部分提取为 `input_classify.rs` 中的可测函数，覆盖各分支。
- 现有 `press_matches_release`、`next_click_count` 测试保持通过。

回滚：恢复原单函数即回滚，三个子函数可独立合入。

### 二、handle_mouse 返回枚举替代 bool

目标：消除 `bool` 语义模糊，调用方按枚举值决定帧策略。

改动：

- 在 `tui/input_classify.rs` 定义 `MouseOutcome { Scrolled, Clicked, Ignored }`。
- `handle_mouse` 返回 `MouseOutcome`，调用方（`crates/app/cli/wf-tui/src/tui.rs:535`）按枚举值决定 `RedrawScope::Full` 或保持无帧。
- `Ignored` 不再触发焦点标记与 `last_active` 更新。

引用位置：

- `crates/app/cli/wf-tui/src/tui.rs:986`（handle_mouse 签名）
- `crates/app/cli/wf-tui/src/tui.rs:535`（Event::Mouse 调用点）
- `crates/app/cli/wf-tui/src/tui/input_classify.rs:65`（MouseAction 定义处）

测试：

- `MouseOutcome` 到 `RedrawScope` 的映射表单元测试。
- 确认 `Ignored` 路径不触发帧请求。

回滚：恢复 `bool` 返回即回滚。

### 三、composer 鼠标点击定位

目标：Interactive 屏幕的 composer 支持点击放置光标，与键盘输入对等。

改动：

- `interactive/render.rs` 渲染 composer 时记录光标格坐标到命中帧（新增 `HitRegion::Composer { col }` 语义）。
- `mouse_hit.rs` 新增 `composer_hit_test(scroll_area, composer_area, col, row) -> Option<usize>` 纯函数，返回目标字符索引。
- `handle_mouse_release` 在 composer 区域命中时调用 `session.set_composer_cursor(byte_index)`。
- 双击 composer 选词、三击选行复用 `copy_word_at` / `copy_line_at` 的文本提取逻辑。

引用位置：

- `crates/app/cli/wf-tui/src/interactive/mouse_hit.rs:79`（word_at_col 文本提取参考）
- `crates/app/cli/wf-tui/src/interactive/mouse_hit.rs:198`（handle_content_click 模式参考）
- `crates/app/cli/wf-tui/src/interactive/render.rs:689`（record_hit_frame 发布点）

测试：

- composer 区域命中/未命中边界。
- 双击选词结果与 `word_at_col` 一致性。

回滚：不发布 composer 命中区域即回滚。

### 四、拖拽文本选择（transcript drag-select）

目标：按住左键拖拽选择 transcript 文本，经现有复制通道输出。

改动：

- `MousePress` 新增 `transcript_anchor: Option<(u64, usize)>` 字段（seq + byte 偏移）。
- `handle_mouse_press` 在内容区按下时记录锚点。
- `handle_mouse_drag` 在内容区拖拽时计算当前覆盖范围，高亮选区。
- `handle_mouse_release` 在内容区抬起时提取选中文本，经 `pending_copy` 通道输出。
- 选区高亮渲染复用 ratatui `Span::styled` 反色，不引入新渲染路径。

引用位置：

- `crates/app/cli/wf-tui/src/tui.rs:75`（MousePress 结构体）
- `crates/app/cli/wf-tui/src/tui.rs:1040`（copy_word_at 文本提取参考）
- `crates/app/cli/wf-tui/src/interactive/mouse_hit.rs:111`（seq_at_display_row 锚点计算参考）

测试：

- 同世代内拖拽选区提取。
- 跨世代拖拽不触发复制。
- Shift+拖拽走原生选择不受影响。

回滚：不记录 `transcript_anchor` 即回滚，高亮随之下线。

### 五、右键复制语义

目标：右键单击内容区复制当前行，右键拖拽走原生选择。

改动：

- `input_classify.rs` 新增 `MouseAction::RightClick` 分类。
- `mouse_is_right_button(kind)` 纯函数。
- `handle_mouse_release` 在右键抬起且无拖拽时，提取当前行文本经 `pending_copy` 输出。
- 右键拖拽（Down→Drag→Up）保持 `IgnoredButton`，走终端原生选择。

引用位置：

- `crates/app/cli/wf-tui/src/tui/input_classify.rs:65`（MouseAction 枚举）
- `crates/app/cli/wf-tui/src/tui/input_classify.rs:141`（mouse_is_left_button 参考）
- `crates/app/cli/wf-tui/src/tui.rs:1066`（copy_line_at 提取参考）

测试：

- 右键单击复制当前行。
- 右键拖拽不触发应用内复制。
- 右键在 composer/gutter/徽标区域不生效。

回滚：右键恢复 `IgnoredButton` 即回滚。

### 六、wf-mini 保留滚轮事件

目标：mini 模式的列表导航支持滚轮，不引入完整鼠标捕获。

改动：

- `wf-mini/src/input.rs` 在事件匹配中新增 `Event::Mouse(mouse)` 分支，仅提取 `ScrollUp` / `ScrollDown`，映射为列表上下移动。
- 其他鼠标事件仍走 `Some(Ok(_))` 丢弃路径。
- 不启用鼠标捕获，不发送 `EnableMouseCapture` 序列。

引用位置：

- `crates/app/cli/wf-mini/src/input.rs:546`（当前统一丢弃点）
- `crates/app/cli/wf-tui/src/tui.rs:1361`（scroll_by_wheel 参考）

测试：

- 滚轮事件触发列表移动。
- 按钮事件不触发任何动作。

回滚：删除新增分支即回滚。

### 七、hover 高亮反馈（仅 Interactive 屏幕）

目标：鼠标悬停在可点击行时提供视觉反馈，不影响帧预算。

改动：

- `wf-tui` 事件循环在收到 `MouseEventKind::Moved` 时记录 hover 格到状态字段（不触发帧请求）。
- 渲染侧读取 hover 格，对命中的可点击行添加背景色样式。
- `RedrawScope::Full` 帧中才读取 hover 状态，hover 本身不触发 `FullFrameReason`。
- 滚动或尺寸变化时清除 hover 格。

引用位置：

- `crates/app/cli/wf-tui/src/tui.rs:535`（Moved 当前被忽略）
- `crates/app/cli/wf-tui/src/interactive/mouse_hit.rs:71`（ClickTarget 可命中行定义）
- `crates/app/cli/wf-tui/src/tui.rs:180`（mouse_press 字段旁的 hover 字段）

测试：

- Moved 事件不触发帧请求。
- hover 高亮与 ClickTarget 命中一致。
- 滚动后 hover 格清除。

回滚：移除 hover 字段与渲染侧读取即回滚。

## 执行顺序与验证

建议顺序：

1. 第二项（枚举语义）— 纯重构，零行为变化，作为后续改动基础。
2. 第一项（拆分函数）— 纯重构，验证行为等价。
3. 第五项（右键复制）— 独立行为，风险低。
4. 第六项（mini 滚轮）— 独立模块，风险低。
5. 第三项（composer 点击）— 需要 render 侧发布命中区。
6. 第四项（拖拽选择）— 依赖第一项拆分后的 drag 路径。
7. 第七项（hover 反馈）— 最后合入，避免帧预算回归。

每项验收标准：

- 现有 `cargo clippy --all-targets --all-features` 全量编译通过。
- 受影响模块的单元测试全过。
- 手动验证：开启鼠标捕获后各路径行为正确；关闭后原生选择无损。
- 帧预算无回归：hover 与 drag 路径不引入额外全帧。

最终全量验证收敛为一次完整编译检查与受影响单测。
