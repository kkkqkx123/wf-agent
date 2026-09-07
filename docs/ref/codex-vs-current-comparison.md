# Codex TUI vs 当前项目 Full CLI 逐模块对比

## 1. Codex 关键实现文件位置索引

### 1.1 样式和颜色系统

| 文件 | 职责 |
|------|------|
| `ref/codex/tui/src/color.rs` | 底层颜色工具：亮度判断、Alpha 混合、CIE76 感知色距 |
| `ref/codex/tui/src/style.rs` | 高层样式构建器：用户消息背景、表格分隔线、强调色 |
| `ref/codex/tui/src/terminal_palette.rs` | 终端色彩能力检测与调色板管理 |
| `ref/codex/tui/src/ui_consts.rs` | 共享 UI 常量 |
| `ref/codex/tui/src/render/highlight.rs` | 语法高亮引擎 (syntect) |
| `ref/codex/tui/src/diff_render.rs` | Diff 渲染器 |

### 1.2 Markdown 渲染

| 文件 | 职责 |
|------|------|
| `ref/codex/tui/src/markdown.rs` | Markdown 渲染入口 |
| `ref/codex/tui/src/markdown_render.rs` | 低层渲染器：表格布局、宽度换行、本地文件链接 |
| `ref/codex/tui/src/markdown_render/streaming.rs` | 流式 Markdown 渲染 |
| `ref/codex/tui/src/markdown_render/table_key_value.rs` | 表格 key/value 渲染 |
| `ref/codex/tui/src/markdown_stream.rs` | Markdown 流收集器 |

### 1.3 对话历史单元

| 文件 | 职责 |
|------|------|
| `ref/codex/tui/src/history_cell/mod.rs` | `HistoryCell` trait 定义 |
| `ref/codex/tui/src/history_cell/messages.rs` | 消息类 HistoryCell |
| `ref/codex/tui/src/history_cell/exec.rs` | 执行类 HistoryCell |
| `ref/codex/tui/src/history_cell/plans.rs` | 计划类 HistoryCell |
| `ref/codex/tui/src/history_cell/approvals.rs` | 审批类 HistoryCell |
| `ref/codex/tui/src/history_cell/patches.rs` | 补丁类 HistoryCell |
| `ref/codex/tui/src/chatwidget.rs` | 聊天主控件 |

### 1.4 底部面板和输入组件

| 文件 | 职责 |
|------|------|
| `ref/codex/tui/src/bottom_pane/mod.rs` | 底部面板核心 |
| `ref/codex/tui/src/bottom_pane/chat_composer.rs` | 聊天输入器状态机 |
| `ref/codex/tui/src/bottom_pane/textarea.rs` | 可编辑文本区组件 |
| `ref/codex/tui/src/bottom_pane/footer.rs` | 底部提示栏 |
| `ref/codex/tui/src/bottom_pane/approval_overlay.rs` | 审批弹窗层 |
| `ref/codex/tui/src/bottom_pane/paste_burst.rs` | 粘贴合并状态机 |

### 1.5 动画和效果

| 文件 | 职责 |
|------|------|
| `ref/codex/tui/src/motion.rs` | 动画原语门控层 |
| `ref/codex/tui/src/shimmer.rs` | Shimmer 光泽效果 |

### 1.6 主题检测

| 文件 | 职责 |
|------|------|
| `ref/codex/tui/src/terminal_probe.rs` | 终端启动探测 |
| `ref/codex/tui/src/terminal_palette.rs` | 色彩级别探测与缓存 |
| `ref/codex/tui/src/theme_picker.rs` | 语法主题选择器 |

---

## 2. 逐模块对比

### 2.1 样式和颜色系统

| 方面 | Codex | 当前项目 | 差距 |
|------|-------|---------|------|
| **终端颜色探测** | OSC 10/11 实时探测 + 缓存 | `ColorDomain::detect_from_env()` 仅检测 COLORTERM/TERM | Codex 探测终端实际颜色，当前仅读环境变量 |
| **明/暗背景检测** | `is_light()` 加权亮度公式 | `ThemeKind` 仅在探测时判断 | Codex 可动态适配，当前是静态预设 |
| **颜色降级** | TrueColor → Ansi256 → Ansi16 三级 | `ColorDomain` 有定义但未用于样式降级 | 当前缺少 `best_color()` 量化 |
| **样式工厂** | `accent_style()`、`user_message_style()` 等自适应函数 | 直接在渲染处硬编码 `Style::default().fg(Color::Cyan)` | 缺少统一的自适应样式层 |
| **颜色混合** | `blend()` 函数支持 alpha 混合 | `Rgb::blend()` 存在但未用于样式生成 | 混合函数存在但未充分利用 |

