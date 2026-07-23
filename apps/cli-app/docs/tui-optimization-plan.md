# CLI-App TUI 功能分析及分阶段优化方案

## 一、项目定位与 Claude Code 的本质差异

### 1.1 当前项目定位

cli-app 是一个**工作流引擎 + Agent 循环的 CLI 工具**，支持三种执行模式：

| 模式 | 用途 | 启用方式 |
|------|------|---------|
| 交互模式 (Interactive) | 终端用户直接操作 | 默认 / `--tui` |
| 无头模式 (Headless) | CI/CD、自动化脚本 | `HEADLESS=true` |
| 编程模式 (Programmatic) | 被其他程序调用 | `CLI_MODE=programmatic` |

在交互模式中，TUI 是**可选的子集**：无子命令 + 有 TTY + 输出格式为 text 时自动启动，否则显示命令行帮助。

### 1.2 与 Claude Code 的根本差异

| 维度 | Claude Code | cli-app |
|------|------------|---------|
| 核心功能 | 单一 AI 对话 | 工作流图管理 + Agent 对话 + 批量 CLI |
| 界面结构 | 单屏聊天 + 弹窗菜单 | 多页面（Dashboard/Workflow/Agent） |
| 用户交互 | 打字 + 浏览历史 | 菜单选择 + 表单编辑 + 实时监控 |
| 使用场景 | 日常 AI 编程辅助 | 工作流编排 + 批量执行 + 运维监控 |
| 输出方式 | 全屏 TUI 独占 | TUI / 命令行参数 / JSON 流 / 静默 |
| 编辑能力 | 单行/多行提示词输入 | 提示词 + 配置文件 + 工作流图 |

**结论**：cli-app 的 TUI 是多种使用方式中的一种，服务于工作流和 Agent 管理，不应该照搬 Claude Code 的单界面聊天模式。

---

## 二、现有 TUI 实现分析

### 2.1 整体架构

```
tui/
├── core/                          # 底层引擎（自制，核心资产）
│   ├── tui.ts                     # TUI 引擎（723 行）：差分渲染、覆盖层、焦点管理
│   ├── keybindings.ts             # 按键绑定管理（243 行）：类型安全、可扩展
│   ├── keys/                      # 键盘输入（7 文件）：Kitty + legacy 双协议，生产级品质
│   ├── terminal.ts                # 终端抽象层
│   ├── stdin-buffer.ts            # 输入缓冲拼接（345 行）
│   ├── autocomplete.ts            # 自动补全接口
│   ├── fuzzy.ts                   # 模糊匹配
│   ├── undo-stack.ts              # 撤销栈
│   ├── kill-ring.ts               # Emacs kill ring
│   └── utils.ts                   # 文本工具
├── components/                    # UI 组件
│   ├── input.ts                   # 单行输入（419 行）
│   ├── editor.ts                  # 多行编辑器（1631 行）
│   ├── select-list.ts             # 可滚动选择列表
│   ├── box.ts / text.ts / spacer.ts / loader.ts
│   ├── iteration-panel.ts         # Agent 迭代面板
│   ├── tool-call-indicator.ts     # 工具调用指示器
│   └── file-selection.ts          # 文件选择
├── screens/                       # 页面
│   ├── dashboard-screen.ts        # 主菜单仪表盘
│   ├── workflow-screen.ts         # 工作流管理（列表 + 详情 + 日志）
│   └── agent-screen.ts            # Agent 监控（状态 + 迭代 + 工具调用 + 输入）
└── app.ts                         # 应用主控（133 行）
```

### 2.2 各层次能力评估

#### 底层引擎 — 整体优良，均保留不动

| 模块 | 评估 | 说明 |
|------|------|------|
| TUI 引擎 | 优 | 差分渲染 + 覆盖层 + 同步输出，性能核心 |
| Terminal | 优 | 跨平台抽象，raw mode，resize 探测 |
| StdinBuffer | 优 | 转义序列拼接，括号粘贴，高字节处理 |
| UndoStack / KillRing | 优 | 干净简洁的 Emacs 编辑基础 |
| Fuzzy / Utils | 优 | 模糊匹配、词边界、ANSI 宽度 |
| Autocomplete | 优 | 接口清晰，支持组合提供器 |
| keys/ | 优 | Kitty + legacy 双协议覆盖，跨终端兼容性生产级品质 |

