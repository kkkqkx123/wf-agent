# TUI 实现差距分析

> 基于 `atomcode-tui.md`（atomcode-tuix 参考架构）对当前项目 TUI 实现进行逐项对比。
> 参考实现：atomcode-tuix (Rust) | 当前项目：`apps/cli-app/src/tui/` (TypeScript)

---

## 1. 概述

当前项目 TUI（52 个文件，分布在 `core/`、`components/`、`screens/`、`handlers/`）实现了基础的差分渲染、组件体系、叠加层和屏幕导航。与 atomcode-tuix 参考架构相比，仅覆盖了**约 20%** 的核心能力，缺失了 Renderer 抽象、模态框系统、UiPhase 状态机、事件优先级调度等关键基础设施。

---

## 2. 完整对比矩阵

### 2.1 事件循环

| 特性 | 参考实现 (atomcode-tuix) | 当前项目 | 差距 |
|------|--------------------------|---------|------|
| 优先级调度 | `tokio::select!` biased 模式，10级优先级 | `process.nextTick` + `setTimeout(16ms)` | **严重** |
| 延迟渲染 tick | 5ms (50fps) 处理节流窗口尾部 | 无独立 tick | **缺失** |
| 配置轮询 tick | 500ms 检测 provider/model/config 变化 | 无 | **缺失** |
| Spinner tick | 100ms bounded channel (cap=1) | `setInterval(80ms)` 直接驱动 | **基础** |
| Type-Ahead 队列 | Streaming 下 FIFO 缓冲，TurnFinished drain | 无 | **缺失** |
| 信号处理 | SIGTSTP/SIGCONT 挂起恢复 | 无 | **缺失** |
| Windows Ctrl+C | 键盘路径 + OS 路径双保险 | 无 | **缺失** |
| Resize 合并 | drain 连续事件，只保留最终尺寸 | 无 | **缺失** |

### 2.2 渲染系统

| 特性 | 参考实现 (atomcode-tuix) | 当前项目 | 差距 |
|------|--------------------------|---------|------|
| Renderer 抽象 | `Renderer` trait，8 方法，3 实现 | 无 trait，内联在 `TUI` 类 | **缺失** |
| Retained 渲染器 | `Vec<Vec<Cell>>` 双缓冲，cell 级 diff | 行级别字符串数组对比 | **基础** |
| Plain 渲染器 | CI/pipe 无缓冲流式输出 | 无 | **缺失** |
| Worker 渲染器 | 后台线程 channel 解耦 | 无（JS 单线程） | **N/A** |
| Cell 结构 | `Cell { char, fg, bg, bold, ... }` | 无 cell 概念 | **缺失** |
| Diff 算法 | `diff_cell_frames()` 逐 cell 比较 | 逐行字符串 `!==` | **基础** |
| DECSET 2026 | 同步输出防闪烁 | ✅ 支持 | **一致** |
| Kiity 键盘协议 | 协商 + CSI u 响应解析 | ✅ 支持 | **一致** |
| JediTerm 兼容 | per-row tight repaint 防 ghosting | 无 | **缺失** |
| Windows conhost | 降级 plain renderer + 禁用 alt-screen | 无 | **缺失** |
| 宽字符/CJK | `Cell::continuation()` 占位 | `visibleWidth()` 函数 | **基础** |

### 2.3 UiLine 语义行系统

| 特性 | 参考实现 (atomcode-tuix) | 当前项目 | 差距 |
|------|--------------------------|---------|------|
| 类型系统 | 18 种永久行 + 4 瞬态行 + 2 覆盖层，强类型 enum | 全部 `string[]` | **缺失** |
| 永久行 | Welcome / User / AssistantText / ToolCall / DiffBlock 等 | 无类型区分 | **缺失** |
| 瞬态行 | Spinner / InputPrompt / StreamingBox / ClearTransient | 无概念 | **缺失** |
| 覆盖层 | DiffPanel / ModalOverlayClear | 无概念 | **缺失** |
| Scrollback 管理 | `MAX_SCROLLBACK_ROWS=5000`，自动修剪 + 消息标记 | 无 | **缺失** |
| 助手文本缓冲限制 | 1MB 上限，超限截断 | 无 | **缺失** |

### 2.4 UiPhase 状态机

| 特性 | 参考实现 (atomcode-tuix) | 当前项目 | 差距 |
|------|--------------------------|---------|------|
| 状态枚举 | `Idle / Streaming / Approval` | `Chat / Normal` | **基础** |
| 状态转换 | 6 条转换规则 | 无显式状态机 | **缺失** |
| 输入路由 | 各状态不同输入行为矩阵 | `InputContext` 仅控制键绑定集 | **基础** |
| Streaming 输入 | type-ahead 队列缓冲 | 无 | **缺失** |
| Approval 输入 | Approve/Reject/Skip | 无概念 | **缺失** |

### 2.5 模态框系统

| 特性 | 参考实现 (atomcode-tuix) | 当前项目 | 差距 |
|------|--------------------------|---------|------|
| Modal trait | `handle_key` / `draw` / `captures_all_keys` / `on_plugin_event` / `poll_background` / `close_requested` | 无 trait | **缺失** |
| ModalAction | `Continue / Close` | 无 | **缺失** |
| 捕获型模态框 | `captures_all_keys()=true` Streaming 下拦截所有按键 | 仅有 `nonCapturing` 标志 | **基础** |
| 模态框类型 | ModelPicker / ProviderWizard / SessionPicker / PasswordModal / OnboardingWizard / PluginManager / FileViewer / DiffViewer / DirPicker / LanguagePicker / ProxyPicker / UsageMonitor | 无 | **缺失** |
| 孤儿模态框清理 | Turn 结束自动 dismiss 密码模态框 | 无 | **缺失** |

