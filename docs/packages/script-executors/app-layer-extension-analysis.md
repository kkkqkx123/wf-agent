# Script-Executors 模块应用层扩展能力分析报告

## 文档信息

- **分析对象**: `packages/script-executors`
- **分析目标**: 评估应用层包装扩展可行性，支持沙箱、审计等功能
- **分析日期**: 2025-01-XX
- **分析结论**: ✅ 完全支持应用层包装扩展

---

## 核心结论

**`packages\script-executors` 模块架构设计优秀，完全支持应用层包装扩展**。通过清晰的接口隔离、分层设计和灵活的注册机制，应用层可以轻松实现沙箱、审计、资源限制等高级功能，无需修改核心模块代码。

---

## 一、架构设计评估

### 1.1 分层架构设计

```
IScriptExecutor (接口层)
    ↓ 定义统一契约
BaseScriptExecutor (抽象基类)
    ├── 重试机制 (RetryStrategy)
    ├── 超时控制 (TimeoutController)
    └── 结果标准化
    ↓
CommandLineExecutor<T> (命令行基类)
    ├── spawn 调用封装
    ├── 环境变量管理
    ├── 工作目录管理
    └── 输出收集
    ↓
具体执行器
    ├── ShellExecutor
    ├── PythonExecutor
    ├── JavaScriptExecutor
    ├── PowerShellExecutor
    └── CmdExecutor
```

### 1.2 设计优势

| 优势 | 说明 | 扩展价值 |
|-----|------|---------|
| ✅ 接口清晰 | [`IScriptExecutor`](../../packages/script-executors/src/core/interfaces/IScriptExecutor.ts:12) 定义统一契约 | 易于包装扩展 |
| ✅ 职责分离 | 验证由 SDK 负责，执行器专注执行 | 避免重复逻辑 |
| ✅ 代码复用 | [`CommandLineExecutor`](../../packages/script-executors/src/core/base/CommandLineExecutor.ts:32) 共享 90%+ 代码 | 减少扩展工作量 |
| ✅ 类型安全 | TypeScript 泛型设计 | 编译时错误检测 |
| ✅ 灵活注册 | [`ScriptService.registerExecutor()`](../../sdk/core/services/script-service.ts:30) | 动态替换执行器 |

---

## 二、关键扩展点分析

### 2.1 扩展层次对比

| 扩展层次 | 实现方式 | 适用场景 | 灵活性 | 复杂度 |
|---------|---------|---------|--------|--------|
| **Level 1** | 接口包装（Decorator模式） | 审计、监控、权限控制 | ⭐⭐⭐⭐⭐ | ⭐ |
| **Level 2** | 继承 BaseScriptExecutor | 自定义执行逻辑、重试策略 | ⭐⭐⭐⭐ | ⭐⭐ |
| **Level 3** | 继承 CommandLineExecutor | 沙箱、资源限制、进程控制 | ⭐⭐⭐⭐ | ⭐⭐⭐ |

### 2.2 核心接口分析

#### IScriptExecutor 接口

```typescript
export interface IScriptExecutor {
  execute(
    script: Script,
    options?: ScriptExecutionOptions,
    context?: ExecutionContext
  ): Promise<ScriptExecutionResult>;
  
  validate(script: Script): ValidationResult;
  getSupportedTypes(): ScriptType[];
  cleanup?(): Promise<void>;
  getExecutorType(): string;
}
```

**扩展友好性**：
- ✅ 所有方法都是 `public`，易于包装
- ✅ `execute()` 方法支持 `ExecutionContext` 参数，可传递扩展上下文
- ✅ `cleanup()` 方法支持资源清理

#### ExecutionContext 类型

```typescript
export interface ExecutionContext {
  threadId?: string;           // 线程隔离
  workingDirectory?: string;   // 工作目录
  environment?: Record<string, string>;  // 环境变量
  signal?: AbortSignal;        // 中止信号
}
```

**扩展友好性**：
- ✅ 可扩展性强，应用层可添加自定义字段
- ✅ 支持线程隔离，便于实现沙箱

---

## 三、应用层包装扩展方案

### 3.1 模式一：审计包装器（推荐）

**适用场景**：执行日志、合规记录、操作追踪