#### 按键绑定系统 — 需要增强

当前 `KeybindingsManager` 没有上下文（Context）概念，所有绑定全局生效。这导致：
- Esc 无法根据场景切换行为（关闭弹窗 vs 切换到 Normal 模式 vs 取消操作）
- Enter 在输入框和选择列表中必须硬编码分派逻辑
- 无法实现「同一按键在不同区域不同行为」

**调整方向**：增加 Context 层（Global / Chat / SelectList / Modal），使按键绑定按上下文分组。

#### Editor 组件 — 需要保留和微调

Editor 的功能对标 Claude Code 的 Chat 模式编辑能力，属于同类产品应有的水准：

| 功能 | 必要性 | 调整 |
|------|--------|------|
| 多行输入 + 自动换行 | 必须 | 保留 |
| 撤销/重做 | 必须 | 保留 |
| Kill ring（Ctrl+U/K/W/Y） | 需要 | 保留 |
| 词边界导航（Alt+左右） | 需要 | 保留 |
| 历史浏览（上下箭头） | 必须 | 保留 |
| 自动补全（Tab / / @ #） | 必须 | 保留 |
| 括号粘贴处理 | 必须 | 保留 |
| Grapheme 感知光标 | 需要 | 保留 |
| Page Up/Down 滚动 | 可保留 | 保留 |
| Jump Forward（Ctrl+]） | 低频 | **移除** |
| Jump Backward（Ctrl+Alt+]） | 低频 | **移除** |
| Yank Pop（Alt+Y） | 低频 | **移除** |

**调整方向**：移除三个低频功能（Claude Code 也没有）；保留其余所有能力。

#### Screen 多页面设计 — 需要保留并增强

三个 Screen 各自服务于不同的功能域：

| Screen | 功能 | 必要性 |
|--------|------|--------|
| DashboardScreen | 主菜单导航 + 系统状态概览 | 必须 |
| WorkflowScreen | 工作流列表 + 详情 + 操作日志 | 必须 |
| AgentScreen | Agent 状态监控 + 流式日志 + 消息输入 | 必须 |

这与 Claude Code 的单屏模式不同。工作流图管理是项目的核心差异化功能，需要独立的界面空间。

---

## 三、Claude Code 功能逐项筛选

### 3.1 值得引入

| Claude Code 功能 | 适用范围 | 理由 |
|-----------------|---------|------|
| 模态上下文（Global/Chat/Normal/Modal） | TUI 全局 | 解决按键冲突，让 Esc 等键在不同场景有不同行为 |
| Vim 风格导航（j/k/g/G/Ctrl+u/d） | AgentScreen 日志浏览 | 提供高效的对话历史滚动方式 |
| Space 折叠/展开区块 | AgentScreen 工具调用/迭代面板 | 长日志中快速定位关键信息 |
| 自定义 keybindings.json | TUI 全局 | 让用户根据习惯调整按键 |
| Ctrl+L 重绘 | TUI 全局 | 修复终端花屏 |
| Ctrl+D 退出 | TUI 全局 | 标准 EOF 退出方式 |
| Esc 统一取消（弹窗/模式的通用退出） | TUI 全局 | 减少用户记忆成本 |

### 3.2 不需要引入（或已有替代）

| Claude Code 功能 | 替代方案 | 原因 |
|-----------------|---------|------|
| Rewind 对话回滚 | 项目已有 Checkpoint 系统 | 功能重复且更强 |
| Shift+Tab 切换权限模式 | 项目有自己的 Approval 系统 | 架构不兼容 |
| Ctrl+B 后台任务 | 已有 TerminalManager 独立终端 | 功能更强 |
| Ctrl+G 外部编辑器 | 非核心需求 | 使用频率低 |
| 斜杠命令 `/model` `/clear` `/vim` | 项目有自己的命令行体系 | 功能可通过 CLI 命令替代 |
| Visual 模式（v/V 文本选择） | 场景有限 | 工作流管理场景不需要对话区文本选择 |
| `Esc Esc` 打开 Rewind | 已排除 | 无此功能 |

### 3.3 与现有能力重叠

