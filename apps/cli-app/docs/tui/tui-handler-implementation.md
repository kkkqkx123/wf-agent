# TUI Handler Implementation Design

## 概述

本文档描述 `TUIHandler` 的完整实现方案。该 Handler 负责将 SDK 发出的 Component Messages 路由到 TUI 界面的各个 Screen 组件，实现实时的状态更新和事件显示。

**当前状态**: 空实现（仅定义了白名单和 switch-case 骨架）  
**目标状态**: 完整的消息分发系统，支持实时 TUI 渲染

---

## 架构设计

### 1. 整体流程

```
SDK (MessageBus) 
  ↓ publish(message)
Routing Rules (CLI_ROUTING_RULES)
  ↓ decide output targets
TUIHandler.handle(message)
  ↓ route by message.type
Screen Components (AgentScreen/WorkflowScreen/etc.)
  ↓ update UI state
TUI.render() → Terminal Display
```

### 2. 核心职责

`TUIHandler` 的职责：
- **消息过滤**: 通过 `supports()` 方法白名单过滤
- **消息路由**: 根据 `message.type` 分发到对应的 Screen
- **状态同步**: 调用 Screen 的公开 API 更新内部状态
- **性能优化**: 避免不必要的重渲染，使用增量更新

**注意**: `TUIHandler` **不直接操作 UI 组件**，而是通过 Screen 实例的公开方法进行间接更新。

---

## 消息类型映射

### 支持的 Message Types

根据 `supports()` 方法的白名单：

| Message Type | Category | Target Screen | 用途 |
|-------------|----------|---------------|------|
| `agent.llm.stream` | AGENT | AgentScreen | LLM 流式输出 |
| `agent.tool.call_start` | AGENT | AgentScreen | 工具调用开始 |
| `agent.tool.call_end` | AGENT | AgentScreen | 工具调用结束 |
| `agent.human_relay.request` | HUMAN_RELAY | AgentScreen (Overlay) | 人工中继请求 |
| `agent.iteration.start` | AGENT | AgentScreen | 迭代开始 |
| `workflow-execution.node.start` | WORKFLOW_EXECUTION | WorkflowScreen | 节点执行开始 |
| `workflow-execution.node.end` | WORKFLOW_EXECUTION | WorkflowScreen | 节点执行结束 |
| `system.error` | SYSTEM | All Screens (Toast) | 系统错误通知 |

---

## 实现方案

### 方案 A: Screen Registry + Direct Method Calls（推荐）

#### 设计思路

在 `TUIHandler` 中维护一个 Screen 注册表，通过 Screen ID 获取对应的 Screen 实例，然后调用其公开的状态更新方法。

#### 优点
- ✅ 简单直接，易于理解和调试
- ✅ 类型安全（TypeScript 编译时检查）
- ✅ 性能好（直接方法调用，无额外抽象层）
- ✅ 与现有架构一致（参考 `TUIHumanRelayHandler` 模式）

#### 缺点
- ❌ 需要手动维护 Screen 注册表
- ❌ Screen 之间耦合度略高（需要暴露公开 API）

#### 实现步骤

##### Step 1: 扩展 Screen 接口，添加消息处理方法

为每个 Screen 定义明确的消息处理 API：

```typescript
// AgentScreen 新增方法
export class AgentScreen implements Screen {
  
  /**
   * Handle agent-related messages
   */
  handleAgentMessage(message: BaseComponentMessage): void {
    switch (message.type) {
      case AgentMessageType.LLM_STREAM:
        this.handleLLMStream(message.data as AgentLLMStreamData);
        break;
      case AgentMessageType.TOOL_CALL_START:
        this.handleToolCallStart(message.data as AgentToolCallData);
        break;
      // ... other types
    }
  }
  
  private handleLLMStream(data: AgentLLMStreamData): void {
    if (data.chunk) {
      this.streamingBuffer += data.chunk;
      // Throttle rendering for performance
      const now = Date.now();
      if (now - this.lastRenderTime > 100 || this.streamingBuffer.length > 200) {
        this.appendLog(this.streamingBuffer, "assistant", { stream: true });
        this.streamingBuffer = "";
        this.lastRenderTime = now;
      }
    }
  }
  
  private handleToolCallStart(data: AgentToolCallData): void {
    this.toolCallPanel.handleToolCallStart(data);
    this.appendLog(`Calling tool: ${data.toolName}`, "tool");
  }
  
  // ... other handlers
}
```