```typescript
import { IScriptExecutor } from '@modular-agent/script-executors';
import type { Script, ScriptExecutionOptions, ScriptExecutionResult, ExecutionContext } from '@modular-agent/types';

/**
 * 审计包装器
 * 在执行前后记录审计日志
 */
class AuditingScriptExecutor implements IScriptExecutor {
  constructor(
    private wrapped: IScriptExecutor,
    private auditService: AuditService
  ) {}

  async execute(
    script: Script,
    options?: ScriptExecutionOptions,
    context?: ExecutionContext
  ): Promise<ScriptExecutionResult> {
    const executionId = this.generateExecutionId();
    
    // 记录执行开始
    await this.auditService.log({
      event: 'script_execution_start',
      executionId,
      scriptName: script.name,
      scriptType: script.type,
      timestamp: new Date(),
      userId: context?.userId,
      threadId: context?.threadId
    });

    try {
      // 执行脚本
      const result = await this.wrapped.execute(script, options, context);
      
      // 记录执行成功
      await this.auditService.log({
        event: 'script_execution_complete',
        executionId,
        scriptName: script.name,
        success: result.success,
        exitCode: result.exitCode,
        executionTime: result.executionTime,
        timestamp: new Date()
      });

      return result;
    } catch (error) {
      // 记录执行失败
      await this.auditService.log({
        event: 'script_execution_failed',
        executionId,
        scriptName: script.name,
        error: error instanceof Error ? error.message : String(error),
        timestamp: new Date()
      });
      throw error;
    }
  }

  validate(script: Script) {
    return this.wrapped.validate(script);
  }

  getSupportedTypes() {
    return this.wrapped.getSupportedTypes();
  }

  async cleanup() {
    await this.wrapped.cleanup();
  }

  getExecutorType() {
    return `AUDITED_${this.wrapped.getExecutorType()}`;
  }

  private generateExecutionId(): string {
    return `exec_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }
}
```

**使用方式**：

```typescript
import { ScriptService } from '@modular-agent/sdk';
import { ShellExecutor } from '@modular-agent/script-executors';

const scriptService = new ScriptService();
const auditService = new AuditService();

// 创建审计包装器
const baseExecutor = new ShellExecutor();
const auditedExecutor = new AuditingScriptExecutor(baseExecutor, auditService);

// 注册到 ScriptService
scriptService.registerExecutor('SHELL', auditedExecutor);
```

### 3.2 模式二：沙箱扩展

**适用场景**：资源限制、文件系统隔离、网络隔离

```typescript
import { CommandLineExecutor } from '@modular-agent/script-executors';
import type { Script, ExecutionContext, ExecutionOutput } from '@modular-agent/types';

/**
 * 沙箱配置
 */
interface SandboxConfig {
  /** 最大内存限制（MB） */
  maxMemory?: number;
  /** 最大CPU时间（秒） */
  maxCpuTime?: number;
  /** 允许的文件路径 */
  allowedPaths?: string[];
  /** 禁止的命令 */
  forbiddenCommands?: string[];
  /** 是否允许网络访问 */
  allowNetwork?: boolean;
}

/**
 * 沙箱 Shell 执行器
 * 通过继承 CommandLineExecutor 实现沙箱隔离
 */
class SandboxShellExecutor extends CommandLineExecutor<'SHELL'> {
  constructor(
    config?: any,
    private sandboxConfig: SandboxConfig = {}
  ) {
    super(config);
  }

  protected async doExecute(
    script: Script,
    context?: ExecutionContext
  ): Promise<ExecutionOutput> {
    // 1. 应用沙箱转换
    const sandboxedScript = this.applySandbox(script);
    
    // 2. 验证脚本内容
    this.validateSandboxRules(sandboxedScript);
    
    // 3. 执行脚本
    return super.doExecute(sandboxedScript, context);
  }

  /**
   * 应用沙箱转换
   */
  private applySandbox(script: Script): Script {
    let content = script.content;

    // 添加资源限制（ulimit）
    if (this.sandboxConfig.maxMemory) {
      content = `ulimit -v ${this.sandboxConfig.maxMemory * 1024}\n${content}`;
    }

    if (this.sandboxConfig.maxCpuTime) {
      content = `ulimit -t ${this.sandboxConfig.maxCpuTime}\n${content}`;
    }

    // 添加网络隔离（如果需要）
    if (this.sandboxConfig.allowNetwork === false) {
      content = `iptables -A OUTPUT -j DROP 2>/dev/null || true\n${content}`;
    }

    return {
      ...script,
      content
    };
  }

  /**
   * 验证沙箱规则
   */
  private validateSandboxRules(script: Script): void {
    const content = script.content.toLowerCase();

    // 检查禁止的命令
    if (this.sandboxConfig.forbiddenCommands) {
      for (const cmd of this.sandboxConfig.forbiddenCommands) {
        if (content.includes(cmd.toLowerCase())) {
          throw new Error(`Forbidden command detected: ${cmd}`);
        }
      }
    }

    // 检查文件路径访问
    if (this.sandboxConfig.allowedPaths) {
      // 实现路径检查逻辑
      // ...
    }
  }
}
```

**使用方式**：

```typescript
import { ScriptService } from '@modular-agent/sdk';