**关键差异**：Codex 的颜色系统是"终端感知"的——它探测终端实际颜色并自适应生成样式。当前项目是"主题预设"的——使用固定的 RGB 调色板。

### 2.2 Markdown 渲染

| 方面 | Codex | 当前项目 | 差距 |
|------|-------|---------|------|
| **渲染引擎** | pulldown-cmark → 样式化 ratatui Lines | `MarkdownStream` 仅提供源文本分割 | 当前只做分割，不做样式化渲染 |
| **表格渲染** | 完整管线：列宽分配、行转 key/value | 无表格支持 | 缺失 |
| **宽度适应** | `adaptive_wrap_line()`、`word_wrap_line()` | `wrap_line()` 基础换行 | Codex 有更好的宽度自适应 |
| **语法高亮** | syntect 支持 250 种语言 | 无语法高亮 | 缺失 |
| **流式渲染** | `StreamingMarkdownRender` | `MarkdownStream` 仅做源文本分割 | 当前不渲染流式内容 |

**关键差异**：Codex 的 Markdown 是"渲染后"的——输出带样式的 ratatui Lines。当前项目是"源文本"的——输出未渲染的纯文本。

### 2.3 对话历史单元

| 方面 | Codex | 当前项目 | 差距 |
|------|-------|---------|------|
| **核心抽象** | `HistoryCell` trait（多态） | `HistoryLine` 结构体（单一） | Codex 支持多种对话单元类型 |
| **类型多样性** | UserMessage、ExecCell、PlanCell、ApprovalCell 等 10+ 种 | `HistoryLine` + `Role` 枚举 | 当前类型单一 |
| **宽度适应** | `display_lines(width)` 重算换行 | `display_lines(width)` 有换行但无样式 | 都有换行但 Codex 有更多样式 |
| **动画信号** | `transcript_animation_tick()` | 无 | 缺失 |
| **Markdown 缓存** | `markdown_render_cache` | 无 | 缺失 |

**关键差异**：Codex 使用 trait 做多态，每种对话单元有自己的渲染逻辑。当前项目用 `Role` 枚举区分，渲染逻辑集中在一处。

### 2.4 底部面板和输入组件

| 方面 | Codex | 当前项目 | 差距 |
|------|-------|---------|------|
| **输入组件** | `ChatComposer`（多行 + 文本区） | `Composer`（单行） | Codex 支持多行输入 |
| **粘贴检测** | `paste_burst.rs` 状态机 | 无 | 缺失 |
| **历史搜索** | Ctrl+R/S 反向搜索 | Up/Down 历史导航 | Codex 搜索功能更丰富 |
| **斜杠命令** | `/theme`、`/statusline` 等 | 无 | 缺失 |
| **页脚折叠** | 单行折叠算法，宽度自适应 | 固定 4 行 Footer | Codex 更节省空间 |
| **审批覆盖层** | `ApprovalOverlay` 独立组件 | `ApprovalView` 内嵌 Footer | Codex 更模块化 |

**关键差异**：Codex 的底部面板是"层叠式"的——多种视图按需叠加。当前是"固定式"的——Footer 固定高度。

### 2.5 动画和效果

| 方面 | Codex | 当前项目 | 差距 |
|------|-------|---------|------|
| **运动模式** | `MotionMode::Animated/Reduced` | 无 | 缺失 |
| **Shimmer 效果** | 时间扫光动画 | `SPINNER_FRAMES` 旋转字符 | Codex 效果更丰富 |

**关键差异**：Codex 有完整的动画系统，当前只有基础 spinner。

### 2.6 主题检测

| 方面 | Codex | 当前项目 | 差距 |
|------|-------|---------|------|
| **探测方式** | OSC 10/11 + 终端名称识别 | OSC 10/11 + COLORTERM/TERM | 基本相同 |
| **缓存机制** | `OnceLock<Mutex<Cache>>` 单例 | 文件缓存 | 基本相同 |
| **热重载** | 无明确热重载 | SIGUSR2 热重载 | 当前更优 |
| **主题选择** | `/theme` 命令 + 实时预览 | Settings 屏幕 | 基本相同 |

---

## 3. 改进优先级

### 高优先级（视觉风格核心）

