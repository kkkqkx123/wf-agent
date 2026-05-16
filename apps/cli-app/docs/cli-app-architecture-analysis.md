# CLI-App 架构分析与改进建议

## 📊 当前架构分析

### 1. 整体架构概览

```
┌─────────────────────────────────────────────────────────────┐
│                    CLI Application Layer                     │
├─────────────────────────────────────────────────────────────┤
│  Entry Point (index.ts)                                      │
│  ├── Command Registration (Commander.js)                     │
│  ├── SDK Initialization (createSDK)                          │
│  ├── Storage Manager                                         │
│  ├── Output System (logger, formatter)                       │
│  └── TUI Mode Support                                        │
├─────────────────────────────────────────────────────────────┤
│  Command Layer (commands/)                                   │
│  ├── workflow/          - Workflow CRUD operations           │
│  ├── workflow-execution/ - Execution management              │
│  ├── checkpoint/        - Checkpoint operations              │
│  ├── template/          - Template management                │
│  ├── tool/              - Tool registration                  │
│  ├── trigger/           - Trigger configuration              │
│  ├── agent/             - Agent loop commands                │
│  ├── skill/             - Skill management                   │
│  └── ... (16 command groups)                                 │
├─────────────────────────────────────────────────────────────┤
│  Adapter Layer (adapters/)                                   │
│  ├── BaseAdapter (SDK access, error handling)               │
│  ├── WorkflowAdapter                                          │
│  ├── WorkflowExecutionAdapter                                 │
│  ├── CheckpointAdapter                                        │
│  └── ... (18 adapters)                                       │
├─────────────────────────────────────────────────────────────┤
│  Service Layer (services/)                                   │
│  ├── Terminal Services                                        │
│  │   ├── TerminalManager (PTY management)                    │
│  │   ├── TaskExecutor (CLI command execution)                │
│  │   └── CommunicationBridge                                 │
│  └── IO Services                                              │
├─────────────────────────────────────────────────────────────┤
│  Utility Layer (utils/)                                      │
│  ├── Output System (stdout/stderr/log management)            │
│  ├── Error Handler                                            │
│  ├── Formatter (table/json/plain)                            │
│  ├── Logger                                                   │
│  └── Exit Manager                                             │
└─────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────┐
│                    SDK Layer (@wf-agent/sdk)                 │
│  ├── API Factory (command-based APIs)                        │
│  ├── Workflow Registry & Execution                           │
│  ├── Agent Loop Engine                                       │
│  ├── Tool System                                             │
│  └── Event System                                            │
└─────────────────────────────────────────────────────────────┘
```

### 2. 核心设计模式

#### 2.1 命令注册模式（Command Pattern）

```typescript
// index.ts - 全局命令注册
program.addCommand(createWorkflowCommands());
program.addCommand(createWorkflowExecutionCommands());
// ... 16个命令组

// commands/workflow/index.ts - 命令工厂
export function createWorkflowCommands(): Command {
  const cmd = new Command("workflow").description("Manage workflows");
  
  cmd.command("list")
    .action(async () => {
      const adapter = new WorkflowAdapter();
      const workflows = await adapter.listWorkflows();
      output.output(formatWorkflowList(workflows));
    });
}
```

**优点**：
- ✅ 清晰的命令分组
- ✅ 易于扩展新命令
- ✅ 统一的错误处理

**问题**：
- ❌ 每个 action 都创建新的 adapter 实例
- ❌ 命令逻辑与适配器耦合

#### 2.2 适配器模式（Adapter Pattern）

