# Codex TUI 与当前项目 Full 模式 CLI 对比分析

## 1. 概述

本文档对比分析了 `ref/codex` 目录下的 Codex TUI 实现与当前项目 `wf-agent` 的 Full 模式 CLI，旨在为视觉风格改进提供参考。

## 2. Codex TUI 架构分析

### 2.1 核心架构

```
Tui (终端生命周期管理)
  → App (应用状态机 + 事件循环)
    → ChatWidget (会话级 UI 状态 + 交互状态机)
      → BottomPane (输入框 + 瞬态视图栈)
        → ChatComposer (可编辑输入)
        → Footer (提示、状态行、上下文)
        → ApprovalOverlay (权限提示)
        → SelectionView (列表选择器)
        → StatusIndicatorWidget (加载指示器)
      → TranscriptState (历史单元、流式、覆盖层)
      → HistoryCell (可渲染对话单元)
```

### 2.2 视觉风格系统

#### 2.2.1 颜色哲学 (styles.md)

- **极简主义**：保守使用颜色，避免自定义 RGB（除 `shimmer.rs` 外）
- **主色调**：默认终端前景
- **次要文本**：`dim`
- **用户输入提示、选择、状态指示**：ANSI `cyan`
- **成功/增加**：ANSI `green`
- **错误/失败/删除**：ANSI `red`
- **品牌色**：ANSI `magenta`
- **严格避免**：自定义 RGB、ANSI `black/white/blue/yellow` 作为前景色

#### 2.2.2 终端自适应颜色系统 (terminal_palette.rs)

- **`StdoutColorLevel`**：支持 `TrueColor`、`Ansi256`、`Ansi16`、`Unknown`
- **`best_color()`**：根据终端能力选择最近颜色
- **终端探测**：启动时通过 OSC 10/11 查询终端实际颜色
- **Windows Terminal 特殊处理**：提升到 TrueColor

#### 2.2.3 亮/暗背景检测 (color.rs)

- **`is_light()`**：加权亮度公式 `Y = 0.299R + 0.587G + 0.114B`
- **`blend()`**：Alpha 混合两颜色
- **`perceptual_distance()`**：CIE76 Lab 空间距离

#### 2.2.4 自适应样式生成 (style.rs)

- **`accent_style()`**：亮背景用深青色，暗背景用青色
- **`user_message_style()`**：微妙背景混合（暗：12% 白，亮：4% 黑）
- **`table_separator_style()`**：20% 透明度混合前景/背景

#### 2.2.5 Markdown 渲染样式 (markdown_render.rs)

| 元素 | 样式 |
|------|------|
| H1 | 粗体 + 下划线 |
| H2 | 粗体 |
| H3 | 粗体 + 斜体 |
| 代码块 | cyan |
| 链接 | cyan + 下划线 |
| 引用 | green |

#### 2.2.6 动画效果 (shimmer.rs)

- 基于时间的扫光动画（2秒周期）
- 真彩色终端使用 RGB 插值
- 低色彩终端使用三级回退（dim/normal/bold）

### 2.3 关键组件

#### 2.3.1 HistoryCell 特征

对话显示的基本单元，支持：
- `display_lines()`：显示行
- `raw_lines()`：原始行（用于复制）
- `transcript_lines()`：转录行（用于覆盖层）
- `desired_height()`：期望高度
- `transcript_animation_tick()`：动画信号

#### 2.3.2 内联视口

- 主 UI 占据终端底部可变高度区域
- 历史滚动到上方正常回滚区
- 避免备用屏幕限制，同时支持全屏覆盖层

#### 2.3.3 底部面板 (bottom_pane/)

- **ChatComposer**：多行输入、粘贴突发检测、斜杠命令、历史搜索
- **Footer**：模式相关提示，单行折叠算法
- **ApprovalOverlay**：模态权限提示
- **SelectionView**：通用列表选择器
- **StatusIndicatorWidget**：动画加载指示器

#### 2.3.4 转录覆盖层

- Ctrl+T 触发全屏覆盖层
- 缓存机制：`ActiveCellTranscriptKey`（修订 + 动画 tick）
- 支持时间相关输出的重新渲染

### 2.4 设计特点

1. **终端优先设计**：所有颜色决策跨 TrueColor/256/16 色终端优雅降级
2. **分层渲染**：内联视口 + 覆盖层，避免备用屏幕限制
3. **状态机架构**：底部面板、流式生命周期、速率限制均为显式状态机
4. **缓存感知设计**：转录、语法高亮、终端颜色均有针对性缓存
5. **宽度自适应**：表格列分配、页脚提示折叠、用户消息换行均适应终端宽度
6. **平台抽象**：Crossterm 后端，特殊处理 Windows Terminal、Zellij、tmux 等

## 3. 当前项目 Full 模式 CLI 架构分析

### 3.1 核心架构