| Claude Code 功能 | 现有实现 | 取舍 |
|-----------------|---------|------|
| Readline 编辑（Ctrl+A/E/U/K） | Editor 已完整实现 | 保留 |
| 历史浏览（上下箭头） | Editor.history 已实现 | 保留 |
| 自动补全（Tab） | Editor.autocomplete 已实现 | 保留 |
| 粘贴支持 | StdinBuffer 已处理 | 保留 |
| Enter 提交 / Shift+Enter 换行 | Editor 已实现 | 保留 |

---

## 四、分阶段实施方案

### 阶段一：模态上下文与 Normal 浏览模式（预计 3 天）

这是最核心的架构增强，直接影响用户交互体验。

#### 4.1.1 按键绑定增加 Context 支持

**涉及文件**：`core/keybindings.ts`、`core/tui.ts`

**改动内容**：
- `KeybindingDefinitions` 中每个按键定义增加 `context` 字段
- `KeybindingsManager.matches()` 增加 `context` 参数
- `TUI` 类增加 `currentContext` 状态，在 `handleInput` 中根据上下文路由按键
- 区分四种上下文：`global`、`chat`、`selectList`、`modal`

**关键按键在不同上下文的行为**：

| 按键 | Global | Chat（输入框） | SelectList（选择列表） | Modal（弹窗） |
|------|--------|--------------|---------------------|-------------|
| Enter | — | 提交消息 | 确认选择 | 确认 |
| Esc | 切换到 Normal | 切换到 Normal | 取消选择 | 关闭弹窗 |
| Ctrl+C | 中断/退出 | 中断 | 中断 | 中断 |
| 上/下箭头 | — | 历史浏览 | 上下选择 | 上下选择 |
| Tab | — | 触发补全 | — | — |

#### 4.1.2 TUI 增加 InputMode 状态

**涉及文件**：`core/tui.ts`

**新增类型**：

```typescript
enum InputMode {
  Chat,    // 插入模式：焦点在输入框
  Normal,  // 浏览模式：焦点在对话历史
}
```

**切换逻辑**：
- 启动时默认 Chat 模式
- Esc → Normal 模式
- Normal 模式下按 Enter 或任意字母键 → Chat 模式

#### 4.1.3 AgentScreen 实现 Normal 浏览模式

**涉及文件**：`screens/agent-screen.ts`、新增 `components/chat-history.ts`

**新增按键（仅 Normal 模式生效）**：

| 按键 | 功能 |
|------|------|
| j / Down | 向下滚动 1 行 |
| k / Up | 向上滚动 1 行 |
| Ctrl+u | 向上半屏 |
| Ctrl+d | 向下半屏 |
| g / Ctrl+Home | 跳到日志顶部 |
| G / Ctrl+End | 跳到最新消息 |

**改动内容**：
- `AgentScreen` 增加 `mode` 状态和 `scrollOffset` 跟踪
- 日志内容渲染时根据 `scrollOffset` 切片显示
- `handleInput` 中根据当前模式分派按键

**不影响**：DashboardScreen 和 WorkflowScreen 不需要 Normal 模式，焦点管理通过 SelectList 完成。

#### 4.1.4 Editor 增加 mode 通知

**涉及文件**：`components/editor.ts`

**改动内容**：
- Editor 增加 `onModeSwitch` 回调（当 Esc 按下时通知父组件切换为 Normal）
- Enter 在 Normal 模式下先切换回 Chat，不提交

---

### 阶段二：日志折叠与全局热键（预计 2 天）

#### 4.2.1 日志区块折叠（Space 折叠/展开）

**涉及文件**：`screens/agent-screen.ts`、新增 `components/foldable-section.ts`

**改动内容**：
- 新增 `FoldableSection` 组件：接收标题行 + 内容行，支持折叠/展开状态切换
- AgentScreen 中的工具调用日志和迭代详情用 `FoldableSection` 包裹
- Normal 模式下 Space 键切换当前区块折叠状态
- 折叠时显示摘要行（如 `[Tool Call: read_file → 50 lines]`）

#### 4.2.2 全局热键标准化

**涉及文件**：`app.ts`、`core/keybindings.ts`

**新增全局按键**：

| 按键 | 功能 | 对标 Claude Code |
|------|------|-------------------|
| Ctrl+L | 强制重绘界面（修复花屏） | Ctrl+L |
| Ctrl+D | 退出 TUI（EOF 语义） | Ctrl+D |
| Esc | 通用退出：关闭弹窗 → 取消操作 → 切换到 Normal | Esc |

