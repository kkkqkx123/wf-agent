# CLI-App 架构设计与演进规划

## 📐 架构设计意图

### 核心设计原则

1. **单一执行路径**：所有工作流执行都通过 SDK，终端仅用于展示
2. **显式依赖管理**：避免全局状态，使用依赖注入（进行中）
3. **分层清晰**：Command → Service → Adapter → SDK
4. **终端原生控制**：充分利用 PTY 的实时交互能力（中断、信号处理等）
5. **架构一致性**：CLI 和 Web Backend 都直接使用 SDK，无中间层

### 为什么不需要独立后端服务？

**关键决策**：SDK 不应封装为独立的后端服务，原因如下：

#### 1. 终端控制的天然优势

```typescript
// CLI 可以直接响应终端信号
process.on('SIGINT', () => {
  executionService.stopExecution(executionId); // 立即中断
});

// PTY 可以发送控制字符
terminal.pty.write('\x03'); // Ctrl+C 中断工作流
```

如果封装为后端服务：
- ❌ 需要 WebSocket/SSE 传递控制指令
- ❌ 增加延迟和复杂性
- ❌ 需要处理连接断开等边界情况
- ❌ 失去终端原生交互能力

#### 2. 架构简洁性

**当前架构（推荐）**：
```
┌──────────┐         ┌──────────┐
│ CLI App  │──────→  │          │
└──────────┘         │   SDK    │ ← 单一抽象层
┌──────────┐         │          │
│ Web Back │──────→  │          │
└──────────┘         └──────────┘
```

**错误架构（不推荐）**：
```
┌──────────┐         ┌──────────┐         ┌──────────┐
│ CLI App  │──────→  │Backend Svc│──────→  │   SDK    │
└──────────┘         └──────────┘         └──────────┘
```

额外封装层的弊端：
- 违反 KISS 原则
- 引入网络延迟和故障点
- 增加维护成本
- 违背"薄封装"原则

#### 3. Web Backend 的正确做法

Web Backend 应该**直接使用 SDK**，而非通过中间服务：

```typescript
// apps/web-app-backend/src/controllers/workflow.controller.ts
import { createSDK, ExecuteWorkflowCommand } from '@wf-agent/sdk';

export class WorkflowController {
  private sdk: SDKInstance;
  
  constructor() {
    this.sdk = createSDK({ /* config */ });
  }
  
  async execute(req, res) {
    const result = await this.sdk.executeCommand(
      new ExecuteWorkflowCommand({ workflowId, options: { input } })
    );
    res.json(result);
  }
  
  // WebSocket for real-time updates
  handleWebSocket(ws, executionId) {
    this.sdk.events.on('workflow.progress', (event) => {
      if (event.executionId === executionId) {
        ws.send(JSON.stringify(event));
      }
    });
  }
}
```

**优势**：
- ✅ 与 CLI 保持一致的架构
- ✅ 直接利用 SDK 的事件系统
- ✅ 无额外性能开销
- ✅ 易于维护和测试

---

## 🏗️ 当前架构实现

### 已完成的改进

#### 1. ExecutionService 统一执行层 ✅