```typescript
// adapters/base-adapter.ts
export class BaseAdapter {
  protected sdk: SDKInstance;
  protected output: CLIOutput;
  
  constructor() {
    this.sdk = getSDKInstance(); // 全局单例访问
    this.output = getOutput();
  }
  
  protected async executeWithErrorHandling<T>(
    operation: () => Promise<T>,
    context: string
  ): Promise<T> {
    try {
      return await operation();
    } catch (error) {
      this.handleError(error, context);
    }
  }
}

// adapters/workflow-execution-adapter.ts
export class WorkflowExecutionAdapter extends BaseAdapter {
  async executeWorkflow(workflowId: string, input?: Record<string, unknown>) {
    return this.executeWithErrorHandling(async () => {
      const dependencies = this.sdk.getFactory().getDependencies();
      const command = new ExecuteWorkflowCommand({ workflowId, options: { input } }, dependencies);
      const result = await this.sdk.executeCommand(command);
      
      if (isFailure(result)) {
        throw getError(result);
      }
      
      return getData(result);
    }, "executeWorkflow");
  }
}
```

**优点**：
- ✅ 统一的 SDK 访问方式
- ✅ 内置错误处理和日志记录
- ✅ 封装 SDK 的复杂性

**问题**：
- ⚠️ 通过 `getSDKInstance()` 全局访问，隐式依赖
- ⚠️ 所有 adapter 共享同一个 SDK 实例

#### 2.3 终端隔离模式（Terminal Isolation Pattern）

```typescript
// services/terminal/terminal-manager.ts
export class TerminalManager {
  createTerminal(options: TerminalOptions): TerminalSession {
    if (options.background) {
      // 后台模式：使用 child_process.spawn
      return this.createBackgroundTerminal(...);
    } else {
      // 前台模式：使用 node-pty
      return this.createForegroundTerminal(...);
    }
  }
}

// services/terminal/task-executor.ts
export class TaskExecutor {
  async executeInTerminal(
    workflowId: string,
    input: Record<string, unknown>,
    terminal: TerminalSession
  ) {
    // 构建 CLI 命令字符串
    const command = `modular-agent execution run ${workflowId} --input '${JSON.stringify(input)}'`;
    
    // 在 PTY 中执行
    terminal.pty.write(command + "\r");
    
    return { taskId, status: "started", ... };
  }
}
```

**优点**：
- ✅ 进程级隔离，互不干扰
- ✅ 支持前后台两种模式
- ✅ 实时终端输出

**问题**：
- ❌ **TaskExecutor 完全未使用 SDK**（已修复）
- ❌ 通过 CLI 命令间接调用，性能开销大
- ❌ 命令字符串拼接存在注入风险

### 3. 数据流分析

#### 3.1 阻塞模式执行流程（Blocking Mode）

```
User Command
    ↓
index.ts (preAction hook)
    ↓ SDK Initialization
    ↓ Storage Setup
    ↓
workflow-execution/index.ts (--blocking flag)
    ↓
WorkflowExecutionAdapter.executeWorkflow()
    ↓
BaseAdapter → getSDKInstance()
    ↓
SDK.executeCommand(ExecuteWorkflowCommand)
    ↓
SDK Internal Processing
    ↓
Result → Formatter → Output
```

**特点**：
- ✅ 直接 SDK 调用，性能好
- ✅ 同步等待结果
- ✅ 适合脚本化自动化

#### 3.2 后台模式执行流程（Background Mode）

```
User Command
    ↓
index.ts (preAction hook)
    ↓ SDK Initialization (仅用于初始化检查)
    ↓
workflow-execution/index.ts (--background flag)
    ↓
TerminalManager.createTerminal({ background: true })
    ↓
TaskExecutor.executeInTerminal()
    ↓
Build CLI Command String
    ↓
terminal.pty.write(command)
    ↓
Spawn NEW modular-agent process
    ↓
    [New Process]
    ↓
    index.ts (preAction hook) ← 重复初始化！
    ↓
    SDK Initialization (第二次)
    ↓
    ExecuteWorkflowCommand
    ↓
    Result → Log File
```

**问题**：
- ❌ **双重 SDK 初始化**：父进程和子进程都初始化 SDK
- ❌ **资源浪费**：每次后台任务都启动完整 CLI 应用
- ❌ **状态不同步**：父子进程无法共享状态
- ❌ **调试困难**：错误分散在两个进程中

### 4. 关键问题分析