**改动内容**：
- `CLIAppTUI.setupGlobalKeybindings()` 中注册全局按键处理
- `TUI.requestRender(true)` 已支持强制重绘（Ctrl+L 直接调用）
- Ctrl+D 在输入框为空时退出，否则作为普通 delete-char-forward

---

### 阶段三：Editor 微调与自定义按键配置（预计 2 天）

#### 4.3.1 Editor 移除低频功能

**涉及文件**：`components/editor.ts`、`core/keybindings.ts`

**移除的按键定义**：
- `tui.editor.jumpForward` 和 `tui.editor.jumpBackward`
- `tui.editor.yankPop`

**移除的代码**：
- Editor 中 `yankPop()` 方法及其调用
- `handleInput` 中 Jump Forward/Backward 相关分支

**保留（不受影响）**：
- Ctrl+Y（yank）保持
- Ctrl+W（deleteWordBackward）保持
- Kill Ring 本身保持完整

#### 4.3.2 自定义按键配置文件

**涉及文件**：`core/keybindings.ts`、`app.ts`

**配置格式**（`~/.config/modular-agent/keybindings.json`）：

```json
{
  "bindings": [
    {
      "context": "chat",
      "bindings": {
        "ctrl+enter": "tui.input.submit",
        "enter": "tui.input.newLine"
      }
    },
    {
      "context": "global",
      "bindings": {
        "ctrl+q": "global:quit"
      }
    }
  ]
}
```

**改动内容**：
- 向 `KeybindingsManager` 增加 `context` 字段支持（阶段一已完成）
- 增加 `loadUserKeybindings()` 函数，从配置文件读取并 `setUserBindings()`
- `CLIAppTUI.start()` 时自动加载
- 新增 Agent 输入框斜杠命令 `/keybindings`：在系统编辑器中打开配置文件

---

### 阶段四：Screen 功能增强（预计 2 天）

#### 4.4.1 DashboardScreen 增强

**涉及文件**：`screens/dashboard-screen.ts`

**新增功能**：
- 实时显示活跃 Agent 数量（从 AgentLoopAdapter 获取）
- 最近执行记录列表（最近 5 条）
- 系统资源状态（内存、CPU 使用情况）

#### 4.4.2 WorkflowScreen 增强

**涉及文件**：`screens/workflow-screen.ts`

**新增功能**：
- 图结构预览（调用 `WorkflowGraphAdapter.getGraph()` 获取节点和边信息，以缩进树形展示）
- 操作确认弹窗（新建/编辑/删除前弹出确认，利用已有的 Overlay 系统）
- 快捷键：`N` 新建、`E` 编辑、`D` 删除（已有骨架，完善实现）

#### 4.4.3 AgentScreen 工具调用面板增强

**涉及文件**：`components/tool-call-indicator.ts`、`components/iteration-panel.ts`

**新增功能**：
- 工具调用详情展开/折叠：默认显示标题行，按 Enter 展开查看参数和输出
- 迭代面板增加耗时趋势（对比相邻迭代的耗时变化）

---

## 五、实施总览

| 阶段 | 内容 | 涉及文件数 | 预计工时 |
|------|------|-----------|---------|
| 一 | 模态上下文 + Normal 浏览模式 | 4-5 | 3 天 |
| 二 | 日志折叠 + 全局热键 | 3-4 | 2 天 |
| 三 | Editor 微调 + 自定义按键配置 | 3 | 2 天 |
| 四 | Screen 功能增强 | 4-5 | 2 天 |
| **合计** | | **~15** | **9 天** |

---

## 六、不做的事

1. **不改变 keys/ 键盘处理模块** — Kitty + legacy 双协议覆盖是生产级品质实现，无副作用，保留不动
2. **不合并三个 Screen 为单界面** — 工作流图管理需要独立空间
3. **不大规模拆分 Editor** — 1631 行是合理体量，只做微量裁剪
4. **不引入 Rewind / 权限切换 / 后台任务 / 外部编辑器 / Visual 模式 / 斜杠命令体系** — 项目已有更合适的替代方案
5. **不改变多模式架构** — TUI 只是交互模式的一个子集，headless/CLI 命令模式保持不变