```typescript
// WorkflowScreen 新增方法
export class WorkflowScreen implements Screen {
  
  /**
   * Handle workflow execution messages
   */
  handleWorkflowExecutionMessage(message: BaseComponentMessage): void {
    switch (message.type) {
      case WorkflowExecutionMessageType.NODE_START:
        this.handleNodeStart(message.data as WorkflowExecutionNodeData);
        break;
      case WorkflowExecutionMessageType.NODE_END:
        this.handleNodeEnd(message.data as WorkflowExecutionNodeData);
        break;
    }
  }
  
  private handleNodeStart(data: WorkflowExecutionNodeData): void {
    this.appendLog(`Node started: ${data.nodeId} (${data.nodeType})`, "system");
    // Update node status in visualization (future enhancement)
  }
  
  private handleNodeEnd(data: WorkflowExecutionNodeData): void {
    this.appendLog(
      `Node completed: ${data.nodeId} (${data.duration}ms)`,
      "system"
    );
  }
}
```

##### Step 2: 修改 TUIHandler，添加 Screen 引用

```typescript
export class TUIHandler implements OutputHandler {
  readonly target = OutputTarget.TUI;
  readonly name = "tui";

  private screens: Map<string, Screen> = new Map();

  constructor(private tui: TUI) {}

  /**
   * Register a screen instance for message routing
   */
  registerScreen(screenId: string, screen: Screen): void {
    this.screens.set(screenId, screen);
  }

  /**
   * Unregister a screen instance
   */
  unregisterScreen(screenId: string): void {
    this.screens.delete(screenId);
  }

  supports(message: BaseComponentMessage): boolean {
    const supportedTypes = new Set([
      "agent.llm.stream",
      "agent.tool.call_start",
      "agent.tool.call_end",
      "agent.human_relay.request",
      "agent.iteration.start",
      "workflow-execution.node.start",
      "workflow-execution.node.end",
      "system.error",
    ]);

    return supportedTypes.has(message.type);
  }

  async handle(message: BaseComponentMessage): Promise<void> {
    switch (message.type) {
      case "agent.llm.stream":
      case "agent.tool.call_start":
      case "agent.tool.call_end":
      case "agent.human_relay.request":
      case "agent.iteration.start":
        // Route to AgentScreen
        const agentScreen = this.screens.get("agent") as AgentScreen | undefined;
        if (agentScreen?.handleAgentMessage) {
          agentScreen.handleAgentMessage(message);
        }
        break;

      case "workflow-execution.node.start":
      case "workflow-execution.node.end":
        // Route to WorkflowScreen
        const workflowScreen = this.screens.get("workflow") as WorkflowScreen | undefined;
        if (workflowScreen?.handleWorkflowExecutionMessage) {
          workflowScreen.handleWorkflowExecutionMessage(message);
        }
        break;

      case "system.error":
        // Show error toast/notification on all active screens
        this.showErrorNotification(message);
        break;
    }
  }

  /**
   * Show error notification overlay
   */
  private showErrorNotification(message: BaseComponentMessage): void {
    const errorData = message.data as SystemErrorData;
    
    const overlay = new Box(1, 1);
    overlay.addChild(new Text("❌ System Error", 0, 0));
    overlay.addChild(new Text(errorData.message, 0, 0));
    if (errorData.stack) {
      overlay.addChild(new Text("Stack trace available in logs", 0, 0));
    }
    
    const handle = this.tui.showOverlay(overlay, {
      anchor: "center",
      nonCapturing: false,
    });
    
    // Auto-hide after 5 seconds
    setTimeout(() => handle.hide(), 5000);
  }
}
```

##### Step 3: 在 CLIAppTUI 中注册 Screens