1. **样式系统重构**：实现自适应样式层（`accent_style()`、`user_message_style()`）
2. **Markdown 渲染**：将 `MarkdownStream` 输出改为带样式的 ratatui Lines
3. **HistoryCell trait**：将 `HistoryLine` 重构为多态 trait

### 中优先级（体验提升）

4. **多行输入**：将 `Composer` 升级为多行 `ChatComposer`
5. **页脚折叠**：实现宽度自适应的页脚提示
6. **语法高亮**：集成 syntect 或 two_face

### 低优先级（锦上添花）

7. **动画系统**：实现 `MotionMode` 和 `shimmer` 效果
8. **粘贴检测**：实现 `paste_burst` 状态机

---

## 4. 架构调整：Agent 对话界面作为主界面

### 4.1 当前架构问题

当前项目使用 8 个独立屏幕 + 导航栈：

```
Dashboard → Workflow → Executions → Session → ...
```

Session 只是第 4 个屏幕，需要按数字键切换才能进入对话界面。

### 4.2 目标架构

参考 Codex 的设计理念：**对话界面是唯一主界面**，其他功能作为覆盖层/弹窗切换：

```
┌──────────────────────────────────────┐
│  Agent 对话界面（常驻主界面）          │
│                                      │
│  scrollback 区域                     │
│  (历史消息 + 流式输出)                │
│                                      │
│  Streaming tail line                 │
├──────────────────────────────────────┤
│  Footer 区域                         │
│  (状态 + 审批/问题视图)               │
├─────────────────────────────────────┤
│ > 用户输入                           │
└──────────────────────────────────────┘

切换界面：
- Ctrl+T: 转录覆盖层（全屏对话历史）
- Ctrl+B: 侧边栏（Dashboard/工作流/执行/检查点列表）
- /命令: 命令面板（设置/搜索/帮助）
```

### 4.3 实施方案

#### 阶段一：将 Session 设为默认主界面

修改 `tui.rs` 中的导航逻辑：

```rust
// 当前：Dashboard 是默认屏幕
// 修改：Session 是默认屏幕
impl Screens {
    pub fn new() -> Self {
        Self {
            stack: vec![Screen::new(ScreenKind::Session)], // 改为 Session
            selected: 0,
        }
    }
}
```

#### 阶段二：其他屏幕改为覆盖层

将 Workflow/Executions/Checkpoints 等改为侧边栏弹窗：

```rust
// 新增：侧边栏模式
pub enum OverlayKind {
    Sidebar(ScreenKind),  // 侧边栏显示其他屏幕
    Transcript,           // 转录覆盖层
    CommandPalette,       // 命令面板
}
```

#### 阶段三：键盘映射调整

| 按键 | 当前行为 | 目标行为 |
|------|---------|---------|
| `1-8` | 跳转到对应屏幕 | 打开侧边栏并选中对应项 |
| `Ctrl+T` | 无 | 打开转录覆盖层 |
| `Ctrl+B` | 无 | 打开/关闭侧边栏 |
| `/` | 无 | 打开命令面板 |
| `q`/`Esc` | 返回上一层 | 关闭覆盖层/返回对话 |

### 4.4 关键文件改动

| 文件 | 改动内容 |
|------|---------|
| `tui.rs` | 默认屏幕改为 Session，新增覆盖层管理 |
| `screens.rs` | 新增 `OverlayKind`，支持侧边栏模式 |
| `session.rs` | 移除固定边框，成为全屏主界面 |
| `keymap.rs` | 新增 Ctrl+T/Ctrl+B 快捷键 |
| `footer.rs` | 在对话模式下显示精简页脚 |

---

## 5. 关键文件映射（当前 → Codex 参考）

| 当前文件 | Codex 参考文件 | 改进方向 |
|---------|---------------|---------|
| `theme.rs` | `color.rs` + `terminal_palette.rs` | 增加明/暗检测、颜色降级 |
| `scrollback.rs` | `history_cell/` | 重构为 trait |
| `markdown.rs` | `markdown_render.rs` | 增加样式化渲染 |
| `composer.rs` | `bottom_pane/chat_composer.rs` | 升级为多行 |
| `footer.rs` | `bottom_pane/footer.rs` | 实现折叠算法 |
| `session.rs` | `chatwidget.rs` | 重构为分层状态机 |
| `approval.rs` | `bottom_pane/approval_overlay.rs` | 分离为独立组件 |
| `modal.rs` | `bottom_pane/list_selection_view.rs` | 增强选择器 |
| `render.rs` | `markdown_render/streaming.rs` | 增加样式化渲染 |
| `tui.rs` | `tui.rs` | 重构事件循环 |