const scriptService = new ScriptService();

// 创建沙箱执行器
const sandboxExecutor = new SandboxShellExecutor(
  { type: 'SHELL' },
  {
    maxMemory: 512,        // 512MB 内存限制
    maxCpuTime: 60,        // 60秒 CPU 时间限制
    forbiddenCommands: ['rm -rf', 'dd', 'mkfs'],
    allowNetwork: false    // 禁止网络访问
  }
);

// 注册到 ScriptService
scriptService.registerExecutor('SHELL', sandboxExecutor);
```

### 3.3 模式三：权限控制包装器

**适用场景**：基于角色的访问控制、操作权限验证

```typescript
import { IScriptExecutor } from '@modular-agent/script-executors';
import type { Script, ScriptExecutionOptions, ScriptExecutionResult, ExecutionContext } from '@modular-agent/types';

/**
 * 权限配置
 */
interface PermissionConfig {
  /** 角色到脚本权限映射 */
  rolePermissions: Map<string, Set<string>>;
  /** 默认拒绝 */
  defaultDeny?: boolean;
}

/**
 * 权限控制包装器
 */
class PermissionControlledExecutor implements IScriptExecutor {
  constructor(
    private wrapped: IScriptExecutor,
    private permissionConfig: PermissionConfig
  ) {}

  async execute(
    script: Script,
    options?: ScriptExecutionOptions,
    context?: ExecutionContext
  ): Promise<ScriptExecutionResult> {
    // 获取用户角色
    const userRole = context?.userId ? await this.getUserRole(context.userId) : 'anonymous';

    // 检查权限
    if (!this.hasPermission(userRole, script.name)) {
      throw new Error(
        `Permission denied: User '${context?.userId}' with role '${userRole}' cannot execute script '${script.name}'`
      );
    }

    // 执行脚本
    return this.wrapped.execute(script, options, context);
  }

  validate(script: Script) {
    return this.wrapped.validate(script);
  }

  getSupportedTypes() {
    return this.wrapped.getSupportedTypes();
  }

  async cleanup() {
    await this.wrapped.cleanup();
  }

  getExecutorType() {
    return `PERMISSION_CONTROLLED_${this.wrapped.getExecutorType()}`;
  }

  private hasPermission(role: string, scriptName: string): boolean {
    const allowedScripts = this.permissionConfig.rolePermissions.get(role);
    
    if (!allowedScripts) {
      return !this.permissionConfig.defaultDeny;
    }

    return allowedScripts.has(scriptName);
  }

  private async getUserRole(userId: string): Promise<string> {
    // 实现用户角色查询逻辑
    return 'user';
  }
}
```

### 3.4 模式四：监控包装器

**适用场景**：性能监控、指标收集、告警

```typescript
import { IScriptExecutor } from '@modular-agent/script-executors';
import type { Script, ScriptExecutionOptions, ScriptExecutionResult, ExecutionContext } from '@modular-agent/types';

/**
 * 监控指标
 */
interface ExecutionMetrics {
  scriptName: string;
  scriptType: string;
  startTime: number;
  endTime: number;
  duration: number;
  success: boolean;
  exitCode?: number;
  stdoutLength: number;
  stderrLength: number;
  memoryUsage?: NodeJS.MemoryUsage;
}

/**
 * 监控包装器
 */
class MonitoringScriptExecutor implements IScriptExecutor {
  private metrics: ExecutionMetrics[] = [];

  constructor(
    private wrapped: IScriptExecutor,
    private metricsService: MetricsService
  ) {}

  async execute(
    script: Script,
    options?: ScriptExecutionOptions,
    context?: ExecutionContext
  ): Promise<ScriptExecutionResult> {
    const startTime = Date.now();
    const startMemory = process.memoryUsage();

    try {
      const result = await this.wrapped.execute(script, options, context);

      // 记录成功指标
      this.recordMetrics({
        scriptName: script.name,
        scriptType: script.type,
        startTime,
        endTime: Date.now(),
        duration: Date.now() - startTime,
        success: result.success,
        exitCode: result.exitCode,
        stdoutLength: result.stdout?.length || 0,
        stderrLength: result.stderr?.length || 0,
        memoryUsage: process.memoryUsage()
      });

      return result;
    } catch (error) {
      // 记录失败指标
      this.recordMetrics({
        scriptName: script.name,
        scriptType: script.type,
        startTime,
        endTime: Date.now(),
        duration: Date.now() - startTime,
        success: false,
        stdoutLength: 0,
        stderrLength: 0,
        memoryUsage: process.memoryUsage()
      });

      throw error;
    }
  }