#### 问题 1：SDK 初始化的职责不清

**现状**：
```typescript
// index.ts - preAction hook
sdkInstance = createSDK({ ... });
await sdkInstance.waitForReady();

// 但 TaskExecutor 不使用它，而是 spawn 新进程
const command = `modular-agent execution run ${workflowId}`;
terminal.pty.write(command); // 这会触发第二次 SDK 初始化
```

**影响**：
- 后台模式下 SDK 被初始化两次
- 存储连接被打开两次
- 内存和资源浪费

#### 问题 2：Adapter 层与 Command 层的边界模糊

**现状**：
```typescript
// commands/workflow-execution/index.ts
.action(async (workflowId, options) => {
  if (options.blocking) {
    // 直接使用 Adapter
    const adapter = new WorkflowExecutionAdapter();
    const execution = await adapter.executeWorkflow(workflowId, inputData);
    output.output(formatWorkflowExecution(execution));
  } else {
    // 使用 Terminal + TaskExecutor
    const terminal = getTerminalManager().createTerminal({...});
    const result = await getTaskExecutor().executeInTerminal(workflowId, inputData, terminal);
  }
});
```

**问题**：
- Command 层需要了解两种执行模式的细节
- 违反了单一职责原则
- 难以测试和维护

#### 问题 3：全局状态的隐式依赖

**现状**：
```typescript
// adapters/base-adapter.ts
constructor() {
  this.sdk = getSDKInstance(); // 隐式依赖全局变量
  if (!sdk) {
    throw new Error("SDK instance not initialized...");
  }
}

// index.ts
let sdkInstance: SDKInstance | null = null;
export function getSDKInstance(): SDKInstance | null {
  return sdkInstance;
}
```

**问题**：
- 测试时需要 mock 全局函数
- 难以实现多实例场景
- 违反依赖注入原则

#### 问题 4：终端服务的职责过载

**现状**：
- `TerminalManager`：管理 PTY 会话
- `TaskExecutor`：跟踪任务状态 + 构建 CLI 命令
- `CommunicationBridge`：进程间通信（未充分使用）

**问题**：
- TaskExecutor 既不是真正的"执行器"（不执行工作流），也不是纯粹的"任务管理器"
- 命名与实际职责不符
- 与 WorkflowExecutionAdapter 功能重叠

---

## 🎯 理想架构设计

### 设计原则

1. **单一职责**：每个模块只做一件事
2. **显式依赖**：避免全局状态，使用依赖注入
3. **分层清晰**：Command → Service → Adapter → SDK
4. **统一执行路径**：所有执行都通过 SDK，终端仅作为展示层
5. **可测试性**：所有组件都可独立测试

### 推荐的架构重构

#### 方案 A：最小改动方案（推荐短期实施）

保持现有架构，仅修复明显问题：

```typescript
// 1. 明确 TaskExecutor 的职责：仅用于终端隔离
export class TaskExecutor {
  // 移除 SDK 依赖 ✅ 已完成
  
  async executeInTerminal(workflowId: string, input: any, terminal: TerminalSession) {
    // 保持 CLI 命令方式，但添加注释说明这是有意为之
    const command = `modular-agent execution run ${workflowId} --input '${JSON.stringify(input)}'`;
    terminal.pty.write(command + "\r");
    return { taskId, status: "started" };
  }
}

// 2. 重命名为更准确的名称
// TaskExecutor → TerminalWorkflowRunner
// 或保留原名但在文档中明确说明
```

**优点**：
- ✅ 改动最小
- ✅ 向后兼容
- ✅ 快速解决问题

**缺点**：
- ❌ 未解决双重初始化问题
- ❌ 架构不一致仍然存在

#### 方案 B：统一执行层方案（推荐中期实施）⭐

**核心思想**：所有工作流执行都通过 SDK，终端仅作为输出展示层。

