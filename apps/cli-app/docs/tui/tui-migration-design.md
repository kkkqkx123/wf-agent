# CLI-App TUI 化改造设计方案

## 重要说明

**本方案基于 `ref/pi/tui` 目录中的参考实现，构建自有的 TUI 框架，而非直接使用外部库。**

### 关键决策

1. **自有实现**: 从 `ref/pi/tui/src` 移植核心代码，完全控制源码
2. **无 Markdown 渲染**: 不使用 marked 或任何 Markdown 解析库，直接输出原始文本
3. **差分渲染**: 实现高效的屏幕更新机制，减少闪烁
4. **跨平台支持**: 支持 Linux、macOS、Windows、Termux
5. **Kitty 协议**: 支持现代键盘协议，区分修饰键和按键事件

---

## 一、现状分析

### 1.1 当前架构

CLI-App 目前采用传统的命令行交互模式：

```
┌─────────────────────────────────────────────────────────────┐
│                      CLI-App 当前架构                        │
├─────────────────────────────────────────────────────────────┤
│  Commander.js                                               │
│    ├── workflow 命令组 (register/list/show/delete)          │
│    ├── agent 命令组 (run/start/pause/resume/stop)           │
│    ├── thread 命令组 (run/status/cancel/pause/resume)       │
│    ├── checkpoint 命令组                                    │
│    ├── template 命令组                                      │
│    ├── tool 命令组                                          │
│    ├── trigger 命令组                                       │
│    └── ...                                                  │
├─────────────────────────────────────────────────────────────┤
│  输出系统 (Output/Formatter/Logger)                         │
│    ├── 表格输出 (workflow list --table)                     │
│    ├── JSON 输出 (--json)                                   │
│    ├── 文本输出 (默认)                                      │
│    └── 流式输出 (agent run --stream)                        │
├─────────────────────────────────────────────────────────────┤
│  交互方式                                                   │
│    ├── 命令行参数                                           │
│    ├── 简单的 readline 交互 (human-relay)                   │
│    └── 独立终端执行 (thread run)                            │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 存在的问题

1. **用户体验不一致**
   - 不同命令的输出格式不统一
   - 缺乏实时状态更新
   - 长任务执行时没有进度显示

2. **交互能力有限**
   - 仅支持简单的命令行参数
   - 复杂的配置需要手动编辑文件
   - 缺乏可视化选择器

3. **信息展示受限**
   - 表格输出在终端宽度不足时显示混乱
   - 缺乏滚动、分页等浏览机制
   - 多步骤流程缺乏向导式界面

4. **Human Relay 体验差**
   - 使用简单的 readline 交互
   - 没有富文本展示能力
   - 不支持多行输入的友好编辑

## 二、TUI 化改造目标

### 2.1 核心目标

将 CLI-App 的交互模式全面升级为 TUI（Terminal User Interface），基于 `ref/pi/tui` 目录中的参考实现，构建自有的 TUI 框架，提供：

1. **统一的交互界面** - 所有操作通过 TUI 完成
2. **实时状态展示** - 动态更新的状态面板
3. **富文本编辑** - 支持多行输入和 ANSI 转义码
4. **可视化选择** - 列表、表格、树形结构的可视化展示
5. **向导式配置** - 分步骤的配置流程
6. **差分渲染** - 高效的屏幕更新机制

### 2.2 改造范围

| 模块 | 当前方式 | TUI 化后 | 优先级 |
|------|----------|----------|--------|
| 主界面 | 命令行 | 仪表盘 + 菜单导航 | 高 |
| Workflow 管理 | 命令参数 | 可视化列表 + 编辑器 | 高 |
| Agent Loop | 流式输出 | 实时状态面板 | 高 |
| Thread 执行 | 独立终端 | 内嵌执行视图 | 中 |
| Human Relay | readline | 多行文本编辑器 | 高 |
| 配置管理 | 文件编辑 | 向导式表单 | 中 |
| 日志查看 | 文件查看 | 可过滤日志面板 | 中 |

**注意**: 不集成 Markdown 渲染功能，直接使用原始文本输出。

## 三、pi-tui 架构分析

### 3.1 核心设计理念

参考实现 `ref/pi/tui/src` 展示了一个高性能 TUI 框架的核心设计：

#### 3.1.1 组件系统 (Component System)

**核心接口**:
```typescript
interface Component {
  render(width: number): string[];  // 将组件渲染为字符串数组
  handleInput?(data: string): void;  // 可选的键盘输入处理
  invalidate(): void;                // 清除缓存状态
  wantsKeyRelease?: boolean;         // 是否接收按键释放事件
}
```

**关键特性**:
- **纯函数式渲染**: `render()` 方法接收宽度参数，返回字符串数组，无副作用
- **差分渲染**: TUI 引擎对比前后两次渲染结果，只更新变化的行
- **组件组合**: `Container` 作为基础容器，可以嵌套任意子组件
- **焦点管理**: 通过 `Focusable` 接口支持硬件光标定位（用于 IME）

#### 3.1.2 终端抽象层 (Terminal Abstraction)

**Terminal 接口**:
```typescript
interface Terminal {
  start(onInput, onResize): void;    // 启动终端监听
  stop(): void;                      // 停止并恢复状态
  write(data: string): void;         // 写入输出
  get columns(): number;             // 终端列数
  get rows(): number;                // 终端行数
  hideCursor(): void;                // 隐藏光标
  showCursor(): void;                // 显示光标
  moveBy(lines: number): void;       // 相对移动光标
  clearLine(): void;                 // 清除当前行
  setTitle(title: string): void;     // 设置窗口标题
}
```

**ProcessTerminal 实现**:
- 启用 raw mode 捕获所有键盘输入
- 支持 Kitty 键盘协议（区分修饰键、重复按键、释放事件）
- 支持括号化粘贴模式（bracketed paste）
- 自动检测终端尺寸变化
- Windows 兼容：启用 VT 输入模式

#### 3.1.3 差分渲染引擎 (Differential Rendering)

**工作原理**:
1. **首次渲染**: 直接输出所有内容到屏幕
2. **后续渲染**:
   - 对比新旧渲染结果的每一行
   - 找出第一个和最后一个变化的行
   - 使用 ANSI 转义码移动光标到变化区域
   - 只重新渲染变化的行
   - 使用同步输出模式 (`\x1b[?2026h...\x1b[?2026l`) 避免闪烁

**优化策略**:
- **宽度变化**: 触发完整重绘（因为换行会改变）
- **高度变化**: 非 Termux 环境触发完整重绘
- **内容收缩**: 可配置是否清除空行
- **覆盖层合成**: 在差分比较前将 overlay 合并到主内容

#### 3.1.4 覆盖层系统 (Overlay System)

**OverlayOptions**:
```typescript
interface OverlayOptions {
  width?: SizeValue;          // 宽度（绝对值或百分比）
  maxHeight?: SizeValue;      // 最大高度
  anchor?: OverlayAnchor;     // 锚点位置（center, top-left等）
  offsetX?: number;           // 水平偏移
  offsetY?: number;           // 垂直偏移
  row?: SizeValue;            // 行位置（绝对值或百分比）
  col?: SizeValue;            // 列位置（绝对值或百分比）
  margin?: OverlayMargin;     // 边距
  visible?: (w, h) => boolean;// 可见性回调
  nonCapturing?: boolean;     // 是否捕获焦点
}
```

**OverlayHandle**:
```typescript
interface OverlayHandle {
  hide(): void;           // 永久移除
  setHidden(hidden): void;// 临时隐藏/显示
  focus(): void;          // 聚焦并置顶
  unfocus(): void;        // 释放焦点
  isFocused(): boolean;   // 检查焦点状态
}
```

**特性**:
- 支持多层覆盖层堆叠（按 focusOrder 排序）
- 自动焦点管理（新 overlay 获得焦点，隐藏时恢复）
- 灵活的定位和尺寸控制
- 动态可见性（基于终端尺寸或自定义逻辑）

#### 3.1.5 键盘绑定系统 (Keybindings)

**声明式定义**:
```typescript
const TUI_KEYBINDINGS = {
  "tui.editor.cursorUp": { defaultKeys: "up", description: "Move cursor up" },
  "tui.select.confirm": { defaultKeys: "enter", description: "Confirm selection" },
  "tui.input.submit": { defaultKeys: "enter", description: "Submit input" },
  // ... 更多绑定
};
```

**KeybindingsManager**:
- 支持用户自定义覆盖默认绑定
- 自动检测冲突（同一按键绑定多个动作）
- 提供 `matches(data, keybinding)` 方法进行匹配
- 支持组合键（如 `ctrl+shift+d`）

**Kitty 协议支持**:
- 区分按键按下、重复、释放事件
- 支持修饰键组合（Shift、Ctrl、Alt、Super）
- 支持非拉丁键盘布局（base layout key）

### 3.2 内置组件库

#### 3.2.1 基础组件

**Text**:
- 多行文本显示
- 自动词边界换行（word wrap）
- 支持 ANSI 转义码（颜色、样式）
- 可配置内边距和背景色
- 缓存渲染结果提升性能

**Box**:
- 容器组件，带内边距和背景
- 包含多个子组件
- 自动应用背景色到所有子组件
- 缓存优化：仅在内容或背景函数变化时重新渲染

**Spacer**:
- 空白占位符
- 用于布局中的间距控制

#### 3.2.2 交互组件

**SelectList**:
- 可滚动的项目列表
- 支持过滤（setFilter）
- 双列布局（label + description）
- 键盘导航（上下箭头、Enter 确认、Esc 取消）
- 自动计算最优列宽
- 显示滚动指示器（当前选中/总数）

**Editor**:
- 功能完整的多行文本编辑器
- 支持语法高亮（通过主题配置）
- Emacs/Vim 风格快捷键
- 撤销/重做栈（UndoStack）
- 杀环（Kill Ring）支持剪切板操作
- 自动补全支持（AutocompleteProvider）
- 粘贴标记合并（将大段粘贴内容视为原子单元）
- 智能词边界导航
- 硬件光标定位（用于 IME）

**Input**:
- 单行输入框
- 支持自动补全下拉列表
- 密码模式（隐藏输入）
- 历史命令导航

**SettingsList**:
- 设置项列表
- 支持布尔值、字符串、数字等类型
- 实时编辑和验证

#### 3.2.3 辅助组件

**Loader / CancellableLoader**:
- 加载动画（spinner）
- 支持取消操作
- 可自定义指示器样式

**Image**:
- 终端图片渲染
- 支持 Kitty 和 iTerm2 协议
- 自动检测终端能力
- GIF 动画支持

**Markdown** (本次改造不使用):
- Markdown 渲染（复杂，暂不集成）
- 代码块高亮
- 表格、列表支持

**TruncatedText**:
- 自动截断超长文本
- 支持省略号

### 3.3 工具函数

**文本处理**:
- `visibleWidth(text)`: 计算可见宽度（考虑宽字符、ANSI 码）
- `truncateToWidth(text, maxWidth, ellipsis)`: 截断文本到指定宽度
- `wrapTextWithAnsi(text, width)`: 保留 ANSI 码的智能换行
- `extractSegments(line, start, end, maxAfter)`: 提取行的分段（用于 overlay 合成）

**模糊搜索**:
- `fuzzyMatch(query, text)`: 模糊匹配算法
- `fuzzyFilter(query, items)`: 过滤项目列表

**键盘处理**:
- `parseKey(data)`: 解析键盘事件
- `matchesKey(data, keyId)`: 匹配按键
- `isKeyRelease(data)`: 判断是否为释放事件
- `decodePrintableKey(data)`: 解码可打印字符

### 3.4 架构优势

1. **高性能**: 差分渲染减少不必要的屏幕刷新
2. **低耦合**: 组件独立，易于测试和复用
3. **可扩展**: 自定义组件只需实现 `Component` 接口
4. **跨平台**: 支持 Linux、macOS、Windows、Termux
5. **现代化**: 支持 Kitty 协议、括号化粘贴、同步输出
6. **无障碍**: 硬件光标定位支持 IME 候选窗口

### 3.5 与我们的需求映射

| pi-tui 组件 | CLI-App 用途 | 是否需要 |
|-------------|--------------|----------|
| Text | 显示状态信息、日志 | ✅ 需要 |
| Box | 面板容器、边框 | ✅ 需要 |
| SelectList | Workflow 列表、菜单 | ✅ 需要 |
| Editor | Human Relay 输入、Agent 消息 | ✅ 需要 |
| Input | 搜索框、配置输入 | ✅ 需要 |
| Loader | 异步操作加载指示 | ✅ 需要 |
| Spacer | 布局间距 | ✅ 需要 |
| SettingsList | 配置界面 | ⚠️ 可选 |
| Markdown | 富文本展示 | ❌ 不需要 |
| Image | 图片展示 | ❌ 不需要 |

## 四、详细设计方案

### 3.1 整体架构

```
┌─────────────────────────────────────────────────────────────────────┐
│                      TUI-CLI 架构                                    │
├─────────────────────────────────────────────────────────────────────┤
│  TUI Application Layer                                               │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │  Main App (TUI Container)                                      │  │
│  │    ├── Header (状态栏、当前上下文)                              │  │
│  │    ├── Sidebar (导航菜单)                                      │  │
│  │    ├── Content Area (动态内容区)                               │  │
│  │    └── Footer (快捷键提示)                                     │  │
│  └───────────────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────────┤
│  Screen/Page Layer                                                   │
│  ┌──────────────┬──────────────┬──────────────┬──────────────────┐  │
│  │  Dashboard   │  Workflow    │  Agent Loop  │  Settings        │  │
│  │  Screen      │  Screen      │  Screen      │  Screen          │  │
│  └──────────────┴──────────────┴──────────────┴──────────────────┘  │
├─────────────────────────────────────────────────────────────────────┤
│  Component Layer (基于 pi-tui)                                       │
│  ┌────────────┬────────────┬────────────┬────────────┬────────────┐ │
│  │  Editor    │  SelectList│  Markdown  │  Box       │  Loader    │ │
│  │  Input     │  Settings  │  Image     │  Spacer    │  ...       │ │
│  └────────────┴────────────┴────────────┴────────────┴────────────┘ │
├─────────────────────────────────────────────────────────────────────┤
│  Adapter Layer (复用现有)                                            │
│  ┌──────────────┬──────────────┬──────────────┬──────────────────┐  │
│  │WorkflowAdapter│AgentLoopAdapter│ThreadAdapter│ 其他 Adapters   │  │
│  └──────────────┴──────────────┴──────────────┴──────────────────┘  │
├─────────────────────────────────────────────────────────────────────┤
│  SDK Layer                                                           │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │              @wf-agent/sdk                                  │  │
│  └───────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

### 4.2 核心组件设计

#### 4.2.1 主应用框架

```typescript
// src/tui/app.ts
import { TUI, ProcessTerminal, Container, Box } from "../tui/core/index.js";

export class CLIAppTUI {
  private tui: TUI;
  private terminal: ProcessTerminal;
  private mainContainer: Container;
  private currentScreen: string = "dashboard";
  
  // 屏幕注册表
  private screens: Map<string, Screen> = new Map();
  
  constructor() {
    this.terminal = new ProcessTerminal();
    this.tui = new TUI(this.terminal);
    this.mainContainer = new Container();
    
    this.initializeScreens();
    this.setupKeybindings();
  }
  
  private initializeScreens() {
    this.screens.set("dashboard", new DashboardScreen());
    this.screens.set("workflow", new WorkflowScreen());
    this.screens.set("agent", new AgentScreen());
    this.screens.set("thread", new ThreadScreen());
    this.screens.set("settings", new SettingsScreen());
  }
  
  public start() {
    this.tui.addChild(this.mainContainer);
    this.showScreen("dashboard");
    this.tui.start();
  }
  
  public showScreen(name: string) {
    this.currentScreen = name;
    const screen = this.screens.get(name);
    if (screen) {
      this.mainContainer.clear();
      this.mainContainer.addChild(screen.render());
      this.tui.requestRender();
    }
  }
}
```

#### 4.2.2 Dashboard 屏幕

```typescript
// src/tui/screens/dashboard-screen.ts
import { Container, Box, Text, SelectList } from "../tui/core/index.js";

export class DashboardScreen implements Screen {
  private container: Container;
  private menuList: SelectList;
  
  constructor() {
    this.container = new Container();
    this.setupLayout();
  }
  
  private setupLayout() {
    // 标题区域
    const header = new Box();
    header.addChild(new Text("Modular Agent CLI", { style: "bold" }));
    
    // 主菜单
    this.menuList = new SelectList({
      items: [
        { id: "workflow", label: "📋 Workflow Management", description: "Manage workflows" },
        { id: "agent", label: "🤖 Agent Loop", description: "Run and monitor agents" },
        { id: "thread", label: "🧵 Thread Execution", description: "Execute workflows" },
        { id: "checkpoint", label: "💾 Checkpoints", description: "Manage checkpoints" },
        { id: "settings", label: "⚙️  Settings", description: "Configure CLI" },
      ],
      onSelect: (item) => app.showScreen(item.id),
    });
    
    // 快捷状态面板
    const statusPanel = new Box({ border: true, title: "Quick Status" });
    statusPanel.addChild(new Text("Active Agents: 0"));
    statusPanel.addChild(new Text("Running Threads: 0"));
    statusPanel.addChild(new Text("Last Updated: --"));
    
    this.container.addChild(header);
    this.container.addChild(this.menuList);
    this.container.addChild(statusPanel);
  }
  
  render(): Component {
    return this.container;
  }
}
```

#### 4.2.3 Workflow 管理屏幕

```typescript
// src/tui/screens/workflow-screen.ts
export class WorkflowScreen implements Screen {
  private container: Container;
  private workflowList: SelectList;
  private detailPanel: Box;
  private adapter: WorkflowAdapter;
  
  constructor() {
    this.adapter = new WorkflowAdapter();
    this.setupLayout();
    this.loadWorkflows();
  }
  
  private setupLayout() {
    this.container = new Container();
    
    // 工具栏
    const toolbar = new Box();
    toolbar.addChild(new Text("[N]ew  [E]dit  [D]elete  [R]efresh  [B]ack", { style: "dim" }));
    
    // 分割布局：左侧列表，右侧详情
    const splitContainer = new Container();
    
    // 工作流列表
    this.workflowList = new SelectList({
      onSelect: (item) => this.showWorkflowDetail(item.id),
    });
    
    // 详情面板
    this.detailPanel = new Box({ border: true, title: "Workflow Details" });
    this.detailPanel.addChild(new Text("Select a workflow to view details"));
    
    splitContainer.addChild(this.workflowList);
    splitContainer.addChild(this.detailPanel);
    
    this.container.addChild(toolbar);
    this.container.addChild(splitContainer);
  }
  
  private async loadWorkflows() {
    const workflows = await this.adapter.listWorkflows();
    this.workflowList.setItems(
      workflows.map(w => ({
        id: w.id,
        label: w.name,
        description: `${w.status} | ${w.createdAt}`,
      }))
    );
  }
  
  private async showWorkflowDetail(id: string) {
    const workflow = await this.adapter.getWorkflow(id);
    this.detailPanel.clear();
    // 直接使用原始文本，不使用 Markdown 渲染
    this.detailPanel.addChild(new Text(formatWorkflowDetail(workflow)));
  }
  
  render(): Component {
    return this.container;
  }
}
```

#### 4.2.4 Agent Loop 实时屏幕

```typescript
// src/tui/screens/agent-screen.ts
export class AgentScreen implements Screen {
  private container: Container;
  private logPanel: Box;
  private statusPanel: Box;
  private adapter: AgentLoopAdapter;
  private currentAgentId?: string;
  
  constructor() {
    this.adapter = new AgentLoopAdapter();
    this.setupLayout();
  }
  
  private setupLayout() {
    this.container = new Container();
    
    // 状态面板
    this.statusPanel = new Box({ border: true, title: "Agent Status" });
    this.updateStatus("idle");
    
    // 日志面板（带滚动）
    this.logPanel = new Box({ 
      border: true, 
      title: "Execution Log",
      scrollable: true,
    });
    
    // 输入区域
    const inputBox = new Editor({
      placeholder: "Enter your message...",
      onSubmit: (text) => this.sendMessage(text),
    });
    
    this.container.addChild(this.statusPanel);
    this.container.addChild(this.logPanel);
    this.container.addChild(inputBox);
  }
  
  public async startAgent(config: AgentLoopConfig) {
    this.updateStatus("running");
    
    const result = await this.adapter.executeAgentLoopStream(
      config,
      {},
      (event) => this.handleEvent(event)
    );
    
    this.updateStatus(result.success ? "completed" : "error");
  }
  
  private handleEvent(event: any) {
    switch (event.type) {
      case "text":
        this.appendLog(event.delta, "assistant");
        break;
      case "tool_call_start":
        this.appendLog(`🔧 Calling: ${event.data?.toolCall?.function?.name}`, "system");
        break;
      case "tool_call_end":
        const icon = event.data?.success ? "✓" : "✗";
        this.appendLog(`${icon} Tool call completed`, "system");
        break;
      case "iteration_complete":
        this.updateIteration(event.data?.iteration);
        break;
    }
  }
  
  private appendLog(message: string, type: "user" | "assistant" | "system") {
    const styled = type === "user" ? chalk.blue(message) :
                   type === "assistant" ? chalk.green(message) :
                   chalk.gray(message);
    this.logPanel.addChild(new Text(styled));
    // 自动滚动到底部
    this.logPanel.scrollToBottom();
  }
  
  render(): Component {
    return this.container;
  }
}
```

#### 4.2.5 Human Relay TUI 处理器

```typescript
// src/tui/handlers/tui-human-relay-handler.ts
import type { HumanRelayHandler, HumanRelayRequest, HumanRelayResponse } from "@wf-agent/types";
import { TUI, Box, Text, Editor, SelectList } from "../tui/core/index.js";

export class TUIHumanRelayHandler implements HumanRelayHandler {
  private tui: TUI;
  
  constructor(tui: TUI) {
    this.tui = tui;
  }
  
  async handle(request: HumanRelayRequest, context: any): Promise<HumanRelayResponse> {
    return new Promise((resolve, reject) => {
      // 创建覆盖层
      const overlay = new Box({
        border: true,
        title: "🤝 Human Relay Request",
        width: "80%",
        maxHeight: "80%",
      });
      
      // 请求信息
      overlay.addChild(new Text(`Request ID: ${request.requestId}`));
      overlay.addChild(new Text(`Timeout: ${request.timeout}ms`));
      overlay.addChild(new Spacer());
      
      // 对话历史（直接使用原始文本，不使用 Markdown）
      if (request.messages.length > 0) {
        overlay.addChild(new Text("Conversation History:", { style: "bold" }));
        for (const msg of request.messages) {
          const content = typeof msg.content === "string" 
            ? msg.content 
            : JSON.stringify(msg.content);
          // 截断长消息，直接显示原始文本
          const truncated = content.length > 200 ? content.substring(0, 200) + "..." : content;
          overlay.addChild(new Text(`${msg.role}: ${truncated}`));
        }
        overlay.addChild(new Spacer());
      }
      
      // 当前提示
      overlay.addChild(new Text("Prompt:", { style: "bold" }));
      overlay.addChild(new Text(request.prompt));
      overlay.addChild(new Spacer());
      
      // 响应编辑器
      const editor = new Editor({
        multiline: true,
        placeholder: "Enter your response (Ctrl+Enter to submit, Esc to cancel)...",
        onSubmit: (text) => {
          handle.hide();
          resolve({
            requestId: request.requestId,
            content: text,
            timestamp: Date.now(),
          });
        },
      });
      
      overlay.addChild(new Text("Your Response:", { style: "bold" }));
      overlay.addChild(editor);
      
      // 显示覆盖层
      const handle = this.tui.showOverlay(overlay, {
        anchor: "center",
        nonCapturing: false,
      });
      
      // 聚焦编辑器
      handle.focus();
    });
  }
}
```

### 3.3 命令映射方案

| 原命令 | TUI 入口 | 交互方式 |
|--------|----------|----------|
| `workflow register <file>` | Workflow Screen → [N]ew | 文件选择器 + 表单 |
| `workflow list` | Workflow Screen | 可视化列表 |
| `workflow show <id>` | Workflow Screen → 选择项 | 详情面板 |
| `workflow delete <id>` | Workflow Screen → [D]elete | 确认对话框 |
| `agent run` | Agent Screen → New Session | 配置表单 + 实时视图 |
| `agent start` | Agent Screen → Background | 后台启动确认 |
| `thread run <wf>` | Thread Screen → Run | 工作流选择 + 参数表单 |
| `thread status <id>` | Thread Screen → 选择项 | 状态详情 |
| `checkpoint list` | Checkpoint Screen | 列表视图 |

### 3.4 键盘导航设计

```
全局快捷键:
  Ctrl+Q    - 退出应用
  Ctrl+D    - 返回 Dashboard
  F1        - 帮助
  Tab       - 在面板间切换
  ↑/↓       - 在列表中导航
  Enter     - 确认/选择
  Esc       - 取消/返回

Dashboard:
  ↑/↓       - 选择菜单项
  Enter     - 进入选中模块
  1-9       - 快速跳转

Workflow Screen:
  N         - 新建工作流
  E         - 编辑选中
  D         - 删除选中
  R         - 刷新列表
  B         - 返回
  /         - 搜索过滤

Agent Screen:
  Ctrl+N    - 新会话
  Ctrl+R    - 重新运行
  Ctrl+C    - 停止当前
  ↑/↓       - 滚动日志
```

## 五、实施计划

### 5.1 阶段划分

#### Phase 1: TUI 核心引擎 (2-3 周)
- [ ] 移植 Terminal 接口和 ProcessTerminal 实现
- [ ] 实现 TUI 渲染引擎（差分渲染）
- [ ] 实现 Component 接口和 Container
- [ ] 实现键盘绑定系统（KeybindingsManager）
- [ ] 实现工具函数（visibleWidth, truncateToWidth, wrapTextWithAnsi）
- [ ] 基础测试：终端启动/停止、渲染循环、键盘输入

#### Phase 2: 基础组件库 (1-2 周)
- [ ] Text 组件（支持 ANSI、自动换行）
- [ ] Box 组件（容器、内边距、背景色）
- [ ] Spacer 组件
- [ ] SelectList 组件（列表选择、过滤）
- [ ] Input 组件（单行输入）
- [ ] Loader 组件（加载动画）
- [ ] 组件单元测试

#### Phase 3: Editor 组件 (2-3 周)
- [ ] 多行文本编辑器核心
- [ ] 光标导航（上下左右、词边界）
- [ ] 文本编辑（插入、删除、剪切、复制、粘贴）
- [ ] 撤销/重做栈（UndoStack）
- [ ] 杀环（Kill Ring）
- [ ] 自动补全支持
- [ ] 硬件光标定位（IME 支持）
- [ ] 编辑器集成测试

#### Phase 4: 应用框架 (1 周)
- [ ] CLIAppTUI 主应用类
- [ ] Screen 接口和屏幕管理
- [ ] Dashboard 屏幕
- [ ] 键盘导航和快捷键
- [ ] 覆盖层系统（Overlay）

#### Phase 5: Workflow 模块 (1 周)
- [ ] Workflow 列表视图
- [ ] Workflow 详情面板
- [ ] 文件选择器组件
- [ ] 注册/删除操作

#### Phase 6: Agent Loop 模块 (1-2 周)
- [ ] Agent 实时状态面板
- [ ] 流式日志显示
- [ ] 消息输入编辑器
- [ ] 会话管理

#### Phase 7: Human Relay 升级 (1 周)
- [ ] TUIHumanRelayHandler 实现
- [ ] 多行对话展示
- [ ] Editor 集成

#### Phase 8: 其他模块 (1 周)
- [ ] Thread 执行视图
- [ ] Checkpoint 管理
- [ ] Settings 配置界面

#### Phase 9: 优化与测试 (1 周)
- [ ] 性能优化
- [ ] 无障碍支持
- [ ] 集成测试
- [ ] 跨平台兼容性测试

### 5.2 目录结构

```
src/
├── tui/
│   ├── core/                  # TUI 核心引擎（从 ref/pi/tui 移植）
│   │   ├── index.ts           # 导出所有核心接口
│   │   ├── tui.ts             # TUI 渲染引擎
│   │   ├── terminal.ts        # Terminal 接口和 ProcessTerminal
│   │   ├── keybindings.ts     # 键盘绑定系统
│   │   ├── keys.ts            # 键盘事件解析
│   │   ├── utils.ts           # 工具函数
│   │   ├── fuzzy.ts           # 模糊搜索
│   │   ├── autocomplete.ts    # 自动补全
│   │   ├── stdin-buffer.ts    # 输入缓冲
│   │   ├── kill-ring.ts       # 杀环
│   │   └── undo-stack.ts      # 撤销栈
│   ├── components/            # UI 组件
│   │   ├── text.ts            # Text 组件
│   │   ├── box.ts             # Box 容器
│   │   ├── spacer.ts          # Spacer
│   │   ├── select-list.ts     # SelectList
│   │   ├── input.ts           # Input
│   │   ├── editor.ts          # Editor
│   │   ├── loader.ts          # Loader
│   │   └── settings-list.ts   # SettingsList (可选)
│   ├── screens/               # 屏幕页面
│   │   ├── dashboard-screen.ts
│   │   ├── workflow-screen.ts
│   │   ├── agent-screen.ts
│   │   ├── thread-screen.ts
│   │   ├── checkpoint-screen.ts
│   │   └── settings-screen.ts
│   ├── handlers/              # TUI 处理器
│   │   └── tui-human-relay-handler.ts
│   └── theme.ts               # 主题配置
├── commands/                  # 保留命令行入口（向后兼容）
├── adapters/                  # 复用现有适配器
└── ...
```

### 5.3 向后兼容

```typescript
// src/index.ts
import { isHeadlessMode } from "./utils/exit-manager.js";

async function main() {
  // 检查运行模式
  if (isHeadlessMode() || process.argv.includes("--cli")) {
    // 传统命令行模式
    await runCLI();
  } else {
    // TUI 模式
    const { CLIAppTUI } = await import("./tui/app.js");
    const app = new CLIAppTUI();
    app.start();
  }
}
```

## 六、预期收益

1. **用户体验提升**
   - 统一的交互界面
   - 实时状态反馈
   - 可视化操作

2. **开发效率提升**
   - 组件化开发
   - 清晰的架构分层
   - 自有的 TUI 框架，无外部依赖

3. **维护性提升**
   - 代码结构清晰
   - 易于扩展新功能
   - 统一的错误处理
   - 完全控制源码，便于调试和优化

## 七、风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| TUI 引擎实现复杂度 | 高 | 充分参考 pi-tui 实现，分阶段开发 |
| 性能问题（大日志） | 中 | 虚拟滚动，日志截断，差分渲染优化 |
| 终端兼容性 | 中 | 检测终端能力，优雅降级，跨平台测试 |
| Editor 组件复杂性 | 高 | 优先实现基础功能，逐步增强 |
| 学习成本 | 低 | 提供文档和示例，基于成熟设计 |
| 开发周期延长 | 中 | 合理划分阶段，优先核心功能 |