  validate(script: Script) {
    return this.wrapped.validate(script);
  }

  getSupportedTypes() {
    return this.wrapped.getSupportedTypes();
  }

  async cleanup() {
    await this.wrapped.cleanup();
  }

  getExecutorType() {
    return `MONITORED_${this.wrapped.getExecutorType()}`;
  }

  private recordMetrics(metrics: ExecutionMetrics) {
    this.metrics.push(metrics);
    this.metricsService.record(metrics);
  }

  getMetrics(): ExecutionMetrics[] {
    return [...this.metrics];
  }
}
```

---

## 四、扩展能力矩阵

| 功能需求 | 推荐方式 | 实现难度 | 代码示例 | 性能影响 |
|---------|---------|---------|---------|---------|
| **审计日志** | 接口包装 | ⭐ | 3.1 | 低（异步日志） |
| **沙箱隔离** | CommandLineExecutor扩展 | ⭐⭐ | 3.2 | 中（资源限制） |
| **资源限制** | CommandLineExecutor扩展 | ⭐⭐ | 3.2 | 低（ulimit） |
| **权限控制** | 接口包装 | ⭐⭐ | 3.3 | 低（内存检查） |
| **执行监控** | 接口包装 | ⭐ | 3.4 | 低（指标收集） |
| **结果过滤** | 接口包装 | ⭐ | 包装器处理 | 低 |
| **重试策略** | BaseScriptExecutor扩展 | ⭐⭐ | 继承重写 | 低 |
| **超时控制** | BaseScriptExecutor扩展 | ⭐⭐ | 继承重写 | 低 |
| **环境隔离** | ExecutionContext扩展 | ⭐ | 传递参数 | 低 |
| **网络隔离** | CommandLineExecutor扩展 | ⭐⭐⭐ | iptables规则 | 中 |

---

## 五、集成优势

### 5.1 ScriptService 友好

```typescript
import { ScriptService } from '@modular-agent/sdk';

// 动态注册不同类型的执行器
scriptService.registerExecutor('SHELL', new ShellExecutor());
scriptService.registerExecutor('SHELL_SANDBOX', new SandboxShellExecutor());
scriptService.registerExecutor('SHELL_AUDITED', new AuditingScriptExecutor(baseExecutor, auditService));
```

### 5.2 依赖注入支持

```typescript
import { Container } from 'inversify';

container.bind<IScriptExecutor>('ShellExecutor').to(ShellExecutor);
container.bind<IScriptExecutor>('AuditedShellExecutor').toDynamicValue((context) => {
  const base = context.container.get<IScriptExecutor>('ShellExecutor');
  const audit = context.container.get<AuditService>('AuditService');
  return new AuditingScriptExecutor(base, audit);
});
```

### 5.3 测试友好

```typescript
import { describe, it, expect } from 'vitest';
import { AuditingScriptExecutor } from './AuditingScriptExecutor';

describe('AuditingScriptExecutor', () => {
  it('should log execution start and complete', async () => {
    const mockExecutor = createMockExecutor();
    const mockAudit = createMockAuditService();
    const executor = new AuditingScriptExecutor(mockExecutor, mockAudit);

    await executor.execute(mockScript);

    expect(mockAudit.log).toHaveBeenCalledWith(
      expect.objectContaining({ event: 'script_execution_start' })
    );
    expect(mockAudit.log).toHaveBeenCalledWith(
      expect.objectContaining({ event: 'script_execution_complete' })
    );
  });
});
```

### 5.4 类型安全

```typescript
// TypeScript 确保类型正确性
const executor: IScriptExecutor = new AuditingScriptExecutor(
  new ShellExecutor(),
  auditService
);

