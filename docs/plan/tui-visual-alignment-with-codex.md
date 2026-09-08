# TUI 视觉效果与 Codex 对齐设计方案

## 1. 概述

本文档详细描述了如何将当前项目 TUI 的视觉效果与 Codex TUI 对齐。基于对 `ref/codex` 目录下 Codex TUI 实现的分析，我们识别了当前项目在视觉风格方面的主要差异，并制定了具体的修改方案。

**对齐策略：**
- **颜色风格**：默认ANSI可选RGB - 默认使用ANSI颜色，但保留用户自定义RGB的能力
- **自适应能力**：增强但可配置 - 增强自适应能力，但允许用户通过配置文件覆盖
- **动画效果**：先实现核心组件动画。剩余组件作为可选任务，给出专门的文档说明codex对应实现的位置和具体实现，后续再考虑

## 2. 当前项目 TUI 视觉效果问题分析

### 2.1 颜色哲学差异

**Codex TUI 特点：**
- 极简主义：保守使用颜色，避免自定义 RGB（除 `shimmer.rs` 外）
- 主色调：默认终端前景
- 次要文本：`dim`
- 用户输入提示、选择、状态指示：ANSI `cyan`
- 成功/增加：ANSI `green`
- 错误/失败/删除：ANSI `red`
- 品牌色：ANSI `magenta`
- 严格避免：自定义 RGB、ANSI `black/white/blue/yellow` 作为前景色

**当前项目问题：**
- 使用丰富的 RGB 颜色（如 `theme.rs` 中的默认调色板）
- 颜色选择不够保守，可能在某些终端上显示效果不佳

### 2.2 终端自适应能力

**Codex TUI 特点：**
- 详细的终端探测和颜色能力检测（`StdoutColorLevel`）
- 支持 `TrueColor`、`Ansi256`、`Ansi16` 三级降级
- `best_color()` 根据终端能力选择最近颜色
- Windows Terminal 特殊处理：提升到 TrueColor

**当前项目问题：**
- 虽然有终端探测，但没有根据终端能力调整颜色
- 缺少颜色降级机制

### 2.3 亮/暗背景适应

**Codex TUI 特点：**
- 自动亮/暗背景检测：加权亮度公式 `Y = 0.299R + 0.587G + 0.114B`
- 自适应样式生成：
  - `accent_style()`：亮背景用深青色，暗背景用青色
  - `user_message_style()`：微妙背景混合（暗：12% 白，亮：4% 黑）
  - `table_separator_style()`：20% 透明度混合前景/背景

**当前项目问题：**
- 有主题探测，但样式生成不够自适应
- 缺少背景混合效果

### 2.4 Markdown 渲染样式

**Codex TUI 特点：**
- 丰富的内联样式：
  - H1：粗体 + 下划线
  - H2：粗体
  - H3：粗体 + 斜体
  - 代码块：cyan
  - 链接：cyan + 下划线
  - 引用：green

**当前项目问题：**
- 没有详细的 Markdown 渲染样式
- 缺少语法高亮支持

### 2.5 动画效果

**Codex TUI 特点：**
- 基于时间的扫光动画（2秒周期）
- 真彩色终端使用 RGB 插值
- 低色彩终端使用三级回退（dim/normal/bold）

**当前项目问题：**
- 只有基础的 spinner 动画
- 缺少时间依赖的动画效果

### 2.6 组件视觉效果

**Codex TUI 特点：**
- 丰富的组件系统：HistoryCell、BottomPane、ChatComposer 等
- 内联视口 + 覆盖层，避免备用屏幕限制
- 分层状态机架构

**当前项目问题：**
- 组件相对简单，缺少 HistoryCell 特征
- 使用全屏备用屏幕，可能限制某些终端特性

## 3. 与 Codex 对齐的修改方案

### 3.1 颜色哲学调整

**目标：** 默认使用ANSI颜色，但保留用户自定义RGB的能力

**具体修改：**

1. **修改 `Theme` 数据结构**
   ```rust
   // 当前
   pub struct Theme {
       pub kind: ThemeKind,
       pub fg: Rgb,
       pub bg: Rgb,
       pub muted: Rgb,
       pub accent: Rgb,
       pub add: Rgb,
       pub remove: Rgb,
       pub warning: Rgb,
       pub error: Rgb,
       pub highlight: Rgb,
       pub source: ThemeSource,
   }
   
   // 修改为
   pub struct Theme {
       pub kind: ThemeKind,
       pub fg: ColorRole,      // 支持 ANSI 或 RGB
       pub bg: ColorRole,
       pub muted: ColorRole,
       pub accent: ColorRole,
       pub add: ColorRole,
       pub remove: ColorRole,
       pub warning: ColorRole,
       pub error: ColorRole,
       pub highlight: ColorRole,
       pub source: ThemeSource,
   }
   
   pub enum ColorRole {
       Ansi(AnsiColor),        // ANSI 颜色（默认）
       Rgb(Rgb),               // 自定义 RGB（可选）
   }
   ```