**文件**: [execution-service.ts](file://d:/项目/agent/wf-agent/apps/cli-app/src/services/execution/execution-service.ts)

**实现内容**:
```typescript
export class ExecutionService {
  async execute(
    workflowId: string,
    input: Record<string, unknown>,
    mode: ExecutionMode // 'blocking' | 'detached' | 'background'
  ): Promise<ExecutionResult>
}
```

**设计意图**:
- ✅ 所有工作流执行都通过 SDK（单一执行路径）
- ✅ 终端仅用于展示，不参与执行逻辑
- ✅ 三种模式共享同一个 SDK 实例
- ✅ 消除双重初始化问题

**使用示例**:
```typescript
// commands/workflow-execution/index.ts
const service = getExecutionService();
const result = await service.execute(workflowId, inputData, mode);
```

#### 2. TaskExecutor 标记为废弃 ✅

**文件**: [task-executor.ts](file://d:/项目/agent/wf-agent/apps/cli-app/src/services/terminal/task-executor.ts)

**改进内容**:
- 添加 `@deprecated` 注释
- 明确说明双重初始化问题
- 推荐使用 ExecutionService

**保留原因**:
- 向后兼容旧代码
- 特殊场景需要完全进程隔离时使用

---

## 🔧 待完成的改进

### 高优先级（1-2周）

#### 1. 实现事件流转发机制

**现状**: `setupEventStreaming()` 和 `setupBackgroundLogging()` 是占位符

**需要完成**:
```typescript
// services/execution/execution-service.ts
private async setupEventStreaming(
  executionId: string,
  terminal: TerminalSession
): Promise<void> {
  // TODO: 等待 SDK 提供事件订阅 API
  const subscription = await this.sdk.subscribeToEvents({
    executionId,
    onProgress: (event) => {
      terminal.pty.write(`Progress: ${event.progress}%\n`);
    },
    onComplete: (event) => {
      terminal.pty.write(`Completed: ${JSON.stringify(event.result)}\n`);
    },
    onError: (event) => {
      terminal.pty.write(`Error: ${event.error}\n`);
    }
  });
}
```

**依赖**: SDK 需要提供事件订阅 API

**影响范围**:
- detached 模式的实时进度显示
- background 模式的日志记录

#### 2. 添加集成测试

**需要编写的测试**:
```typescript
// __tests__/integration/execution-service.test.ts
describe('ExecutionService', () => {
  it('should use single SDK instance across all modes', async () => {
    // 验证 blocking/detached/background 都使用同一个 SDK
    const sdkSpy = vi.spyOn(SDKFactory, 'create');
    
    await service.execute(id1, input, 'blocking');
    await service.execute(id2, input, 'detached');
    await service.execute(id3, input, 'background');
    
    expect(sdkSpy).toHaveBeenCalledTimes(1); // 只初始化一次
  });
  
  it('should handle interruption via terminal signals', async () => {
    // 验证 SIGINT 能正确触发 stopExecution
    const stopSpy = vi.spyOn(adapter, 'stopWorkflowExecution');
    
    process.emit('SIGINT');
    
    expect(stopSpy).toHaveBeenCalledWith(executionId);
  });
  
  it('should stream events to terminal in detached mode', async () => {
    // 验证实时进度显示
    const writeSpy = vi.spyOn(terminal.pty, 'write');
    
    await service.execute(id, input, 'detached');
    sdk.events.emit('workflow.progress', { executionId: id, progress: 50 });
    
    expect(writeSpy).toHaveBeenCalledWith(expect.stringContaining('Progress: 50%'));
  });
});
```

**测试覆盖目标**:
- 单一 SDK 初始化验证
- 中断信号处理
- 事件流转发
- 不同执行模式的正确性

#### 3. 完善错误处理和日志

**当前问题**: 错误信息不够详细

**需要改进**:
```typescript
// 当前
throw new Error("Failed to stop execution");

// 改进后
throw new CLIError(
  "Failed to stop workflow execution",
  "EXECUTION_STOP_FAILED",
  {
    executionId,
    suggestion: "Check if the execution is still running",
    retryable: true
  }
);
```

### 中优先级（1-2个月）

#### 4. 引入依赖注入容器

**现状**: 仍使用 `getSDKInstance()` 全局访问

**需要实现**:
```typescript
// services/container.ts
export class CLIDependencyContainer {
  private sdk: SDKInstance;
  private terminalManager: TerminalManager;
  private executionService: ExecutionService;
  
  constructor(config: CLIConfig) {
    this.sdk = createSDK(config.sdk);
    this.terminalManager = new TerminalManager();
    this.executionService = new ExecutionService(this.sdk, this.terminalManager);
  }
  
  getExecutionService(): ExecutionService {
    return this.executionService;
  }
  
  getTerminalManager(): TerminalManager {
    return this.terminalManager;
  }
}

// index.ts
const container = new CLIDependencyContainer(config);
program.hook('preAction', () => {
  // 注入到命令上下文
  program.setOptionValue('container', container);
});

// commands/workflow-execution/index.ts
.action(async function(workflowId, options) {
  const container = this.opts().container;
  const service = container.getExecutionService();
  // ...
});
```

**优势**:
- ✅ 消除全局状态
- ✅ 易于测试（可以 mock container）
- ✅ 支持多实例场景
- ✅ 明确的依赖关系

#### 5. 优化终端管理

**需要改进**:
- 实现终端池复用（避免频繁创建/销毁 PTY）
- 添加终端健康检查
- 支持终端会话恢复（断线重连）

```typescript
// services/terminal/terminal-pool.ts
export class TerminalPool {
  private pool: Map<string, TerminalSession> = new Map();
  
  acquire(options: TerminalOptions): TerminalSession {
    // 从池中获取或创建新终端
  }
  
  release(sessionId: string): void {
    // 回收到池中而非销毁
  }
  
  healthCheck(): void {
    // 定期检查终端健康状态
  }
}
```

### 低优先级（3-6个月）

#### 6. 性能优化

**潜在优化点**:
- SDK 初始化缓存（避免重复加载配置）
- 命令执行结果缓存
- 终端渲染优化（节流更新）

#### 7. 可观测性增强

**需要添加**:
- 执行指标收集（执行时间、成功率等）
- 分布式追踪（跨服务调用链路）
- 性能分析工具（profiling）

```typescript
// services/metrics/execution-metrics.ts
export class ExecutionMetrics {
  recordExecutionStart(executionId: string, mode: ExecutionMode): void;
  recordExecutionComplete(executionId: string, duration: number): void;
  recordExecutionError(executionId: string, error: Error): void;
  
  getMetrics(): {
    totalExecutions: number;
    averageDuration: number;
    successRate: number;
    modeDistribution: Record<ExecutionMode, number>;
  };
}
```

---

## 📊 架构评分

| 维度 | Phase 1 前 | Phase 2 后 | Phase 3 后 | 目标评分 | 说明 |
|------|-----------|-----------|-----------|---------|------|
| 代码组织 | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | 分层清晰，DI 容器完善 |
| 可维护性 | ⭐⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | 消除全局状态，DI 完善 |
| 可扩展性 | ⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | 事件系统和 DI 完善 |
| 性能 | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | 单一初始化，高效 |
| 可测试性 | ⭐⭐ | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | DI 使测试更容易 |
| 一致性 | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ | 统一架构 |

**总体评分**: ⭐⭐⭐⭐ (4/5) → ⭐⭐⭐⭐⭐ (4.5/5) → ⭐⭐⭐⭐⭐ (4.8/5) → 目标 ⭐⭐⭐⭐⭐ (5/5)

---

## 🎯 实施路线图

### Phase 1: 基础完善（本周）
- [ ] 添加 ADR 文档解释架构决策
- [ ] 完善 ExecutionService 的文档和注释
- [ ] 添加基本的集成测试框架

### Phase 2: 核心功能（1-2周） ✅ COMPLETED
- [x] 实现事件流转发机制（依赖 SDK API）
  - 完成 `setupEventStreaming()` 实现，订阅 NODE_COMPLETED、WORKFLOW_EXECUTION_COMPLETED、WORKFLOW_EXECUTION_FAILED 事件
  - 完成 `setupBackgroundLogging()` 实现，订阅 NODE_STARTED、NODE_COMPLETED、WORKFLOW_EXECUTION_COMPLETED、WORKFLOW_EXECUTION_FAILED 事件
  - 导出 SDK 的 `createExecutionScopedSubscription` 函数供 CLI 使用
- [x] 添加完整的集成测试
  - 创建 `__tests__/integration/execution-service.test.ts`
  - 覆盖单一 SDK 实例验证、执行模式错误处理、事件流设置、监控和控制功能
- [x] 改进错误处理和日志
  - 使用 `CLIError` 替换通用 `Error`
  - 为 `monitorExecution()` 和 `stopExecution()` 添加结构化错误处理
  - 统一错误代码（VALIDATION、API）

### Phase 3: 架构优化（1-2个月） ✅ COMPLETED
- [x] 引入依赖注入容器
  - 创建 `CLIDependencyContainer` 类管理所有服务
  - 提供 `initializeContainer()`、`getContainer()`、`clearContainer()` 函数
  - 在 `index.ts` 的 preAction hook 中初始化容器
  - 更新 shutdown 和 TUI cleanup 使用容器清理资源
- [x] 迁移命令使用容器
  - 更新 `workflow-execution/index.ts` 从容器获取服务
  - 消除全局状态 `executionService` 和 `terminalManager`
  - 通过 `getContainer().getExecutionService()` 等获取依赖
- [ ] 优化终端管理（终端池）- 移至 Phase 4

### Phase 4: 高级特性（3-6个月）
- [ ] 性能优化（缓存、节流）
- [ ] 可观测性增强（指标、追踪）
- [ ] 终端会话恢复功能

---

## 💡 关键设计决策总结

### ✅ 正确的决策

1. **ExecutionService 统一执行层**
   - 所有执行通过 SDK
   - 终端仅用于展示
   - 消除双重初始化

2. **CLI 直接使用 SDK（不封装后端）**
   - 保持终端原生控制能力
   - 架构简洁
   - 无额外延迟

3. **Web Backend 也直接使用 SDK**
   - 与 CLI 保持一致
   - 通过 HTTP/WebSocket 暴露 API
   - 不需要中间层

### ❌ 避免的错误

1. **不要将 SDK 封装为独立后端服务**
   - 失去终端控制能力
   - 增加复杂度
   - 引入网络延迟

2. **不要混合执行路径**
   - 避免部分用 SDK，部分用 CLI 命令
   - 保持单一执行路径

3. **不要过度依赖全局状态**
   - 逐步迁移到依赖注入
   - 提高可测试性

---

## 📝 结论

### 当前状态

✅ **Phase 1, 2 & 3 已完成**：
- ExecutionService 统一执行层已实现
- 单一 SDK 初始化已达成
- TaskExecutor 已标记废弃
- **事件流转发机制已实现**（detached 和 background 模式）
- **集成测试覆盖已添加**（execution-service.test.ts）
- **错误处理已改进**（使用 CLIError 替换 Error）
- **依赖注入容器已实现**（CLIDependencyContainer）
- **命令已迁移到容器**（workflow-execution 命令组）

⚠️ **需要完善**：
- 终端池优化（Phase 4）
- 性能优化和可观测性（Phase 4）

### 最终目标

```
理想的 CLI 架构：
┌──────────────┐
│ Command Layer │ ← 薄层：仅解析输入和格式化输出
└───────┬──────┘
        ↓
┌──────────────┐
│ Service Layer │ ← 厚层：包含所有业务逻辑
│ (Execution)  │
└───────┬──────┘
        ↓
┌──────────────┐
│ Adapter Layer │ ← 封装 SDK 复杂性
└───────┬──────┘
        ↓
┌──────────────┐
│   SDK Layer   │ ← 核心执行引擎
└──────────────┘

特点：
- 显式依赖注入（无全局状态）
- 统一执行路径（所有操作通过 SDK）
- 清晰的职责边界（终端=展示，SDK=执行）
- 易于测试（所有组件可独立测试）
```

这样的架构既能保持 CLI 的简洁性和终端控制能力，又能充分利用 SDK 的功能，同时为未来的扩展（如 Web UI、远程执行）打下坚实基础。
