# CLI-App TUI 化改造设计方案

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

将 CLI-App 的交互模式全面升级为 TUI（Terminal User Interface），利用 `@mariozechner/pi-tui` 框架提供：

1. **统一的交互界面** - 所有操作通过 TUI 完成
2. **实时状态展示** - 动态更新的状态面板
3. **富文本编辑** - 支持 Markdown、代码高亮
4. **可视化选择** - 列表、表格、树形结构的可视化展示
5. **向导式配置** - 分步骤的配置流程

### 2.2 改造范围

| 模块 | 当前方式 | TUI 化后 | 优先级 |
|------|----------|----------|--------|
| 主界面 | 命令行 | 仪表盘 + 菜单导航 | 高 |
| Workflow 管理 | 命令参数 | 可视化列表 + 编辑器 | 高 |
| Agent Loop | 流式输出 | 实时状态面板 | 高 |
| Thread 执行 | 独立终端 | 内嵌执行视图 | 中 |
| Human Relay | readline | 富文本编辑器 | 高 |
| 配置管理 | 文件编辑 | 向导式表单 | 中 |
| 日志查看 | 文件查看 | 可过滤日志面板 | 中 |

## 三、详细设计方案

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

### 3.2 核心组件设计

#### 3.2.1 主应用框架

```typescript
// src/tui/app.ts
import { TUI, ProcessTerminal, Container, Box } from "@mariozechner/pi-tui";

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

#### 3.2.2 Dashboard 屏幕

```typescript
// src/tui/screens/dashboard-screen.ts
import { Container, Box, Text, SelectList } from "@mariozechner/pi-tui";

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

#### 3.2.3 Workflow 管理屏幕

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
    this.detailPanel.addChild(new Markdown(formatWorkflowDetail(workflow)));
  }
  
  render(): Component {
    return this.container;
  }
}
```

#### 3.2.4 Agent Loop 实时屏幕

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

#### 3.2.5 Human Relay TUI 处理器

```typescript
// src/tui/handlers/tui-human-relay-handler.ts
import type { HumanRelayHandler, HumanRelayRequest, HumanRelayResponse } from "@wf-agent/types";
import { TUI, Box, Markdown, Editor, SelectList } from "@mariozechner/pi-tui";

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
      
      // 对话历史
      if (request.messages.length > 0) {
        overlay.addChild(new Text("Conversation History:", { style: "bold" }));
        for (const msg of request.messages) {
          const content = typeof msg.content === "string" 
            ? msg.content 
            : JSON.stringify(msg.content);
          overlay.addChild(new Markdown(`**${msg.role}**: ${content.substring(0, 200)}...`));
        }
        overlay.addChild(new Spacer());
      }
      
      // 当前提示
      overlay.addChild(new Text("Prompt:", { style: "bold" }));
      overlay.addChild(new Markdown(request.prompt));
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

## 四、实施计划

### 4.1 阶段划分

#### Phase 1: 基础框架 (1-2 周)
- [ ] 集成 `@mariozechner/pi-tui` 依赖
- [ ] 创建 TUI 应用主框架
- [ ] 实现 Dashboard 和导航
- [ ] 基础键盘事件处理

#### Phase 2: Workflow 模块 (1 周)
- [ ] Workflow 列表视图
- [ ] Workflow 详情面板
- [ ] 文件选择器组件
- [ ] 注册/删除操作

#### Phase 3: Agent Loop 模块 (1-2 周)
- [ ] Agent 实时状态面板
- [ ] 流式日志显示
- [ ] 消息输入编辑器
- [ ] 会话管理

#### Phase 4: Human Relay 升级 (1 周)
- [ ] TUIHumanRelayHandler 实现
- [ ] 富文本对话展示
- [ ] 多行编辑器集成

#### Phase 5: 其他模块 (1 周)
- [ ] Thread 执行视图
- [ ] Checkpoint 管理
- [ ] Settings 配置界面

#### Phase 6: 优化与测试 (1 周)
- [ ] 性能优化
- [ ] 无障碍支持
- [ ] 集成测试

### 4.2 目录结构

```
src/
├── tui/
│   ├── app.ts                 # TUI 应用主入口
│   ├── screens/               # 屏幕页面
│   │   ├── dashboard-screen.ts
│   │   ├── workflow-screen.ts
│   │   ├── agent-screen.ts
│   │   ├── thread-screen.ts
│   │   ├── checkpoint-screen.ts
│   │   └── settings-screen.ts
│   ├── components/            # 可复用组件
│   │   ├── file-picker.ts
│   │   ├── confirmation-dialog.ts
│   │   ├── status-bar.ts
│   │   └── log-viewer.ts
│   ├── handlers/              # TUI 处理器
│   │   └── tui-human-relay-handler.ts
│   └── theme.ts               # 主题配置
├── commands/                  # 保留命令行入口（向后兼容）
├── adapters/                  # 复用现有适配器
└── ...
```

### 4.3 向后兼容

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

## 五、预期收益

1. **用户体验提升**
   - 统一的交互界面
   - 实时状态反馈
   - 可视化操作

2. **开发效率提升**
   - 组件化开发
   - 复用 pi-tui 组件
   - 清晰的架构分层

3. **维护性提升**
   - 代码结构清晰
   - 易于扩展新功能
   - 统一的错误处理

## 六、风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| TUI 库兼容性问题 | 高 | 充分测试，准备回退方案 |
| 性能问题（大日志） | 中 | 虚拟滚动，日志截断 |
| 终端兼容性 | 中 | 检测终端能力，优雅降级 |
| 学习成本 | 低 | 提供文档和示例 |
