# Full 模式 CLI 视觉风格改进方案

## 1. 目标

将当前项目的 Full 模式 CLI 改进为类似 Codex 的视觉风格，主要改进点：

1. 采用极简颜色哲学，减少 RGB 颜色使用
2. 增强终端自适应能力
3. 改进动画效果
4. 优化宽度适应性

## 2. 核心改进点

### 2.0 架构调整：Agent 对话界面作为主界面

#### 当前实现
- 8 个独立屏幕 + 导航栈
- Session 是第 4 个屏幕，需要按数字键切换

#### 目标架构
- Agent 对话界面是唯一主界面
- 其他功能（Dashboard/工作流/执行等）作为覆盖层/弹窗切换

#### 改进方案

**步骤 1：修改默认屏幕**

在 `tui.rs` 中将默认屏幕改为 Session：

```rust
impl Screens {
    pub fn new() -> Self {
        Self {
            stack: vec![Screen::new(ScreenKind::Session)], // 改为 Session
            selected: 0,
        }
    }
}
```

**步骤 2：新增覆盖层系统**

在 `screens.rs` 中添加覆盖层管理：

```rust
pub enum OverlayKind {
    Sidebar(ScreenKind),  // 侧边栏显示其他屏幕
    Transcript,           // 转录覆盖层
    CommandPalette,       // 命令面板
}
```

**步骤 3：调整键盘映射**

在 `keymap.rs` 中添加新快捷键：

```rust
// Ctrl+T: 转录覆盖层
// Ctrl+B: 侧边栏
// /: 命令面板
```

**步骤 4：修改 Session 布局**

在 `session.rs` 中移除固定边框，成为全屏主界面：

```rust
fn draw_scrollback(&mut self, frame: &mut Frame, area: Rect) {
    // 移除 Block::default().title("Session...")
    // 直接渲染 scrollback 内容
}
```

### 2.1 颜色系统改进

#### 当前实现
- 使用 RGB 颜色定义主题（`Theme` 结构体）
- 固定调色板，不适应终端实际能力
- 缺少终端颜色探测

#### 改进方案

**步骤 1：实现终端颜色能力检测**

在 `theme.rs` 中添加终端颜色能力检测：

```rust
// 检测终端颜色能力
pub fn detect_color_level() -> ColorLevel {
    let colorterm = std::env::var("COLORTERM").ok();
    let term = std::env::var("TERM").ok();
    
    match colorterm.as_deref() {
        Some("truecolor") | Some("24bit") => ColorLevel::TrueColor,
        _ => {
            if term.map(|t| t.contains("256color")).unwrap_or(false) {
                ColorLevel::Ansi256
            } else {
                ColorLevel::Ansi16
            }
        }
    }
}

// OSC 10/11 探测
pub fn probe_terminal_colors() -> (Option<Rgb>, Option<Rgb>) {
    // 类似 codex 的 terminal_probe.rs 实现
}
```

**步骤 2：修改 Theme 结构体**

将 RGB 颜色改为支持 ANSI 颜色：

```rust
pub struct Theme {
    pub kind: ThemeKind,
    pub fg: ColorRole,  // 改为 ColorRole
    pub bg: ColorRole,
    pub muted: ColorRole,
    pub accent: ColorRole,
    pub add: ColorRole,
    pub remove: ColorRole,
    pub warning: ColorRole,
    pub error: ColorRole,
    pub highlight: ColorRole,
}

pub enum ColorRole {
    Ansi(AnsiColor),
    Rgb(Rgb),
}
```

**步骤 3：实现自适应样式**

参考 codex 的 `style.rs`，实现自适应样式：

```rust
pub fn accent_style() -> Style {
    let bg = default_bg();
    if is_light(bg) {
        Style::default().fg(best_color((0, 95, 135))).bold()
    } else {
        Style::default().fg(Color::Cyan).bold()
    }
}

pub fn user_message_style() -> Style {
    let bg = default_bg();
    match bg {
        Some(bg) => Style::default().fg(user_message_bg(bg)),
        None => Style::default(),
    }
}
```

### 2.2 动画效果改进

#### 当前实现
- 基础加载指示器
- 缺少时间依赖动画

#### 改进方案

**步骤 1：创建 motion 模块**

创建 `crates/app/wf-cli/src/motion.rs`：