### 2.6 组件体系

| 特性 | 参考实现 (atomcode-tuix) | 当前项目 | 差距 |
|------|--------------------------|---------|------|
| Component 接口 | 无统一接口（Rust 函数式） | `render()` / `handleInput()` / `invalidate()` | **独特** |
| Container | 无 | ✅ 子组件管理 | **领先** |
| 编辑器 | 无（直接使用 InputPrompt） | 多行编辑器 + 语法 + 撤销/回溯 | **领先** |
| SelectList | 命令 Palette MenuPayload | 支持筛选/描述/导航 | **一致** |
| Spinner/Loader | `SPINNER_FRAMES` Unicode/ASCII 4帧 | `frames` 可配，`setInterval` | **相当** |
| Markdown 渲染 | 有 | 无 | **缺失** |
| Diff 面板 | `DiffPanel {title, rows, footer}` | 无 | **缺失** |
| 图像显示 | Kitty IAL + iTerm2 协议 | 无 | **缺失** |
| 文件浏览器 | `DirPicker` | `FileSelection` 组件 | **相当** |
| 折叠区 | 无 | `FoldableSection` | **独特** |

### 2.7 主题与颜色

| 特性 | 参考实现 (atomcode-tuix) | 当前项目 | 差距 |
|------|--------------------------|---------|------|
| 主题检测 | `is_light_for_render()` 环境检测 + 终端响应 | 无 | **缺失** |
| 颜色角色 | `Role` enum（Default / Muted / Brand / Add / Remove 等） | 部分组件有 Theme 接口 | **基础** |
| 256 色硬编码 | 绕过 Solarized 等调色板重映射 | 无 | **缺失** |

### 2.8 边界情况

| 特性 | 参考实现 (atomcode-tuix) | 当前项目 | 差距 |
|------|--------------------------|---------|------|
| RAII 终端保护 | `TerminalGuard` + panic hook | `stop()` 方法，无崩溃安全 | **基础** |
| 配置热重载 | 500ms mtime 轮询 | 无 | **缺失** |
| 图像粘贴 | 3 级降级链 | 无 | **缺失** |
| 粘贴处理 | 三级解析（arboard RGBA → fileURL → macOS NSPasteboard） | 基础括号粘贴 | **基础** |
| 输入节流 | 20ms THROTTLE_WINDOW + deferred_queue | 16ms setTimeout | **相当** |
| 滚动历史 | 5000 行上限，消息标记管理 | 无 | **缺失** |

---

## 3. 当前项目的独特能力

以下能力在参考架构中不存在，但当前项目已实现：

| 能力 | 位置 | 说明 |
|------|------|------|
| 多 Screen 导航 | `app.ts` | Dashboard / Workflow / Agent 三个屏幕，可切换 |
| Container 子组件管理 | `core/tui.ts` | 组件树管理，addChild/removeChild/clear |
| 撤销栈 (UndoStack) | `core/undo-stack.ts` | Emacs 风格撤销 |
| 杀环 (KillRing) | `core/kill-ring.ts` | 剪切/复制环形缓冲 |
| 模糊搜索 | `core/fuzzy.ts` | 独立模糊匹配算法 |
| 组件级折叠 | `components/foldable-section.ts` | 可折叠区域 |
| 代理迭代面板 | `components/iteration-panel.ts` | Agent 执行进度展示 |
| 工具调用指示器 | `components/tool-call-indicator.ts` | 工具调用可视化 |

---

## 4. 关键差距总结

### P0 —— 架构级缺失（阻碍 Agent 交互流程）

| ID | 缺失项 | 影响 |
|----|--------|------|
| **P0-1** | **Renderer 抽象层** | 无法在 CI/pipe 中复用，渲染逻辑与 TUI 强耦合 |
| **P0-2** | **UiPhase 状态机** | 无法处理 Streaming 下的 type-ahead、Approval 下的工具审批、Ctrl+O 跳过等核心交互 |
| **P0-3** | **模态框系统** | 无法实现密码输入（askpass）、模型选择、文件查看等交互 |
| **P0-4** | **事件优先级调度** | 渲染/输入/定时器无优先级，高负载下输入可能延迟 |

### P1 —— 功能级缺失（影响用户体验）

| ID | 缺失项 | 影响 |
|----|--------|------|
| P1-1 | Scrollback 管理 | 行数无上限，内存可能溢出 |
| P1-2 | 信号处理 | Ctrl+Z 挂起后终端状态混乱 |
| P1-3 | 终端兼容降级 | Windows conhost/JediTerm 下渲染异常 |
| P1-4 | 配置热重载 | 需要重启才能应用配置变更 |
| P1-5 | RAII 终端保护 | 崩溃后终端残留 raw mode |

### P2 —— 增强级缺失（提升体验）

| ID | 缺失项 | 影响 |
|----|--------|------|
| P2-1 | Markdown 渲染器 | Agent 输出无法正确渲染 |
| P2-2 | Diff 面板 | 代码变更无法可视化查看 |
| P2-3 | 图像支持 | 无法粘贴或显示图像 |
| P2-4 | 主题系统 | 无亮/暗主题自适应 |

---

## 5. 当前项目特有优势

当前项目在**组件化和屏幕管理**方面领先于参考架构：

- Container 组件树提供了灵活的 UI 组合能力
- 多 Screen 导航适合工作流管理类应用
- 撤销栈、杀环、模糊搜索等编辑基础设施完善
- 折叠区、迭代面板、工具调用指示器对 Agent 场景友好

这些优势应在后续开发中保留并融入新架构。
