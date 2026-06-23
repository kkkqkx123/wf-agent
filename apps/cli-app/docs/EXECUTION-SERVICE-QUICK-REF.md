# ExecutionService 快速参考

## 🚀 快速开始

### 基本用法

```typescript
import { ExecutionService } from "../services/execution/execution-service.js";
import { TerminalManager } from "../services/terminal/terminal-manager.js";
import { getSDKInstance } from "../index.js";

// 初始化
const sdk = getSDKInstance();
const terminalManager = new TerminalManager();
const executionService = new ExecutionService(sdk, terminalManager);

// 执行工作流
const result = await executionService.execute(
  "my-workflow-id",
  { param1: "value1", param2: "value2" },
  "detached" // 或 'blocking' | 'background'
);

console.log("Execution ID:", result.executionId);
console.log("Mode:", result.mode);
```

## 📋 API 参考

### ExecutionService

#### `execute(workflowId, input, mode)`

统一的工作流执行接口。

**参数**:
- `workflowId` (string): 工作流 ID
- `input` (Record<string, unknown>): 输入数据
- `mode` (ExecutionMode): 执行模式
  - `'blocking'`: 阻塞模式，同步等待结果
  - `'detached'`: 分离模式，前台终端显示（默认）
  - `'background'`: 后台模式，日志文件记录

**返回**: `Promise<ExecutionResult>`

**示例**:
```typescript
// Blocking mode
const result = await executionService.execute("wf-123", {}, "blocking");
console.log("Result:", result.result);

// Detached mode
const result = await executionService.execute("wf-123", {}, "detached");
console.log("Terminal:", result.terminalId);

// Background mode
const result = await executionService.execute("wf-123", {}, "background");
console.log("Log file:", result.logFile);
```

#### `monitorExecution(executionId)`

监控执行状态。

**参数**:
- `executionId` (string): 执行 ID

**返回**: `Promise<{ executionId, status, progress?, lastUpdate }>`

**示例**:
```typescript
const status = await executionService.monitorExecution("abc-123-def");
console.log("Status:", status.status);
console.log("Progress:", status.progress);
```

#### `stopExecution(executionId)`

停止执行。

**参数**:
- `executionId` (string): 执行 ID

**返回**: `Promise<void>`

**示例**:
```typescript
await executionService.stopExecution("abc-123-def");
console.log("Execution stopped");
```

#### `cleanup()`

清理资源。

**返回**: `Promise<void>`

**示例**:
```typescript
await executionService.cleanup();
```

## 🎯 执行模式对比

| 特性 | Blocking | Detached | Background |
|------|----------|----------|------------|
| **执行方式** | SDK 直接调用 | SDK + 终端显示 | SDK + 日志文件 |
| **同步/异步** | 同步 | 异步 | 异步 |
| **终端创建** | ❌ 无 | ✅ 前台 PTY | ✅ 后台进程 |
| **实时输出** | ❌ 最终结果 | ✅ 终端显示 | ✅ 日志文件 |
| **用户交互** | ❌ 等待完成 | ✅ 可交互 | ❌ 无交互 |
| **适用场景** | 脚本自动化 | 交互式使用 | 长时间任务 |
| **返回数据** | 完整结果 | 执行元数据 | 执行元数据 |

## 💡 最佳实践

### 1. 选择合适的模式

```typescript
// 脚本自动化 → blocking
if (isAutomated) {
  const result = await executionService.execute(wfId, input, "blocking");
  processResult(result.result);
}

// 交互式 CLI → detached（默认）
else if (isInteractive) {
  const result = await executionService.execute(wfId, input, "detached");
  console.log(`Started in terminal: ${result.terminalId}`);
}

// 后台任务 → background
else {
  const result = await executionService.execute(wfId, input, "background");
  console.log(`Running in background, log: ${result.logFile}`);
}
```

### 2. 错误处理

```typescript
try {
  const result = await executionService.execute(wfId, input, mode);
  console.log("Success:", result.executionId);
} catch (error) {
  console.error("Execution failed:", error.message);
  // 处理错误...
}
```