```
TuiApp
├── adapter: Arc<DomainAdapter>     // 运行时适配器
├── screens: Screens                // 8屏导航栈
├── modals: ModalStack              // 模态弹窗栈
├── data: HashMap<ScreenKind, (ScreenData, Instant)>  // 数据缓存
├── inflight: HashMap<ScreenKind, Instant>             // 进行中的获取
├── data_tx/rx: mpsc channels       // 后台数据获取通道
├── session: Option<SessionController>  // 活跃的交互会话
├── search_input: String            // 搜索屏幕的查询文本
├── exec_filter: ExecStatusFilter   // 执行列表的过滤器
├── notice: Option<(String, Instant)>  // 临时通知行
├── frame: FrameRequester           // 帧率限制器(120 FPS)
├── resize: ResizeDebouncer         // resize 防抖
└── theme: Theme                    // 实时主题
```

### 3.2 八个屏幕

| 编号 | 屏幕 | 内容 | 边框颜色 |
|------|------|------|---------|
| 1 | Dashboard | 聚合计数 + 最近执行 | Cyan |
| 2 | Workflow | 工作流列表 | Green |
| 3 | Executions | 执行记录列表 | Yellow |
| 4 | Session | 交互式代理对话界面 | Magenta |
| 5 | Checkpoints | 检查点列表 | Blue |
| 6 | Search | 全文搜索 | Cyan |
| 7 | Settings | LLM 配置 + 主题信息 | White |
| 8 | Help | 帮助文本 | Yellow |

### 3.3 Session 屏幕布局

```
┌──────────────────────────────────────┐
│  Session (Ctrl-C twice to exit)      │  ← 标题栏(紫色边框)
│                                      │
│  scrollback 区域                     │  ← 历史消息 + 流式输出
│  (包含 HistoryLine 的 display_lines) │     (tail-follow，可向上滚动)
│                                      │
│  Streaming tail line                 │  ← 正在流式接收的未完成行
├──────────────────────────────────────┤
│  Footer 区域(4行)                    │  ← Phase/工具/错误状态
│  (Footer::draw)                      │     Approval/Question 视图
├─────────────────────────────────────┤
│ > 用户输入                           │  ← 单行输入框(Composer)
└─────────────────────────────────────┘
```

### 3.4 视觉风格系统

#### 3.4.1 主题探测管道

优先级：用户配置文件 > OSC 10/11 实时探测 > 缓存文件 > 内置默认

#### 3.4.2 主题数据模型

```rust
struct Theme {
    kind: ThemeKind,     // Dark / Light
    fg: Rgb,             // 默认文本色
    bg: Rgb,             // 画布背景
    muted: Rgb,          // 次要文本
    accent: Rgb,         // 品牌/强调色
    add: Rgb,            // 增加(diff +, 成功)
    remove: Rgb,         // 减少(diff -)
    warning: Rgb,        // 警告
    error: Rgb,          // 错误
    highlight: Rgb,      // 高亮/选择
}
```

#### 3.4.3 默认暗色调色板

- 背景：`#0F141A`（深灰蓝）
- 前景：`#E5E7EB`（浅灰）
- Accent：`#22D3EE`（青色）
- Success：`#4A DE 80`（绿色）
- Error：`#F87171`（红色）
- Warning：`#FA CC 15`（黄色）
- Highlight：`#60 A5 FA`（蓝色）

## 4. 关键差异对比

### 4.1 设计理念差异

| 方面 | Codex TUI | 当前项目 Full CLI |
|------|-----------|------------------|
| **定位** | 单一代理对话界面 | 管理平台，多屏幕覆盖全生命周期 |
| **复杂度** | 专注对话体验 | 8个独立屏幕 + 深度导航 |
| **视觉风格** | 极简主义，ANSI 颜色优先 | 更丰富的 RGB 颜色 |
| **动画** | 复杂的 shimmer 动画 | 基础加载指示器 |
| **宽度适应** | 深度自适应（表格列分配、页脚折叠） | 基础自适应 |

### 4.2 架构差异

| 方面 | Codex TUI | 当前项目 Full CLI |
|------|-----------|------------------|
| **状态管理** | 分层状态机（App → ChatWidget → BottomPane） | 扁平化状态（TuiApp 直接管理） |
| **渲染模型** | 内联视口 + 覆盖层 | 全屏备用屏幕 |
| **历史管理** | HistoryCell 特征 + 多种实现 | HistoryLine 结构体 |
| **输入处理** | 多行 ChatComposer | 单行 Composer |
| **动画系统** | 集中式 motion 模块 | 分散式实现 |

### 4.3 视觉风格差异

| 方面 | Codex TUI | 当前项目 Full CLI |
|------|-----------|------------------|
| **颜色使用** | 极简，ANSI 颜色优先 | 更丰富，RGB 颜色 |
| **背景检测** | 自适应亮/暗背景 | 主题预设 |
| **终端能力** | 动态检测并适配 | 固定调色板 |
| **Markdown 样式** | 丰富的内联样式 | 基础样式 |
| **动画效果** | 时间扫光动画 | 基础加载指示器 |

## 5. 改进建议

### 5.1 视觉风格改进

#### 5.1.1 采用极简颜色哲学

- **建议**：借鉴 Codex 的极简主义，减少 RGB 颜色使用
- **实施**：
  - 将 `Theme` 中的 `accent`、`add`、`remove` 等改为 ANSI 颜色
  - 保留 RGB 仅用于特殊效果（如动画）
  - 优先使用 `cyan`、`green`、`red`、`magenta` 等 ANSI 颜色