```typescript
// 新的架构层次
┌─────────────────────────────────────────────┐
│         Command Layer (commands/)            │
│  - Parse user input                          │
│  - Delegate to ExecutionService              │
└─────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────┐
│      ExecutionService (NEW)                  │
│  - Unified execution interface               │
│  - Mode selection (blocking/detached/bg)     │
│  - Progress tracking                         │
└─────────────────────────────────────────────┘
                    ↓
        ┌───────────┴───────────┐
        ↓                       ↓
┌──────────────────┐  ┌──────────────────────┐
│ BlockingExecutor │  │ DetachedExecutor     │
│ (direct SDK)     │  │ (SDK + terminal UI)  │
└──────────────────┘  └──────────────────────┘
                              ↓
                    ┌──────────────────┐
                    │ SDK Layer        │
                    │ (single init)    │
                    └──────────────────┘
```

**具体实现**：

```typescript
// services/execution/execution-service.ts
export class ExecutionService {
  private sdk: SDKInstance;
  private terminalManager: TerminalManager;
  
  constructor(sdk: SDKInstance, terminalManager: TerminalManager) {
    this.sdk = sdk;
    this.terminalManager = terminalManager;
  }
  
  /**
   * Unified execution interface
   */
  async execute(
    workflowId: string,
    input: Record<string, unknown>,
    mode: 'blocking' | 'detached' | 'background'
  ): Promise<ExecutionResult> {
    switch (mode) {
      case 'blocking':
        return this.executeBlocking(workflowId, input);
      case 'detached':
        return this.executeDetached(workflowId, input);
      case 'background':
        return this.executeBackground(workflowId, input);
    }
  }
  
  /**
   * Blocking mode: Direct SDK call, wait for result
   */
  private async executeBlocking(
    workflowId: string,
    input: Record<string, unknown>
  ): Promise<ExecutionResult> {
    const adapter = new WorkflowExecutionAdapter(this.sdk);
    const execution = await adapter.executeWorkflow(workflowId, input);
    
    return {
      type: 'blocking',
      executionId: execution.id,
      status: execution.status,
      result: execution.result,
    };
  }
  
  /**
   * Detached mode: SDK execution + terminal display
   */
  private async executeDetached(
    workflowId: string,
    input: Record<string, unknown>
  ): Promise<ExecutionResult> {
    // 1. Start workflow via SDK (single initialization)
    const adapter = new WorkflowExecutionAdapter(this.sdk);
    const execution = await adapter.executeWorkflow(workflowId, input);
    
    // 2. Create terminal for display only
    const terminal = this.terminalManager.createTerminal({ background: false });
    
    // 3. Stream events to terminal
    this.streamEventsToTerminal(execution.id, terminal);
    
    return {
      type: 'detached',
      executionId: execution.id,
      terminalId: terminal.id,
      pid: terminal.pid,
    };
  }
  
  /**
   * Background mode: SDK execution + log file
   */
  private async executeBackground(
    workflowId: string,
    input: Record<string, unknown>
  ): Promise<ExecutionResult> {
    // 1. Start workflow via SDK
    const adapter = new WorkflowExecutionAdapter(this.sdk);
    const execution = await adapter.executeWorkflow(workflowId, input);
    
    // 2. Create background terminal for logging
    const terminal = this.terminalManager.createTerminal({ 
      background: true,
      logFile: `logs/workflow-${execution.id}.log`
    });
    
    // 3. Redirect output to log file
    this.redirectOutputToFile(execution.id, terminal);
    
    return {
      type: 'background',
      executionId: execution.id,
      logFile: terminal.logFile,
      pid: terminal.pid,
    };
  }
  
  /**
   * Stream SDK events to terminal
   */
  private streamEventsToTerminal(executionId: string, terminal: TerminalSession) {
    // Subscribe to SDK events
    this.sdk.events.on('workflow.progress', (event) => {
      if (event.executionId === executionId) {
        terminal.pty.write(`Progress: ${event.progress}%\n`);
      }
    });
    
    this.sdk.events.on('workflow.completed', (event) => {
      if (event.executionId === executionId) {
        terminal.pty.write(`Completed: ${JSON.stringify(event.result)}\n`);
      }
    });
  }
}
```