2. **更新默认调色板**
   - 将 `accent` 改为 ANSI `cyan`
   - 将 `add` 改为 ANSI `green`
   - 将 `remove` 改为 ANSI `red`
   - 将 `warning` 改为 ANSI `yellow`
   - 将 `highlight` 改为 ANSI `magenta`

3. **保留用户自定义RGB的能力**
   - 用户可以通过配置文件覆盖默认ANSI颜色
   - 支持在主题文件中使用RGB值
   - 提供回退机制：如果用户指定RGB，使用RGB；否则使用默认ANSI

**实施步骤：**
1. 创建 `ColorRole` 枚举
2. 修改 `Theme` 数据结构
3. 更新所有使用 `Theme` 的代码
4. 更新默认调色板
5. 实现配置文件解析，支持用户自定义颜色

### 3.2 终端自适应能力增强

**目标：** 增强自适应能力，但允许用户通过配置文件覆盖

**具体修改：**

1. **创建终端能力检测模块**
   ```rust
   // terminal_probe.rs
   pub enum StdoutColorLevel {
       TrueColor,
       Ansi256,
       Ansi16,
       Unknown,
   }
   
   impl StdoutColorLevel {
       pub fn detect() -> Self {
           // 检测 COLORTERM / TERM 环境变量
       }
       
       pub fn best_color(&self, rgb: Rgb) -> ColorRole {
           // 根据终端能力选择最佳颜色
       }
   }
   ```

2. **增强 `probe_theme` 函数**
   ```rust
   pub fn probe_theme() -> Theme {
       let color_level = StdoutColorLevel::detect();
       // 根据终端能力调整颜色
       // ...
   }
   ```

3. **添加用户配置覆盖**
   ```rust
   pub fn probe_theme_with_config(config: &UserConfig) -> Theme {
       let mut theme = probe_theme();
       // 用户配置覆盖默认值
       if let Some(custom_accent) = config.accent_color {
           theme.accent = ColorRole::Rgb(custom_accent);
       }
       // ...
       theme
   }
   ```

4. **缓存探测结果**
   - 将终端能力缓存到文件中
   - 避免重复探测

**实施步骤：**
1. 创建 `terminal_probe.rs` 模块
2. 实现 `StdoutColorLevel` 枚举和检测逻辑
3. 修改 `probe_theme` 函数
4. 实现用户配置覆盖机制
5. 添加缓存机制

### 3.3 亮/暗背景适应改进

**目标：** 增强自适应样式生成，但允许用户通过配置文件覆盖

**具体修改：**

1. **增强背景检测**
   ```rust
   // style.rs
   pub fn is_light(bg: Rgb) -> bool {
       luminance(bg) > DARK_LUMINANCE_THRESHOLD
   }
   
   pub fn blend(top: Rgb, bottom: Rgb, alpha: f32) -> Rgb {
       // Alpha 混合两颜色
   }
   ```

2. **实现自适应样式生成**
   ```rust
   pub fn accent_style(bg: Rgb, config: &UserConfig) -> Style {
       // 用户配置优先
       if let Some(custom_style) = config.accent_style {
           return custom_style;
       }
       
       // 默认自适应
       if is_light(bg) {
           // 亮背景用深青色
           to_bold_style(Rgb::new(0x00, 0x5F, 0x87))
       } else {
           // 暗背景用青色
           to_bold_style(Rgb::new(0x22, 0xD3, 0xEE))
       }
   }
   
   pub fn user_message_style(bg: Rgb, config: &UserConfig) -> Style {
       // 用户配置优先
       if let Some(custom_style) = config.user_message_style {
           return custom_style;
       }
       
       // 默认自适应
       let blended = if is_light(bg) {
           blend(Rgb::new(0, 0, 0), bg, 0.04)
       } else {
           blend(Rgb::new(255, 255, 255), bg, 0.12)
       };
       Style::default().bg(to_ratatui_color(blended))
   }
   ```