### 3. 监控执行

```typescript
// 定期检查状态
const checkInterval = setInterval(async () => {
  const status = await executionService.monitorExecution(executionId);
  console.log(`Progress: ${status.status}`);
  
  if (status.status === "completed" || status.status === "failed") {
    clearInterval(checkInterval);
  }
}, 5000);
```

### 4. 资源清理

```typescript
// 应用退出时清理
process.on("exit", async () => {
  await executionService.cleanup();
});
```

## 🔧 命令行对应关系

| ExecutionService | CLI Command |
|------------------|-------------|
| `execute(wfId, input, 'blocking')` | `modular-agent execution run <wfId> --blocking` |
| `execute(wfId, input, 'detached')` | `modular-agent execution run <wfId>` |
| `execute(wfId, input, 'background')` | `modular-agent execution run <wfId> --background` |
| `monitorExecution(execId)` | `modular-agent execution status <execId>` |
| `stopExecution(execId)` | `modular-agent execution cancel <execId>` |

## ⚠️ 注意事项

### 1. SDK 实例必须已初始化

```typescript
// ❌ 错误：SDK 未初始化
const service = new ExecutionService(null, terminalManager);

// ✅ 正确：确保 SDK 已初始化
const sdk = getSDKInstance();
if (!sdk) {
  throw new Error("SDK not initialized");
}
const service = new ExecutionService(sdk, terminalManager);
```

### 2. ExecutionResult 字段可能为空

```typescript
const result = await executionService.execute(wfId, input, "detached");

// ✅ 安全检查
if (result.terminalId) {
  console.log("Terminal:", result.terminalId);
}

if (result.result) {
  console.log("Result:", result.result);
}
```

### 3. 事件流转发尚未实现

当前版本中，终端仅显示初始信息，不显示实时进度。

```typescript
// TODO: Future implementation
// Terminal will show real-time progress when SDK event subscription is available
```

## 📖 迁移指南

### 从 TaskExecutor 迁移

**之前**：
```typescript
import { TaskExecutor } from "../services/terminal/task-executor.js";
import { TerminalManager } from "../services/terminal/terminal-manager.js";

const terminalManager = new TerminalManager();
const taskExecutor = new TaskExecutor();

const terminal = terminalManager.createTerminal({ background: false });
const result = await taskExecutor.executeInTerminal(wfId, input, terminal);
```

**现在**：
```typescript
import { ExecutionService } from "../services/execution/execution-service.js";
import { TerminalManager } from "../services/terminal/terminal-manager.js";

const terminalManager = new TerminalManager();
const executionService = new ExecutionService(sdk, terminalManager);

const result = await executionService.execute(wfId, input, "detached");
```

**优势**：
- ✅ 代码更简洁（3行 vs 5行）
- ✅ 单一 SDK 初始化
- ✅ 自动管理终端
- ✅ 类型更安全

## 🐛 常见问题

### Q: 为什么我的终端没有显示实时进度？

A: 当前版本的事件流转发功能尚未实现。终端仅显示初始信息。未来版本将支持实时进度显示。

### Q: 如何获取执行的最终结果？

A: 使用 `blocking` 模式：
```typescript
const result = await executionService.execute(wfId, input, "blocking");
console.log("Final result:", result.result);
```

### Q: 可以同时运行多个工作流吗？

A: 可以。每个执行都有独立的 executionId：
```typescript
const result1 = await executionService.execute("wf-1", {}, "detached");
const result2 = await executionService.execute("wf-2", {}, "detached");
// 两个工作流并行运行
```

### Q: 如何取消正在运行的工作流？

A: 使用 `stopExecution`：
```typescript
await executionService.stopExecution(executionId);
```

## 📚 更多信息

- [完整架构分析](./cli-app-architecture-analysis.md)
- [重构总结](./execution-service-refactoring-summary.md)
- [TaskExecutor 废弃说明](./task-executor-refactoring.md)

---

**最后更新**: 2026-05-16  
**版本**: 1.0.0