**命令层简化**：

```typescript
// commands/workflow-execution/index.ts
let executionService: ExecutionService;

function getExecutionService(): ExecutionService {
  if (!executionService) {
    const sdk = getSDKInstance();
    const terminalManager = new TerminalManager();
    executionService = new ExecutionService(sdk, terminalManager);
  }
  return executionService;
}

workflowExecutionCmd
  .command("run <workflow-id>")
  .option("-b, --blocking", "Run in blocking mode")
  .option("--background", "Run in background")
  .action(async (workflowId, options) => {
    const mode = options.blocking ? 'blocking' : 
                 options.background ? 'background' : 'detached';
    
    const service = getExecutionService();
    const result = await service.execute(workflowId, inputData, mode);
    
    // Format and display result based on mode
    output.output(formatExecutionResult(result));
  });
```

**优点**：
- ✅ **单一 SDK 初始化**：所有模式共享同一个 SDK 实例
- ✅ **统一执行路径**：所有执行都通过 SDK API
- ✅ **清晰的职责**：终端仅用于显示，不参与执行
- ✅ **更好的性能**：无进程 spawn 开销
- ✅ **状态同步**：父子上下文共享同一状态
- ✅ **易于测试**：ExecutionService 可独立测试

**缺点**：
- ⚠️ 需要较大的重构工作量
- ⚠️ 需要实现事件流转发机制
- ⚠️ 需要处理并发执行的隔离

#### 方案 C：完全解耦方案（长期愿景）

将 CLI 应用拆分为两个独立部分：

```
┌─────────────────────────────────────────────┐
│  CLI Frontend (commands + UI)               │
│  - User interaction                         │
│  - Command parsing                          │
│  - Result formatting                        │
│  - Terminal management                      │
└─────────────────────────────────────────────┘
                    ↓ (via IPC or API)
┌─────────────────────────────────────────────┐
│  Execution Backend (SDK wrapper service)    │
│  - Single SDK instance                      │
│  - Execution queue                          │
│  - Event distribution                       │
│  - State management                         │
└─────────────────────────────────────────────┘
```

**特点**：
- 前后端分离
- 支持远程执行
- 更好的可扩展性

**适用场景**：
- 需要分布式执行
- 需要 Web UI 支持
- 需要多用户协作

---

## 📋 具体改进建议

### 短期改进（1-2周）

#### 1. 完善文档和注释

```typescript
// task-executor.ts
/**
 * Task Executor - Terminal Isolation Layer
 * 
 * Purpose: Provides process-level isolation for long-running workflows.
 * 
 * Design Decision: Uses CLI commands instead of direct SDK calls because:
 * 1. Process isolation prevents crashes from affecting the main CLI
 * 2. Independent resource cleanup
 * 3. Better suited for background/daemon scenarios
 * 
 * For direct SDK integration, use WorkflowExecutionAdapter instead.
 */
```

#### 2. 添加架构决策记录（ADR）

创建 `docs/architecture/cli-app-architecture.md`，记录：
- 为什么选择当前的分层架构
- Terminal vs SDK 执行的权衡
- 未来演进方向

#### 3. 改进错误处理

```typescript
// 当前：简单的错误抛出
throw new Error("Write operation is not supported for background terminals");

// 改进：提供上下文和建议
throw new CLIError(
  "Cannot write to background terminal. Background terminals use log files for output.",
  "TERMINAL_OPERATION_NOT_SUPPORTED",
  {
    suggestion: "Use foreground terminal or check log file for output",
    terminalId: sessionId,
  }
);
```

#### 4. 添加集成测试

```typescript
// __tests__/integration/execution-modes.test.ts
describe("Execution Modes", () => {
  it("should execute in blocking mode with single SDK init", async () => {
    // Test that blocking mode uses direct SDK call
  });
  
  it("should execute in background mode with isolated process", async () => {
    // Test that background mode creates separate process
  });
  
  it("should share SDK instance across commands", async () => {
    // Test SDK singleton behavior
  });
});
```

