# TUI 分阶段改造方案

> 基于 `docs/analysis/tui-gap-analysis.md` 提出的差距分析，制定分阶段实施计划。

---

## 目录

1. [总体目标](#1-总体目标)
2. [架构总览（目标态）](#2-架构总览目标态)
3. [Phase 1 —— 事件循环与渲染架构重构](#3-phase-1--事件循环与渲染架构重构)
4. [Phase 2 —— UiPhase 状态机与输入路由](#4-phase-2--uiphase-状态机与输入路由)
5. [Phase 3 —— 模态框系统](#5-phase-3--模态框系统)
6. [Phase 4 —— 终端兼容性与边界保护](#6-phase-4--终端兼容性与边界保护)
7. [Phase 5 —— 业务组件补齐](#7-phase-5--业务组件补齐)
8. [影响分析](#8-影响分析)
9. [实施路线图](#9-实施路线图)

---

## 1. 总体目标

### 1.1 目标

在保留当前项目组件化和屏幕管理优势的基础上，补齐参考架构中的核心基础设施，使 TUI 具备支撑 Agent 工作流全流程交互的能力。

### 1.2 设计原则

- **渐进增强**：每阶段独立可交付，不阻塞现有功能
- **向后兼容**：外部接口（Screen / Component）尽量不破坏现有消费者
- **保持优势**：保留 Container 组件树、多 Screen 导航、编辑基础设施
- **适度简化**：TypeScript 环境不需要完全复刻 Rust 的 cell 级 diff，行级别 + 区域标记即可

---

## 2. 架构总览（目标态）

```
                     ┌─────────────────────────────┐
                     │     TUI Application          │
                     │  (CLIAppTUI / Screen 导航)    │
                     └──────────┬──────────────────┘
                                │
                     ┌──────────▼──────────────────┐
                     │     Event Loop (Scheduler)    │
                     │  biased priority queue:       │
                     │  1. Deferred render (50fps)  │
                     │  2. Config poll (500ms)      │
                     │  3. Spinner (100ms)          │
                     │  4. Terminal input           │
                     │  5. Agent events             │
                     └──────────┬──────────────────┘
                                │
              ┌─────────────────┼─────────────────┐
              ▼                 ▼                  ▼
     ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐
     │ Retained     │  │ Plain        │  │ Worker           │
     │ Renderer     │  │ Renderer     │  │ Renderer         │
     │ (TTY, diff)  │  │ (CI/pipe)    │  │ (bg thread)      │
     └──────┬───────┘  └──────┬───────┘  └────────┬─────────┘
            │                 │                     │
            └─────────────────┼─────────────────────┘
                              │
                     ┌────────▼────────┐
                     │   Renderer      │
                     │   Interface     │
                     │  (6 methods)    │
                     └─────────────────┘

┌──────────────┐    ┌──────────────┐    ┌──────────────────┐
│ UiPhase      │    │ Modal System │    │ UiLine 语义行    │
│ State Machine│◄──►│ (Modal trait) │◄──►│ (类型化输出)      │
│ Idle/Stream/ │    │ 12+ modal    │    │ 18 永久 + 4 瞬态  │
│ Approval     │    │ types        │    │ + 2 overlay       │
└──────────────┘    └──────────────┘    └──────────────────┘
```

---

## 3. Phase 1 —— 事件循环与渲染架构重构

### 3.1 目标

建立 Renderer 抽象层和事件优先级调度机制，为后续功能提供基础。

### 3.2 变更清单

#### 3.2.1 Renderer 接口 (`core/renderer.ts`，新建)

定义 6 方法渲染接口：

```typescript
export interface Renderer {
  render(lines: UiLine[]): void;
  flush(): void;
  shutdown(): void;
  reset(): void;
  clearScreen(): void;
  beginSync(): void;
  endSync(): void;
}
```

#### 3.2.2 RetainedRenderer (`core/retained-renderer.ts`，从 `TUI` 类中提取)

将现有 TUI 类中的渲染逻辑提取为 RetainedRenderer：

- 当前 diff 逻辑（行级别字符串对比）保持不变
- 添加 `previousLines` 和 `currentLines` 双缓冲
- 添加 DECSET 2026 同步输出封装
- overlay 合成逻辑保留

#### 3.2.3 PlainRenderer (`core/plain-renderer.ts`，新建)

- 流式输出，无缓冲
- 无光标/ANSI 控制字符
- 检测 `!process.stdout.isTTY` 时自动使用

#### 3.2.4 TUI 类重构 (`core/tui.ts`)

- `TUI` 构造函数接受 `Renderer` 实例
- 渲染调用委托给 `renderer.render()` / `renderer.flush()`
- `TUI` 专注于输入处理、组件管理、调度

```typescript
export class TUI {
  constructor(
    private terminal: Terminal,
    public renderer: Renderer
  ) {}
  // ...输入处理、组件树、overlay
}
```

#### 3.2.5 EventScheduler (`core/event-scheduler.ts`，新建)

简化版优先级调度器，替代 `process.nextTick` + `setTimeout`：

```typescript
export interface TickSource {
  interval: number;      // ms
  handler: () => void;
  priority: number;      // 1 (highest) - 10 (lowest)
  enabled: () => boolean;
}

export class EventScheduler {
  addSource(source: TickSource): void;
  removeSource(name: string): void;
  start(): void;
  stop(): void;
}
```

优先级设计（简化至 5 级）：

| 优先级 | Tick 源 | 间隔 | 条件 |
|--------|---------|------|------|
| 1 | Deferred render | 16ms (60fps) | 有 pending update |
| 2 | Config poll | 500ms | Idle 状态 |
| 3 | Spinner | 100ms | Streaming 状态 |
| 4 | Agent event | - | 事件到达 |
| 5 | Terminal input | - | 数据到达 |

输入事件使用 `async/await` + `EventEmitter` 优先级队列替代 `tokio::select!`，不做完全复刻。

### 3.3 文件改动

| 路径 | 操作 | 说明 |
|------|------|------|
| `apps/cli-app/src/tui/core/renderer.ts` | 新建 | Renderer 接口 |
| `apps/cli-app/src/tui/core/retained-renderer.ts` | 新建 | 从 TUI 提取渲染逻辑 |
| `apps/cli-app/src/tui/core/plain-renderer.ts` | 新建 | 非 TTY 降级渲染器 |
| `apps/cli-app/src/tui/core/event-scheduler.ts` | 新建 | 优先级调度器 |
| `apps/cli-app/src/tui/core/tui.ts` | 重构 | 集成 Renderer 委托 |
| `apps/cli-app/src/tui/core/index.ts` | 修改 | 导出新模块 |
| `apps/cli-app/src/tui/app.ts` | 修改 | 传入 Renderer 实例 |

### 3.4 验证

- `PlainRenderer` 对 `stdout` 写入验证（直接写内容，无 ANSI）
- `RetainedRenderer` 空渲染不加新内容
- TUI 启动/停止至少调用一次 render/flush
- Renderer 接口的 mock 可替换性验证
- Diff 渲染得到相同输出

---

## 4. Phase 2 —— UiPhase 状态机与输入路由

### 4.1 目标

引入三态状态机，驱动输入路由、spinner 可见性、type-ahead 行为。

### 4.2 变更清单

#### 4.2.1 UiPhase 枚举 (`core/uiphase.ts`，新建)

```typescript
export enum UiPhase {
  Idle = "idle",
  Streaming = "streaming",
  Approval = "approval",
}
```

#### 4.2.2 UiPhaseManager (`core/uiphase.ts`)

```typescript
export class UiPhaseManager {
  get phase(): UiPhase;
  transition(newPhase: UiPhase): void;
  onTransition(cb: (from: UiPhase, to: UiPhase) => void): void;
}
```

状态转换规则：

- `Idle` --[Submit]--> `Streaming`
- `Streaming` --[ToolCall]--> `Approval`
- `Approval` --[Approve]--> `Streaming`
- `Approval` --[Reject]--> `Idle`
- `Streaming` --[TurnFinished]--> `Idle`
- `Streaming` --[Cancel/Ctrl+C]--> `Idle`

#### 4.2.3 TypeAheadQueue (`core/typeahead-queue.ts`，新建)

```typescript
export class TypeAheadQueue {
  enqueue(input: string): void;
  dequeue(): string | null;
  clear(): void;
  get length(): number;
  pause(): void;   // Provider 切换时
  resume(): void;
}
```

#### 4.2.4 TUI 类输入路由重构

```typescript
// TUI.handleInput 根据 UiPhase 路由：
// Idle → 正常输入/命令 palette/历史导航
// Streaming → type-ahead 队列/ Ctrl+O 跳过/ Ctrl+C 取消
// Approval → Approve(y)/Reject(n)/Skip(Ctrl+O)/Cancel(Ctrl+C)
```

#### 4.2.5 输入行为矩阵

| 操作 | Idle | Streaming | Approval |
|------|:----:|:---------:|:--------:|
| 文本输入 | ✓ (直接) | ✓ (type-ahead) | ✗ |
| Enter | Submit | 换行/提交 | Approve |
| ↑/↓ | 历史导航 | 历史导航 | ✗ |
| Ctrl+C | 退出确认 | 取消 turn | 取消 turn |
| Ctrl+O | ✗ | 跳过工具 | ✗ |
| Esc | 清空输入 | ✗ | Reject |

### 4.3 文件改动

| 路径 | 操作 | 说明 |
|------|------|------|
| `apps/cli-app/src/tui/core/uiphase.ts` | 新建 | UiPhase 枚举 + UiPhaseManager |
| `apps/cli-app/src/tui/core/typeahead-queue.ts` | 新建 | type-ahead FIFO 队列 |
| `apps/cli-app/src/tui/core/tui.ts` | 重构 | 输入路由增加 phase 判断 |
| `apps/cli-app/src/tui/core/event-scheduler.ts` | 修改 | Spinner tick 根据 Streaming 状态启停 |
| `apps/cli-app/src/tui/core/index.ts` | 修改 | 导出新模块 |
| `apps/cli-app/src/tui/screens/agent-screen.ts` | 修改 | 集成 UiPhase 状态机 |

### 4.4 验证

- 状态转换合规，无非法转换
- Streaming 下 type-ahead 队列可缓冲/正确 drain
- Approval 下文本输入被拦截
- Ctrl+C 在 Streaming/Approval 取消 turn
- 状态切换后 Spinner 正确启停

---

## 5. Phase 3 —— 模态框系统

### 5.1 目标

建立完善的模态框接口和类型体系，支持业务交互（密码、选择、查看）。

### 5.2 变更清单

#### 5.2.1 Modal 接口 (`modals/modal.ts`，新建)

```typescript
export enum ModalAction {
  Continue = "continue",
  Close = "close",
}

export interface Modal {
  handleKey(key: string): ModalAction;
  render(width: number, height: number): string[];
  handlePaste?(text: string): ModalAction;
  capturesAllKeys(): boolean;       // 类比 atomcode captures_all_keys
  onPluginEvent?(event: unknown): void;
  pollBackground?(): boolean;       // 后台轮询（如 UsageMonitor）
  closeRequested?(): boolean;       // 外部请求关闭
}
```

#### 5.2.2 ModalManager (`modals/modal-manager.ts`，新建)

```typescript
export class ModalManager {
  get activeModal(): Modal | null;
  get isCapturing(): boolean;
  show(modal: Modal): void;
  close(): void;
  handleKey(key: string): boolean;  // true = consumed
  render(width: number, height: number): string[] | null;
}
```

#### 5.2.3 模态框类型

| 模态框 | 文件 | 优先级 | 说明 |
|--------|------|--------|------|
| `PasswordModal` | `modals/password-modal.ts` | **P0** | 捕获型，askpass 密码输入 |
| `ModelPicker` | `modals/model-picker.ts` | P1 | `/model` 命令 |
| `SessionPicker` | `modals/session-picker.ts` | P1 | `/resume` 命令 |
| `FileViewer` | `modals/file-viewer.ts` | P1 | `/view` 命令 |
| `DiffViewer` | `modals/diff-viewer.ts` | P2 | `/diff` 命令 |
| `ConfirmModal` | `modals/confirm-modal.ts` | P0 | 工具执行确认（替代当前 ConfirmBox） |

PasswordModal 作为捕获型模态框，在 Streaming 下可弹出并拦截所有按键。

#### 5.2.4 ConfirmModal（替换现有 ConfirmBox）

当前 `workflow-screen.ts` 中的 `ConfirmBox` 是内联类，替换为标准的 `ConfirmModal`：

```typescript
export class ConfirmModal implements Modal {
  constructor(
    private title: string,
    private message: string,
    private onConfirm: () => void,
    private onCancel: () => void,
  ) {}
  capturesAllKeys(): boolean { return true; }
  handleKey(key: string): ModalAction { /* y/Enter=confirm, n/Esc=cancel */ }
  render(width: number, height: number): string[] { /* 居中确认框 */ }
}
```

#### 5.2.5 孤儿模态框清理

```typescript
// Turn 结束时检查是否有捕获型模态框残留
phaseManager.onTransition((_from, to) => {
  if (to === UiPhase.Idle && modalManager.isCapturing) {
    modalManager.close();
  }
});
```

### 5.3 文件改动

| 路径 | 操作 | 说明 |
|------|------|------|
| `apps/cli-app/src/tui/modals/modal.ts` | 新建 | Modal 接口 + ModalAction |
| `apps/cli-app/src/tui/modals/modal-manager.ts` | 新建 | 模态框管理器 |
| `apps/cli-app/src/tui/modals/password-modal.ts` | 新建 | 密码输入（捕获型） |
| `apps/cli-app/src/tui/modals/confirm-modal.ts` | 新建 | 确认对话框 |
| `apps/cli-app/src/tui/modals/model-picker.ts` | 新建 | 模型选择 |
| `apps/cli-app/src/tui/modals/session-picker.ts` | 新建 | 会话选择 |
| `apps/cli-app/src/tui/modals/file-viewer.ts` | 新建 | 文件查看 |
| `apps/cli-app/src/tui/modals/diff-viewer.ts` | 新建 | Diff 查看 |
| `apps/cli-app/src/tui/modals/index.ts` | 新建 | 导出 |
| `apps/cli-app/src/tui/core/tui.ts` | 修改 | 集成 ModalManager |
| `apps/cli-app/src/tui/app.ts` | 修改 | 暴露模态框接口 |
| `apps/cli-app/src/tui/screens/workflow-screen.ts` | 修改 | ConfirmBox → ConfirmModal |
| `apps/cli-app/src/tui/screens/agent-screen.ts` | 修改 | 集成模态框 |

### 5.4 验证

- 模态框打开后键盘事件被正确消费
- `capturesAllKeys()` 模态框在 Streaming 下工作
- 多个模态框叠加压栈/弹栈正确
- 孤儿模态框在 Turn 结束后自动关闭
- 密码输入不回显

---

## 6. Phase 4 —— 终端兼容性与边界保护

### 6.1 目标

补齐终端兼容降级、信号处理、RAII 保护、配置热重载等边界能力。

### 6.2 变更清单

#### 6.2.1 TerminalDetector (`core/terminal-detector.ts`，新建)

```typescript
export interface TerminalCapabilities {
  kittyKeyboard: boolean;
  jediterm: boolean;
  legacyConhost: boolean;
  trueColor: boolean;
  synchronizedOutput: boolean;
}

export function detectCapabilities(): TerminalCapabilities {
  // 环境变量检测：
  //   TERM_PROGRAM=IntelliJ IDEA → jediterm
  //   TERM_PROGRAM=Windows Terminal → conhost（非 legacy）
  //   COLORTERM=truecolor → trueColor
  // Kitty 协议通过 CSI u 查询响应检测
}
```

降级策略：

| 检测结果 | 降级动作 |
|---------|---------|
| legacyConhost | 禁用 alt-screen，使用 PlainRenderer，无鼠标捕获 |
| jediterm | per-row tight repaint，避免 cell 级别定位 |
| !synchronizedOutput | 不发送 DECSET 2026 序列 |
| !kittyKeyboard | 禁用增强键盘协议 |

#### 6.2.2 TerminalGuard (`core/terminal-guard.ts`，新建)

RAII 风格终端保护：

```typescript
export class TerminalGuard {
  private static armed = false;

  static arm(): void {
    if (this.armed) return;
    this.armed = true;

    process.on("exit", () => this.restore());
    process.on("SIGINT", () => { this.restore(); process.exit(1); });
    process.on("uncaughtException", (err) => {
      console.error("Uncaught exception:", err);
      this.restore();
      process.exit(1);
    });
    process.on("unhandledRejection", (reason) => {
      console.error("Unhandled rejection:", reason);
    });
  }

  static restore(): void {
    // 恢复 raw mode
    // 恢复 bracketed paste
    // 显示光标
    // 重置终端属性
  }
}
```

#### 6.2.3 SignalHandler (`core/signal-handler.ts`，新建)

```typescript
export class SignalHandler {
  onSuspend(cb: () => void): void;   // Ctrl+Z / SIGTSTP
  onResume(cb: () => void): void;    // SIGCONT
  onInterrupt(cb: () => void): void; // Ctrl+C
}
```

Windows Ctrl+C 双路径：

```typescript
// 路径 1: 键盘事件（crossterm KeyEvent → handleInput → 二次确认）
// 路径 2: OS 信号（process.on("SIGINT") → 单次退出）
// 防抖：信号到达后 500ms 内忽略键盘 Ctrl+C
```

#### 6.2.4 ConfigWatcher (`core/config-watcher.ts`，新建)

```typescript
export class ConfigWatcher {
  watch(path: string, cb: () => void): void;
  unwatch(): void;
}
// 内部使用 fs.watchFile（500ms 轮询兼容性更好）
// 仅在 Idle 状态且无模态框时触发回调
```

#### 6.2.5 ResizeEventBatcher

```typescript
// 在 TUI.handleInput 中：
// 1. 从输入缓冲 drain 连续 ResizeEvent
// 2. 只保留最后一个尺寸
// 3. 非 Resize 事件缓存后统一处理
```

### 6.3 文件改动

| 路径 | 操作 | 说明 |
|------|------|------|
| `apps/cli-app/src/tui/core/terminal-detector.ts` | 新建 | 终端能力检测 |
| `apps/cli-app/src/tui/core/terminal-guard.ts` | 新建 | RAII 终端保护 |
| `apps/cli-app/src/tui/core/signal-handler.ts` | 新建 | 信号处理 |
| `apps/cli-app/src/tui/core/config-watcher.ts` | 新建 | 配置热重载 |
| `apps/cli-app/src/tui/core/terminal.ts` | 修改 | 集成检测结果，控制降级 |
| `apps/cli-app/src/tui/core/tui.ts` | 修改 | 集成 TerminalGuard / SignalHandler |
| `apps/cli-app/src/tui/app.ts` | 修改 | 启动时 arm TerminalGuard |

### 6.4 验证

- legacyConhost 检测后 PlainRenderer 被使用
- Ctrl+Z 挂起后光标恢复，fg 后正常
- 配置变更后触发重绘回调
- 崩溃后终端恢复 raw mode
- Resize 事件不会触发 30+ 次/秒的重绘

---

## 7. Phase 5 —— 业务组件补齐

### 7.1 目标

补齐 Markdown 渲染器、Diff 面板、主题系统等业务组件。

### 7.2 变更清单

#### 7.2.1 MarkdownRenderer (`components/markdown-renderer.ts`，新建)

```typescript
export class MarkdownRenderer {
  render(markdown: string, width: number): string[];
  // 支持段落/标题列表/代码块/引用/链接/粗斜体
  // 代码块使用语法高亮（如可用）
  // 无 HTML 渲染
}
```

依赖策略：
- 优先使用 `marked` 解析（如已在项目依赖中）
- 仅支持标准 Markdown 子集（标题、列表、代码块、引用、链接、粗斜体）
- 输出有限宽度文本行（非 ANSI 富文本）

#### 7.2.2 DiffPanel (`components/diff-panel.ts`，新建)

```typescript
export class DiffPanel implements Component {
  render(width: number): string[];
  // 解析统一 diff 格式
  // 使用 +/- 前缀着色
  // 支持滚动查看大 diff
}
```

#### 7.2.3 ThemeSystem (`core/theme.ts`，新建)

```typescript
export enum ColorRole {
  Default = "default",
  Muted = "muted",
  Brand = "brand",
  Add = "add",
  Remove = "remove",
  Warning = "warning",
  Error = "error",
  Highlight = "highlight",
}

export interface Theme {
  foreground: string;       // ANSI color code
  background: string;
  roles: Record<ColorRole, string>;
}
```

集成终端检测，根据 `isLightTheme` 返回不同颜色值。

### 7.3 文件改动

| 路径 | 操作 | 说明 |
|------|------|------|
| `apps/cli-app/src/tui/components/markdown-renderer.ts` | 新建 | Markdown 渲染 |
| `apps/cli-app/src/tui/components/diff-panel.ts` | 新建 | Diff 面板 |
| `apps/cli-app/src/tui/core/theme.ts` | 新建 | 主题系统 |
| `apps/cli-app/src/tui/components/index.ts` | 修改 | 导出新组件 |

### 7.4 验证

- Markdown 标题/列表/代码块/链接正确渲染
- Diff 面板 +/- 着色正确
- 主题切换后颜色角色映射正确

---

## 8. 影响分析

### 8.1 当前项目受影响的模块

| 模块 | Phase | 影响 | 缓解 |
|------|-------|------|------|
| `TUI` 类构造函数 | P1 | Renderer 作为必选参数 | 默认使用 RetainedRenderer |
| `CLIAppTUI` 启动 | P1 | 需要传入 Renderer | 自动检测 TTY 选择 Renderer |
| `AgentScreen` | P2 | 输入路由增加 phase 判断 | 增量修改，保留原有 onInput |
| `WorkflowScreen` | P3 | ConfirmBox → ConfirmModal | 接口兼容封装 |
| `ProcessTerminal` | P4 | 集成降级策略 | 检测后配置 flags |
| `core/index.ts` | 全部 | 持续增加导出 | 按需导入 |

### 8.2 无影响模块

- `Screen` 接口（`render()` / `handleInput()` / `onActivate()` / `onDeactivate()` / `destroy()`）不受影响
- `Component` 接口（`render(width)` / `handleInput(data)` / `invalidate()`）不受影响
- `Container` 组件树不受影响
- `Editor`、`Input`、`SelectList` 等组件不受影响
- `Keybindings`、`StdinBuffer`、`fuzzy`、`kill-ring`、`undo-stack` 不受影响

### 8.3 向后兼容性

| 变更 | 兼容性 |
|------|--------|
| `TUI` 构造函数增加 Renderer 参数 | ⚠️ 需修改构造调用 |
| UiPhase 枚举新增 | ✅ 非破坏性 |
| ModalManager 新增 | ✅ 非破坏性（可选集成） |
| TerminalGuard arm() | ✅ 非破坏性（新增行为） |
| 主题系统新增 | ✅ 非破坏性（默认使用旧颜色） |

---

## 9. 实施路线图

```
Phase 1: 事件循环与渲染架构重构 (估计 2-3 周)
├── Week 1: Renderer 接口 + RetainedRenderer 提取
├── Week 2: PlainRenderer + EventScheduler
└── Week 3: TUI 类重构 + 集成测试

Phase 2: UiPhase 状态机与输入路由 (1-2 周)
├── Week 1: UiPhase 枚举 + UiPhaseManager
└── Week 2: TypeAheadQueue + 输入路由重构

Phase 3: 模态框系统 (2-3 周)
├── Week 1: Modal 接口 + ModalManager + ConfirmModal
├── Week 2: PasswordModal + ModelPicker + SessionPicker
└── Week 3: FileViewer + DiffViewer + 集成

Phase 4: 终端兼容性与边界保护 (1-2 周)
├── Week 1: TerminalDetector + TerminalGuard
└── Week 2: SignalHandler + ConfigWatcher + Resize 合并

Phase 5: 业务组件补齐 (1-2 周)
├── Week 1: MarkdownRenderer + DiffPanel
└── Week 2: ThemeSystem + 集成
```

**总计估计：7-12 周**，其中 Phase 1-3 为核心改造（5-8 周），Phase 4-5 为增强（2-4 周）。

### 关键依赖

| 前置 | 后置 | 原因 |
|------|------|------|
| Phase 1 (Renderer) | Phase 2-5 | 渲染抽象是架构基础 |
| Phase 1 (EventScheduler) | Phase 2 (Spinner tick) | Spinner 依赖调度器 |
| Phase 2 (UiPhase) | Phase 3 (orphan modal 清理) | 孤儿清理依赖阶段转换事件 |
| Phase 2 (UiPhase) | Phase 4 (ConfigWatcher) | 配置重载仅在 Idle 执行 |
| Phase 1 (Terminal) | Phase 4 (TerminalDetector) | 检测结果影响 Terminal 初始化 |

---

## 附录 A：文件改动总清单

| 路径 | Phase | 操作 |
|------|-------|------|
| `apps/cli-app/src/tui/core/renderer.ts` | P1 | 新建 |
| `apps/cli-app/src/tui/core/retained-renderer.ts` | P1 | 新建 |
| `apps/cli-app/src/tui/core/plain-renderer.ts` | P1 | 新建 |
| `apps/cli-app/src/tui/core/event-scheduler.ts` | P1 | 新建 |
| `apps/cli-app/src/tui/core/uiphase.ts` | P2 | 新建 |
| `apps/cli-app/src/tui/core/typeahead-queue.ts` | P2 | 新建 |
| `apps/cli-app/src/tui/modals/modal.ts` | P3 | 新建 |
| `apps/cli-app/src/tui/modals/modal-manager.ts` | P3 | 新建 |
| `apps/cli-app/src/tui/modals/password-modal.ts` | P3 | 新建 |
| `apps/cli-app/src/tui/modals/confirm-modal.ts` | P3 | 新建 |
| `apps/cli-app/src/tui/modals/model-picker.ts` | P3 | 新建 |
| `apps/cli-app/src/tui/modals/session-picker.ts` | P3 | 新建 |
| `apps/cli-app/src/tui/modals/file-viewer.ts` | P3 | 新建 |
| `apps/cli-app/src/tui/modals/diff-viewer.ts` | P3 | 新建 |
| `apps/cli-app/src/tui/modals/index.ts` | P3 | 新建 |
| `apps/cli-app/src/tui/core/terminal-detector.ts` | P4 | 新建 |
| `apps/cli-app/src/tui/core/terminal-guard.ts` | P4 | 新建 |
| `apps/cli-app/src/tui/core/signal-handler.ts` | P4 | 新建 |
| `apps/cli-app/src/tui/core/config-watcher.ts` | P4 | 新建 |
| `apps/cli-app/src/tui/core/theme.ts` | P5 | 新建 |
| `apps/cli-app/src/tui/components/markdown-renderer.ts` | P5 | 新建 |
| `apps/cli-app/src/tui/components/diff-panel.ts` | P5 | 新建 |
| `apps/cli-app/src/tui/core/tui.ts` | P1/P2/P4 | 重构 |
| `apps/cli-app/src/tui/core/terminal.ts` | P4 | 修改 |
| `apps/cli-app/src/tui/core/index.ts` | 全部 | 持续修改 |
| `apps/cli-app/src/tui/app.ts` | P1/P4 | 修改 |
| `apps/cli-app/src/tui/screens/agent-screen.ts` | P2/P3 | 修改 |
| `apps/cli-app/src/tui/screens/workflow-screen.ts` | P3 | 修改 |