3. **改进表格分隔线样式**
   ```rust
   pub fn table_separator_style(fg: Rgb, bg: Rgb, config: &UserConfig) -> Style {
       // 用户配置优先
       if let Some(custom_style) = config.table_separator_style {
           return custom_style;
       }
       
       // 默认自适应
       let blended = blend(fg, bg, 0.20);
       to_dim_style(blended)
   }
   ```

**实施步骤：**
1. 增强 `is_light` 函数
2. 实现 `blend` 函数
3. 创建 `style.rs` 模块
4. 实现自适应样式生成函数
5. 添加用户配置覆盖机制
6. 更新所有使用样式的代码

### 3.4 Markdown 渲染样式增强

**目标：** 实现丰富的内联样式和语法高亮

**具体修改：**

1. **创建 Markdown 渲染模块**
   ```rust
   // markdown_render.rs
   pub fn render_markdown(text: &str, theme: &Theme) -> Vec<Line<'static>> {
       // 解析 Markdown 并应用样式
   }
   
   fn apply_heading_style(level: u8, theme: &Theme) -> Style {
       match level {
           1 => Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
           2 => Style::default().add_modifier(Modifier::BOLD),
           3 => Style::default().add_modifier(Modifier::BOLD | Modifier::ITALIC),
           _ => Style::default(),
       }
   }
   
   fn apply_code_style(theme: &Theme) -> Style {
       to_style(theme.accent) // cyan
   }
   
   fn apply_link_style(theme: &Theme) -> Style {
       to_style(theme.accent).add_modifier(Modifier::UNDERLINED)
   }
   
   fn apply_quote_style(theme: &Theme) -> Style {
       to_style(theme.add) // green
   }
   ```

2. **集成到历史记录渲染**
   ```rust
   // transcript.rs
   impl HistoryLine {
       pub fn display_lines_with_markdown(&self, width: u16, theme: &Theme) -> Vec<Line<'static>> {
           // 使用 Markdown 渲染器
       }
   }
   ```

**实施步骤：**
1. 创建 `markdown_render.rs` 模块
2. 实现 Markdown 解析和样式应用
3. 集成到 `HistoryLine` 的渲染逻辑
4. 添加语法高亮支持

### 3.5 动画效果增强

**目标：** 先实现核心组件动画。剩余组件作为可选任务，给出专门的文档说明codex对应实现的位置和具体实现，后续再考虑。

**核心组件动画（必须实现）：**

1. **任务执行过程中的动画**
   ```rust
   // motion.rs
   pub enum MotionMode {
       Animated,
       Reduced,
   }
   
   pub struct Motion {
       mode: MotionMode,
       tick: u64,
   }
   
   impl Motion {
       pub fn new() -> Self {
           Self {
               mode: MotionMode::Animated,
               tick: 0,
           }
       }
       
       pub fn tick(&mut self) {
           self.tick += 1;
       }
       
       pub fn shimmer_intensity(&self) -> f32 {
           // 2秒周期的扫光动画
           let phase = (self.tick as f32 / 60.0) % 2.0; // 60 FPS
           (phase * std::f32::consts::PI).sin()
       }
   }
   ```

2. **任务执行状态指示器动画**
   ```rust
   // status_line.rs
   pub fn render_status_indicator(motion: &Motion) -> Span<'static> {
       let intensity = motion.shimmer_intensity();
       // 应用扫光效果到任务执行状态
   }
   ```

3. **加载指示器动画**
   ```rust
   // spinner.rs
   pub fn render_loading_indicator(motion: &Motion) -> Span<'static> {
       // 基于时间的旋转动画
   }
   ```

**可选任务（后续实现）：**

以下动画效果作为可选任务，需要单独的文档说明codex对应实现的位置和具体实现：

1. **Markdown渲染动画**
   - Codex位置：`ref/codex/tui/src/render/markdown_render.rs`
   - 实现：语法高亮动画、代码块动画

2. **历史记录单元动画**
   - Codex位置：`ref/codex/tui/src/history_cell.rs`
   - 实现：HistoryCell的动画信号支持

3. **底部面板动画**
   - Codex位置：`ref/codex/tui/src/bottom_pane/`
   - 实现：状态指示器动画、审批覆盖层动画

4. **转录覆盖层动画**
   - Codex位置：`ref/codex/tui/src/transcript.rs`
   - 实现：时间相关输出的重新渲染

**实施步骤：**
1. 创建 `motion.rs` 模块
2. 实现任务执行过程中的动画
3. 实现任务执行状态指示器动画
4. 实现加载指示器动画
5. 创建可选任务文档，详细说明codex对应实现位置