### 中期改进（1-2个月）

#### 1. 实施方案 B：统一执行层

- 创建 `ExecutionService`
- 重构 `TaskExecutor` 为 `TerminalDisplayManager`
- 实现事件流转发机制
- 更新所有命令使用新的服务

#### 2. 引入依赖注入容器

```typescript
// services/container.ts
export class CLIDependencyContainer {
  private sdk: SDKInstance;
  private terminalManager: TerminalManager;
  private executionService: ExecutionService;
  
  constructor(sdk: SDKInstance) {
    this.sdk = sdk;
    this.terminalManager = new TerminalManager();
    this.executionService = new ExecutionService(sdk, this.terminalManager);
  }
  
  getExecutionService(): ExecutionService {
    return this.executionService;
  }
  
  getTerminalManager(): TerminalManager {
    return this.terminalManager;
  }
}

// index.ts
const container = new CLIDependencyContainer(sdkInstance);

// commands can access via container
const service = container.getExecutionService();
```

#### 3. 优化终端管理

- 实现终端池（Terminal Pool）复用
- 添加终端健康检查
- 支持终端会话恢复

### 长期改进（3-6个月）

#### 1. 考虑方案 C：前后端分离

如果需求增长到需要：
- Web UI 支持
- 远程执行能力
- 多租户隔离

则可以考虑将 SDK 包装为独立服务。

#### 2. 性能优化

- 实现命令执行缓存
- 优化 SDK 初始化时间
- 添加执行结果缓存层

#### 3. 监控和可观测性

- 添加执行指标收集
- 实现分布式追踪
- 添加性能分析工具

---

## 🎓 设计原则总结

### 什么是对的？

1. **分层架构**：Command → Service → Adapter → SDK ✅
2. **适配器模式**：封装 SDK 复杂性 ✅
3. **命令分组**：清晰的命令组织 ✅
4. **错误处理**：统一的错误转换和日志 ✅

### 什么是错的？

1. **隐式全局依赖**：`getSDKInstance()` ❌
2. **双重初始化**：后台模式启动新进程 ❌
3. **职责不清**：TaskExecutor 命名误导 ❌
4. **混合执行模式**：CLI 命令 vs SDK API ❌

### 正确的方向

1. **显式依赖注入**：通过构造函数传递依赖
2. **单一执行路径**：所有执行通过 SDK
3. **清晰的职责边界**：终端=展示，SDK=执行
4. **统一的服务层**：ExecutionService 协调所有模式

---

## 📝 结论

### 当前架构评分

| 维度 | 评分 | 说明 |
|------|------|------|
| 代码组织 | ⭐⭐⭐⭐ | 清晰的分层和模块化 |
| 可维护性 | ⭐⭐⭐ | 适配器模式好，但全局依赖有问题 |
| 可扩展性 | ⭐⭐⭐ | 命令易扩展，但执行模式混乱 |
| 性能 | ⭐⭐ | 后台模式有双重初始化开销 |
| 可测试性 | ⭐⭐ | 全局状态导致测试困难 |
| 一致性 | ⭐⭐ | 两种执行路径不一致 |

**总体评分：⭐⭐⭐ (3/5)**

### 建议行动

1. **立即**：完成 TaskExecutor 的文档化（✅ 已完成）
2. **本周**：添加 ADR 文档，解释架构决策
3. **本月**：实施 ExecutionService，统一执行路径
4. **本季度**：引入依赖注入，消除全局状态

### 最终目标

```
理想的 CLI 架构应该是：
- 薄命令层：仅解析输入和格式化输出
- 厚服务层：包含所有业务逻辑
- 统一执行：所有操作通过 SDK
- 显式依赖：无全局状态
- 易于测试：所有组件可独立测试
```

这样的架构既能保持 CLI 的简洁性，又能充分利用 SDK 的能力，同时为未来的扩展（如 Web UI、远程执行）打下坚实基础。