```typescript
// apps/cli-app/src/tui/app.ts

export class CLIAppTUI {
  private tuiHandler: TUIHandler;
  
  constructor() {
    // ... existing initialization ...
    
    // Initialize TUI handler
    this.tuiHandler = new TUIHandler(this.tui);
    
    // Initialize screens
    this.initializeScreens();
    
    // Register screens with TUI handler
    this.registerScreensWithHandler();
    
    // Register message handlers
    this.initializeMessageHandlers();
  }
  
  private initializeScreens() {
    
    const dashboardScreen = new DashboardScreen(this.messageBus, (screenId) => {
      this.showScreen(screenId);
    });
    this.screens.set("dashboard", dashboardScreen);

    const workflowScreen = new WorkflowScreen(this.messageBus, () => {
      this.showScreen("dashboard");
    });
    this.screens.set("workflow", workflowScreen);

    const agentScreen = new AgentScreen(this.messageBus, () => {
      this.showScreen("dashboard");
    });
    this.screens.set("agent", agentScreen);
  }
  
  private registerScreensWithHandler() {
    // Register all screens with TUI handler for message routing
    this.tuiHandler.registerScreen("agent", this.screens.get("agent")!);
    this.tuiHandler.registerScreen("workflow", this.screens.get("workflow")!);
    // Dashboard and other screens can be registered as needed
  }
  
  private initializeMessageHandlers() {
    // Register TUI handler (already has screen references)
    this.messageBus.registerHandler(this.tuiHandler);
  
    // Register file handlers
    this.messageBus.registerHandler(new FunctionalFileHandler(this.humanRelayService));
    this.messageBus.registerHandler(new DisplayFileHandler(this.displayOutputService));
  }
}
```

---

## 推荐方案总结

**采用方案 A（Screen Registry + Direct Method Calls）**

理由：
1. **简单性优先**: 代码直观易懂，降低维护成本
2. **与现有架构一致**: 参考 `TUIHumanRelayHandler` 的成功模式
3. **性能最优**: 直接方法调用，无额外抽象层
4. **渐进式演进**: 未来可以平滑升级到方案 B 或 C

---

## 实施计划

### Phase 1: 基础实现（1-2 天）

1. ✅ 为 `AgentScreen` 添加 `handleAgentMessage()` 方法
2. ✅ 为 `WorkflowScreen` 添加 `handleWorkflowExecutionMessage()` 方法
3. ✅ 修改 `TUIHandler` 实现 Screen Registry
4. ✅ 在 `CLIAppTUI` 中注册 Screens
5. ✅ 实现基本的消息路由逻辑

**验收标准**:
- [ ] LLM 流式输出能实时显示在 AgentScreen
- [ ] 工具调用事件能在 AgentScreen 显示
- [ ] 工作流节点事件能在 WorkflowScreen 显示

### Phase 2: Human Relay 集成（1 天）

1. ✅ 实现 `agent.human_relay.request` 的处理
2. ✅ 复用现有的 `TUIHumanRelayHandler` 逻辑
3. ✅ 在 AgentScreen 中显示 Human Relay Overlay

**验收标准**:
- [ ] Human Relay 请求能正确触发 Overlay
- [ ] 文件监视功能正常工作
- [ ] 用户输入后能自动关闭 Overlay

### Phase 3: 错误处理和边界情况（1 天）

1. ✅ 实现 `system.error` 的错误通知
2. ✅ 处理 Screen 未注册的情况
3. ✅ 处理消息处理异常（try-catch）
4. ✅ 添加日志记录

**验收标准**:
- [ ] 系统错误能显示 Toast 通知
- [ ] 异常不会导致 TUI 崩溃
- [ ] 关键操作有日志记录

### Phase 4: 性能优化（可选，1-2 天）

1. ⚠️ 实现消息节流（throttling）
2. ⚠️ 实现批量更新（batching）
3. ⚠️ 优化流式输出的渲染频率
4. ⚠️ 添加性能监控

**验收标准**:
- [ ] 高频消息不会造成 UI 卡顿
- [ ] 流式输出流畅无明显延迟
- [ ] CPU 占用率在合理范围

---

## 关键技术点

### 1. 性能优化策略

#### 流式输出节流

```typescript
private handleLLMStream(data: AgentLLMStreamData): void {
  if (data.chunk) {
    this.streamingBuffer += data.chunk;
    
    const now = Date.now();
    // Render every 100ms or when buffer reaches 200 chars
    if (now - this.lastRenderTime > 100 || this.streamingBuffer.length > 200) {
      this.appendLog(this.streamingBuffer, "assistant", { stream: true });
      this.streamingBuffer = "";
      this.lastRenderTime = now;
    }
  }
}
```

#### 增量更新 vs 全量重建

```typescript
// For streaming updates - append only (fast)
if (options?.stream) {
  this.logEntries.push(entry);
  const formatted = `[${timeStr}] ${typeIcon} ${message}`;
  this.logPanel.addChild(new Text(formatted, 1, 0));
} else {
  // For non-streaming - rebuild entire panel (slower but consistent)
  this.logEntries.push(entry);
  this.logPanel.clear();
  this.logEntries.forEach(entry => {
    this.logPanel.addChild(new Text(formattedEntry, 1, 0));
  });
}
```

### 2. 线程安全考虑