```rust
pub enum MotionMode {
    Animated,
    Reduced,
}

pub fn activity_indicator(mode: MotionMode) -> String {
    match mode {
        MotionMode::Animated => shimmer_text("●"),
        MotionMode::Reduced => "●".to_string(),
    }
}

pub fn shimmer_text(text: &str) -> String {
    // 实现类似 codex 的扫光动画
    // 基于进程启动时间，使用余弦函数
}
```

**步骤 2：实现 shimmer 动画**

参考 codex 的 `shimmer.rs`，实现时间依赖的扫光动画：

```rust
pub struct Shimmer {
    start_time: Instant,
    highlight_color: Rgb,
    base_color: Rgb,
}

impl Shimmer {
    pub fn render(&self, text: &str, width: u16) -> Vec<Line<'static>> {
        let elapsed = self.start_time.elapsed().as_secs_f32();
        let period = 2.0; // 2秒周期
        let phase = (elapsed % period) / period;
        
        // 计算扫光位置
        let sweep_pos = phase * width as f32;
        
        // 为每个字符计算样式
        text.chars().enumerate().map(|(i, c)| {
            let distance = (i as f32 - sweep_pos).abs();
            let intensity = if distance < 5.0 {
                (1.0 - distance / 5.0) as f32
            } else {
                0.0
            };
            
            let style = if intensity > 0.5 {
                Style::default().fg(self.highlight_color).bold()
            } else if intensity > 0.0 {
                Style::default().fg(self.base_color)
            } else {
                Style::default().dim()
            };
            
            Line::from(Span::styled(c.to_string(), style))
        }).collect()
    }
}
```

### 2.3 宽度适应性改进

#### 当前实现
- 基础换行
- 缺少表格列分配
- 页脚提示未折叠

#### 改进方案

**步骤 1：实现宽度感知换行**

在 `scrollback.rs` 中添加宽度感知换行：

```rust
impl HistoryLine {
    pub fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        match self.state {
            LineState::UserMessage => {
                // 用户消息：保留 2 列左边距
                adaptive_wrap_lines(&self.text, width.saturating_sub(2), 2)
            }
            LineState::AssistantMessage => {
                // 助手消息：Markdown 渲染
                markdown_render(&self.text, width)
            }
            _ => {
                // 其他：简单换行
                word_wrap_lines(&self.text, width)
            }
        }
    }
}
```

**步骤 2：实现表格列分配**

参考 codex 的表格渲染，实现列分配算法：

```rust
pub struct TableRenderer {
    columns: Vec<Column>,
}

pub enum ColumnType {
    Narrative,
    TokenHeavy,
    Compact,
}

impl TableRenderer {
    pub fn render(&self, width: u16) -> Vec<Line<'static>> {
        // 1. 分类列类型
        // 2. 计算列宽
        // 3. 分配宽度
        // 4. 渲染行
    }
}
```

**步骤 3：实现页脚折叠**

参考 codex 的 `footer.rs`，实现单行折叠算法：

```rust
pub struct Footer {
    hints: Vec<FooterHint>,
}

impl Footer {
    pub fn render(&self, width: u16) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        let mut remaining_width = width;
        
        // 按优先级添加提示
        for hint in &self.hints {
            let hint_width = hint.width() as u16;
            if remaining_width >= hint_width {
                spans.push(hint.render());
                remaining_width -= hint_width;
            } else {
                break;
            }
        }
        
        spans
    }
}
```

### 2.4 历史单元改进

#### 当前实现
- `HistoryLine` 结构体
- 缺少宽度适应和动画信号

#### 改进方案

**步骤 1：定义 HistoryCell 特征**

创建 `crates/app/wf-cli/src/history_cell.rs`：

```rust
pub trait HistoryCell: Debug + Send + Sync + Any {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>>;
    fn raw_lines(&self) -> Vec<Line<'static>>;
    fn desired_height(&self, width: u16) -> u16;
    fn animation_tick(&self) -> Option<u64>;
}
```

**步骤 2：实现多种历史单元**

```rust
pub struct UserMessageCell {
    text: String,
    mentions: Vec<Mention>,
}

impl HistoryCell for UserMessageCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let style = user_message_style();
        adaptive_wrap_lines(&self.text, width.saturating_sub(2), 2)
            .into_iter()
            .map(|line| {
                Line::from(Span::styled(line.to_string(), style))
            })
            .collect()
    }
}

pub struct AssistantMessageCell {
    content: String,
}

impl HistoryCell for AssistantMessageCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        markdown_render(&self.content, width)
    }
}

pub struct ToolCallCell {
    tool_name: String,
    args: String,
    result: Option<String>,
}

impl HistoryCell for ToolCallCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        
        // 工具名称
        lines.push(Line::from(Span::styled(
            format!("▲ {}", self.tool_name),
            Style::default().fg(Color::Cyan),
        )));
        
        // 参数
        if !self.args.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("  {}", self.args),
                Style::default().dim(),
            )));
        }
        
        // 结果
        if let Some(result) = &self.result {
            lines.push(Line::from(Span::styled(
                format!("  ✓ {}", result),
                Style::default().fg(Color::Green),
            )));
        }
        
        lines
    }
}
```