### 3.6 组件视觉效果改进

**目标：** 实现类似 Codex 的组件系统

**具体修改：**

1. **定义 HistoryCell 特征**
   ```rust
   // history_cell.rs
   pub trait HistoryCell {
       fn display_lines(&self, width: u16) -> Vec<Line<'static>>;
       fn raw_lines(&self, width: u16) -> Vec<String>;
       fn transcript_lines(&self, width: u16) -> Vec<Line<'static>>;
       fn desired_height(&self, width: u16) -> u16;
       fn transcript_animation_tick(&mut self);
   }
   ```

2. **实现多种历史单元类型**
   ```rust
   pub struct UserHistoryCell {
       text: String,
   }
   
   pub struct AssistantHistoryCell {
       text: String,
       rendered: Option<Vec<Line<'static>>>,
   }
   
   pub struct ToolCallHistoryCell {
       tool_name: String,
       arguments: String,
       result: Option<String>,
   }
   ```

3. **改进底部面板系统**
   ```rust
   // bottom_pane/
   pub struct BottomPane {
       composer: ChatComposer,
       footer: Footer,
       approval_overlay: Option<ApprovalOverlay>,
       selection_view: Option<SelectionView>,
       status_indicator: StatusIndicatorWidget,
   }
   ```

**实施步骤：**
1. 创建 `history_cell.rs` 模块
2. 实现 `HistoryCell` 特征
3. 创建多种历史单元类型
4. 改进底部面板系统

## 4. 实施计划

### 4.1 第一阶段：基础改进（1-2 周）

1. **重构主题系统**
   - 实现 `ColorRole` 枚举
   - 修改 `Theme` 数据结构
   - 更新默认调色板
   - 实现用户自定义颜色配置

2. **实现终端能力检测**
   - 创建 `terminal_probe.rs` 模块
   - 实现 `StdoutColorLevel` 枚举
   - 添加缓存机制
   - 实现用户配置覆盖

3. **改进亮/暗背景适应**
   - 增强背景检测
   - 实现自适应样式生成
   - 添加用户配置覆盖

### 4.2 第二阶段：组件改进（2-3 周）

1. **增强 Markdown 渲染**
   - 创建 `markdown_render.rs` 模块
   - 实现丰富的内联样式
   - 集成到历史记录渲染

2. **实现核心组件动画**
   - 创建 `motion.rs` 模块
   - 实现任务执行过程中的动画
   - 实现任务执行状态指示器动画
   - 实现加载指示器动画

3. **改进组件系统**
   - 定义 `HistoryCell` 特征
   - 实现多种历史单元类型
   - 改进底部面板系统

### 4.3 第三阶段：细节优化（1-2 周）

1. **性能优化**
   - 实现缓存机制
   - 优化渲染性能
   - 改进宽度适应算法

2. **兼容性测试**
   - 测试不同终端
   - 测试亮/暗主题
   - 测试不同颜色能力

3. **创建可选任务文档**
   - 详细说明codex对应实现位置
   - 给出具体实现指导
   - 作为后续实现的参考

## 5. 预期效果

通过以上修改，当前项目的 TUI 视觉效果将与 Codex TUI 对齐：

1. **更保守的颜色使用**：默认使用ANSI颜色，保留用户自定义RGB能力
2. **更好的终端兼容性**：根据终端能力调整颜色，支持用户配置覆盖
3. **更自适应的样式**：根据背景类型自动调整颜色方案，支持用户配置覆盖
4. **更丰富的 Markdown 渲染**：支持内联样式和语法高亮
5. **更流畅的动画效果**：核心组件的时间依赖动画
6. **更模块化的组件系统**：类似 Codex 的 HistoryCell 特征

## 6. 总结

本方案详细描述了如何将当前项目 TUI 的视觉效果与 Codex TUI 对齐。通过颜色哲学调整、终端自适应能力增强、亮/暗背景适应改进、Markdown 渲染样式增强、动画效果增强和组件视觉效果改进，当前项目的 TUI 将获得更专业、更一致的视觉体验。

建议按照分阶段实施，先从基础改进开始，逐步过渡到组件改进和细节优化。这样可以降低风险，确保每个阶段都能交付可用的改进。

**可选任务说明：**
- 剩余的动画效果（Markdown渲染动画、历史记录单元动画、底部面板动画、转录覆盖层动画）作为可选任务
- 需要创建专门的文档，详细说明codex对应实现的位置和具体实现
- 作为后续实现的参考，不纳入本次实施计划