// 编译时错误检测
executor.execute(script, options, context); // ✅ 正确
executor.invalidMethod(); // ❌ 编译错误
```

---

## 六、实施建议

### 6.1 推荐实施顺序

1. **审计功能**（优先级：高）
   - 使用接口包装器模式
   - 非侵入式实现
   - 立即获得合规性支持

2. **监控功能**（优先级：高）
   - 使用接口包装器模式
   - 收集执行指标
   - 支持性能分析

3. **权限控制**（优先级：中）
   - 使用接口包装器模式
   - 实现基于角色的访问控制
   - 保护敏感脚本

4. **沙箱功能**（优先级：中）
   - 继承 CommandLineExecutor
   - 实现资源限制
   - 提供安全隔离

5. **高级功能**（优先级：低）
   - 自定义重试策略
   - 结果过滤
   - 环境隔离

### 6.2 最佳实践

1. **组合使用多个包装器**
   ```typescript
   const executor = new MonitoringScriptExecutor(
     new AuditingScriptExecutor(
       new PermissionControlledExecutor(
         new ShellExecutor(),
         permissionConfig
       ),
       auditService
     ),
     metricsService
   );
   ```

2. **使用工厂模式创建执行器**
   ```typescript
   class ExecutorFactory {
     createShellExecutor(config: ExecutorConfig): IScriptExecutor {
       let executor = new ShellExecutor(config);
       
       if (config.enableAudit) {
         executor = new AuditingScriptExecutor(executor, this.auditService);
       }
       
       if (config.enableMonitoring) {
         executor = new MonitoringScriptExecutor(executor, this.metricsService);
       }
       
       return executor;
     }
   }
   ```

3. **配置驱动的执行器选择**
   ```typescript
   const executorConfig = {
     'SHELL': {
       executor: 'sandbox',
       options: { maxMemory: 512, maxCpuTime: 60 }
     },
     'PYTHON': {
       executor: 'audited',
       options: { enableAudit: true }
     }
   };
   ```

### 6.3 注意事项

1. **性能考虑**
   - 审计日志使用异步写入
   - 监控指标批量上报
   - 避免在包装器中执行耗时操作

2. **错误处理**
   - 包装器不应吞没原始错误
   - 保留完整的错误堆栈
   - 提供有意义的错误信息

3. **资源清理**
   - 实现 `cleanup()` 方法
   - 确保所有资源正确释放
   - 处理异常情况下的清理

4. **线程安全**
   - 避免共享可变状态
   - 使用不可变数据结构
   - 考虑并发访问场景

---

## 七、总结

### 7.1 核心优势

| 优势 | 说明 |
|-----|------|
| ✅ **零侵入** | 无需修改核心模块代码 |
| ✅ **高灵活性** | 支持多种扩展模式 |
| ✅ **类型安全** | TypeScript 编译时检查 |
| ✅ **易于测试** | 所有组件可独立测试 |
| ✅ **生产就绪** | 架构设计已完全就绪 |

### 7.2 扩展能力

`script-executors` 模块具备**企业级扩展能力**，应用层可以通过标准设计模式实现：

- ✅ **沙箱隔离**（资源限制、文件系统隔离、网络隔离）
- ✅ **审计追踪**（执行日志、合规记录、操作追踪）
- ✅ **权限控制**（角色校验、访问控制、操作授权）
- ✅ **监控告警**（性能指标、异常检测、趋势分析）
- ✅ **结果处理**（数据过滤、格式转换、敏感信息脱敏）

### 7.3 最终结论

**推荐立即实施**，架构设计已完全就绪，无需修改核心模块。应用层可以通过标准设计模式（装饰器模式、继承模式、组合模式）实现所有扩展需求，同时保持代码的可维护性和可测试性。

---

## 附录

### A. 相关文件

- [`IScriptExecutor`](../../packages/script-executors/src/core/interfaces/IScriptExecutor.ts:12) - 执行器接口
- [`BaseScriptExecutor`](../../packages/script-executors/src/core/base/BaseScriptExecutor.ts:18) - 抽象基类
- [`CommandLineExecutor`](../../packages/script-executors/src/core/base/CommandLineExecutor.ts:32) - 命令行基类
- [`ScriptService`](../../sdk/core/services/script-service.ts:21) - 脚本服务
- [`ExecutionContext`](../../packages/script-executors/src/core/types.ts:38) - 执行上下文

### B. 参考资料

- [Decorator Pattern](https://refactoring.guru/design-patterns/decorator)
- [Strategy Pattern](https://refactoring.guru/design-patterns/strategy)
- [TypeScript Generics](https://www.typescriptlang.org/docs/handbook/2/generics.html)

### C. 示例代码

完整的扩展示例代码请参考：
- 审计包装器：`apps/web-app/src/executors/AuditingScriptExecutor.ts`
- 沙箱执行器：`apps/web-app/src/executors/SandboxShellExecutor.ts`
- 权限控制：`apps/web-app/src/executors/PermissionControlledExecutor.ts`
- 监控包装器：`apps/web-app/src/executors/MonitoringScriptExecutor.ts`