### 2.5 输入组件改进

#### 当前实现
- 单行 `Composer`
- 缺少多行支持

#### 改进方案

**步骤 1：升级为多行 ChatComposer**

修改 `composer.rs`，支持多行输入：

```rust
pub struct ChatComposer {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    history: Vec<String>,
    history_index: Option<usize>,
}

impl ChatComposer {
    pub fn insert_char(&mut self, c: char) {
        // 支持多行输入
        if c == '\n' {
            self.lines.insert(self.cursor_row + 1, String::new());
            self.cursor_row += 1;
            self.cursor_col = 0;
        } else {
            let line = &mut self.lines[self.cursor_row];
            line.insert(self.cursor_col, c);
            self.cursor_col += 1;
        }
    }
    
    pub fn render(&self, width: u16) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        
        for (i, line) in self.lines.iter().enumerate() {
            let prompt = if i == 0 { "> " } else { "  " };
            let style = if i == self.cursor_row {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            
            lines.push(Line::from(vec![
                Span::styled(prompt, style),
                Span::styled(line.clone(), style),
            ]));
        }
        
        lines
    }
}
```

## 3. 实施计划

### 阶段一：基础改进（1-2 周）

1. **重构主题系统**
   - 实现终端颜色能力检测
   - 支持 OSC 10/11 探测
   - 实现三级颜色降级

2. **改进颜色使用**
   - 将 RGB 颜色替换为 ANSI 颜色
   - 实现自适应样式
   - 减少 RGB 使用

3. **增强动画效果**
   - 实现 `shimmer` 动画
   - 创建 `motion` 模块
   - 支持时间依赖动画

### 阶段二：架构改进（2-3 周）

1. **重构历史管理**
   - 定义 `HistoryCell` 特征
   - 实现多种历史单元类型
   - 支持宽度适应和动画信号

2. **增强输入组件**
   - 升级为多行 `ChatComposer`
   - 支持粘贴检测
   - 实现历史搜索

3. **改进宽度适应**
   - 实现宽度感知换行
   - 实现表格列分配
   - 实现页脚折叠

### 阶段三：细节优化（1-2 周）

1. **改进 Markdown 渲染**
   - 实现丰富的内联样式
   - 支持语法高亮
   - 实现宽度自适应表格

2. **增强底部面板**
   - 实现 `Footer` 组件
   - 支持单行折叠
   - 支持审批覆盖层

3. **优化性能**
   - 实现缓存机制
   - 优化渲染性能
   - 改进宽度适应算法

## 4. 关键文件

| 文件 | 职责 | 改进内容 |
|------|------|---------|
| `theme.rs` | 主题系统 | 终端探测、颜色降级 |
| `style.rs`（新建） | 样式系统 | 自适应样式、颜色混合 |
| `motion.rs`（新建） | 动画系统 | shimmer 动画、运动模式 |
| `history_cell.rs`（新建） | 历史单元 | HistoryCell 特征、多种实现 |
| `scrollback.rs` | 滚动历史 | 宽度适应换行 |
| `composer.rs` | 输入编辑器 | 多行支持、粘贴检测 |
| `footer.rs` | 页脚 | 单行折叠算法 |
| `markdown.rs` | Markdown | 丰富的内联样式 |

## 5. 总结

通过借鉴 Codex 的设计理念，我们可以将当前项目的 Full 模式 CLI 改进为更专业、更一致的视觉风格。关键改进点包括：

1. **极简颜色哲学**：减少 RGB 使用，优先 ANSI 颜色
2. **终端自适应**：动态检测终端能力，优雅降级
3. **动画系统**：时间依赖的扫光动画
4. **宽度适应**：深度自适应算法
5. **丰富的组件**：HistoryCell、BottomPane、ChatComposer 等

建议按照分阶段实施，先从基础改进开始，逐步过渡到架构改进和细节优化。这样可以降低风险，确保每个阶段都能交付可用的改进。