#### 5.1.2 增强终端自适应能力

- **建议**：实现类似 Codex 的终端颜色检测系统
- **实施**：
  - 启动时通过 OSC 10/11 探测终端实际颜色
  - 支持 `TrueColor`、`Ansi256`、`Ansi16` 三级降级
  - 缓存探测结果，避免重复探测

#### 5.1.3 改进亮/暗背景适应

- **建议**：实现自动亮/暗背景检测
- **实施**：
  - 使用加权亮度公式检测背景
  - 根据背景类型自动调整颜色方案
  - 支持用户覆盖检测结果

### 5.2 架构改进

#### 5.2.1 重构状态管理

- **建议**：采用分层状态机架构
- **实施**：
  - 将 `SessionController` 重构为类似 `ChatWidget` 的状态机
  - 实现 `BottomPane` 等子组件，分离关注点
  - 使用显式状态机管理流式生命周期

#### 5.2.2 改进历史管理

- **建议**：引入 `HistoryCell` 特征
- **实施**：
  - 定义 `HistoryCell` 特征，支持不同类型的对话单元
  - 实现 `UserHistoryCell`、`AssistantHistoryCell`、`ToolCallHistoryCell` 等
  - 支持宽度自适应和动画信号

#### 5.2.3 增强动画系统

- **建议**：实现集中式动画系统
- **实施**：
  - 创建 `motion` 模块，统一管理动画
  - 实现 `MotionMode::Animated` 和 `MotionMode::Reduced`
  - 支持时间依赖的动画信号

### 5.3 组件改进

#### 5.3.1 增强输入组件

- **建议**：将单行 `Composer` 升级为多行 `ChatComposer`
- **实施**：
  - 支持多行输入
  - 实现粘贴突发检测
  - 支持斜杠命令和历史搜索
  - 实现 grapheme 感知光标

#### 5.3.2 改进底部面板

- **建议**：实现类似 Codex 的底部面板系统
- **实施**：
  - 实现 `Footer` 组件，支持模式相关提示
  - 实现单行折叠算法，适应终端宽度
  - 支持审批覆盖层和选择视图

#### 5.3.3 增强 Markdown 渲染

- **建议**：借鉴 Codex 的 Markdown 渲染样式
- **实施**：
  - 实现丰富的内联样式（H1-H6、代码、链接等）
  - 支持语法高亮
  - 实现宽度自适应表格渲染

### 5.4 具体实施步骤

#### 第一阶段：基础改进（1-2 周）

1. **重构主题系统**
   - 实现终端颜色探测（OSC 10/11）
   - 支持三级颜色降级（TrueColor/Ansi256/Ansi16）
   - 实现自动亮/暗背景检测

2. **改进颜色使用**
   - 将 `Theme` 中的 RGB 颜色替换为 ANSI 颜色
   - 实现 `accent_style()`、`user_message_style()` 等自适应样式
   - 减少 RGB 使用，仅保留特殊效果

3. **增强动画效果**
   - 实现 `shimmer` 动画
   - 创建 `motion` 模块
   - 支持时间依赖的动画信号

#### 第二阶段：架构改进（2-3 周）

1. **重构状态管理**
   - 实现分层状态机架构
   - 将 `SessionController` 重构为 `ChatWidget` 风格
   - 实现 `BottomPane` 等子组件

2. **改进历史管理**
   - 定义 `HistoryCell` 特征
   - 实现多种历史单元类型
   - 支持宽度自适应和动画信号

3. **增强输入组件**
   - 将 `Composer` 升级为 `ChatComposer`
   - 支持多行输入和粘贴检测
   - 实现斜杠命令和历史搜索

#### 第三阶段：细节优化（1-2 周）

1. **改进 Markdown 渲染**
   - 实现丰富的内联样式
   - 支持语法高亮
   - 实现宽度自适应表格渲染

2. **增强底部面板**
   - 实现 `Footer` 组件
   - 支持单行折叠算法
   - 支持审批覆盖层和选择视图

3. **优化性能**
   - 实现缓存机制
   - 优化渲染性能
   - 改进宽度适应算法

## 6. 总结

Codex TUI 在视觉风格和架构设计上有很多值得借鉴的地方：

1. **极简颜色哲学**：减少 RGB 使用，优先 ANSI 颜色
2. **终端自适应**：动态检测终端能力，优雅降级
3. **分层状态机**：清晰的关注点分离
4. **丰富的组件系统**：HistoryCell、BottomPane、ChatComposer 等
5. **动画系统**：时间依赖的扫光动画
6. **宽度自适应**：深度自适应算法

当前项目的 Full 模式 CLI 在功能上已经很完善，但在视觉风格和架构设计上还有改进空间。通过借鉴 Codex 的设计理念，可以提升用户体验，使界面更加专业和一致。

建议按照分阶段实施，先从基础改进开始，逐步过渡到架构改进和细节优化。这样可以降低风险，确保每个阶段都能交付可用的改进。