由于 `MessageBus` 可能在异步上下文中调用 `handle()`，需要确保：
- Screen 的状态更新是原子的
- 避免竞态条件（race conditions）
- 使用 `requestRender()` 触发 TUI 重绘

```typescript
async handle(message: BaseComponentMessage): Promise<void> {
  try {
    // Update screen state
    agentScreen.handleAgentMessage(message);
    
    // Request TUI re-render (thread-safe)
    this.tui.requestRender();
  } catch (error) {
    logger.error("Failed to handle message", { error });
  }
}
```

### 3. 内存管理

#### 订阅清理

```typescript
// In Screen.destroy()
destroy(): void {
  // Cleanup subscriptions
  this.subscriptions.forEach(subscription => subscription.unsubscribe());
  this.subscriptions = [];
  
  // Clear buffers
  this.streamingBuffer = "";
  this.logEntries = [];
}
```

#### 日志条目限制

```typescript
// Keep only last 50-100 entries to prevent memory leak
if (this.logEntries.length > 100) {
  this.logEntries.shift(); // Remove oldest entry
}
```

---

## 测试策略

### 单元测试

```typescript
describe("TUIHandler", () => {
  let handler: TUIHandler;
  let mockTUI: MockTUI;
  let mockAgentScreen: MockAgentScreen;
  
  beforeEach(() => {
    mockTUI = new MockTUI();
    handler = new TUIHandler(mockTUI);
    mockAgentScreen = new MockAgentScreen();
    handler.registerScreen("agent", mockAgentScreen);
  });
  
  it("should route LLM stream messages to AgentScreen", async () => {
    const message = createMockMessage({
      type: "agent.llm.stream",
      data: { chunk: "Hello" },
    });
    
    await handler.handle(message);
    
    expect(mockAgentScreen.handleAgentMessage).toHaveBeenCalledWith(message);
  });
  
  it("should ignore unsupported message types", async () => {
    const message = createMockMessage({
      type: "unsupported.type",
    });
    
    expect(handler.supports(message)).toBe(false);
  });
});
```

### 集成测试

```typescript
describe("TUI Message Flow Integration", () => {
  it("should display LLM streaming output in real-time", async () => {
    // Setup
    const tui = new CLIAppTUI();
    tui.showScreen("agent");
    
    // Simulate message
    const messageBus = tui.getMessageBus();
    messageBus.publish(createLLMStreamMessage("Chunk 1"));
    messageBus.publish(createLLMStreamMessage("Chunk 2"));
    
    // Wait for render
    await waitForRender();
    
    // Verify
    const agentScreen = getActiveScreen();
    expect(agentScreen.getLogEntries()).toContain("Chunk 1");
    expect(agentScreen.getLogEntries()).toContain("Chunk 2");
  });
});
```

---

## 已知问题和 TODO

### 当前限制

1. **Dashboard Screen 未实现消息处理**
   - 需要添加实时状态汇总功能
   - 待 Phase 2 或后续版本实现

2. **错误通知样式简陋**
   - 当前仅使用简单的 Box + Text
   - 建议创建专门的 `ErrorToast` 组件

3. **没有消息历史回放**
   - 切换 Screen 后再切回，看不到之前的消息
   - 需要在 Screen 中持久化消息历史

### 未来增强

1. **消息过滤和搜索**
   - 在 AgentScreen 中添加消息过滤器
   - 支持按类型、时间范围搜索

2. **消息导出**
   - 支持将对话历史导出为 Markdown/JSON
   - 集成到 File Handler

3. **可配置的通知偏好**
   - 允许用户自定义哪些消息显示在 TUI
   - 保存到配置文件

4. **可视化增强**
   - 为工作流执行添加图形化进度条
   - 为 Agent 迭代添加统计图表

---

## 参考资料

- [TUI Core Architecture](../src/tui/core/tui.ts)
- [AgentScreen Implementation](../src/tui/screens/agent-screen.ts)
- [WorkflowScreen Implementation](../src/tui/screens/workflow-screen.ts)
- [TUIHumanRelayHandler](../src/tui/handlers/tui-human-relay-handler.ts)
- [MessageBus API](../../../sdk/api/shared/component-message/message-bus.ts)
- [Component Message Types](../../../packages/types/src/component-message/categories/index.ts)

---

## 变更日志

- **2026-05-16**: 初始设计文档创建
  - 完成三种方案的对比分析
  - 确定采用方案 A（Screen Registry）
  - 制定四阶段实施计划
