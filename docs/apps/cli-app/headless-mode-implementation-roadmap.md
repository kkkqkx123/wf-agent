# CLI App 无头模式实施路线图

## 概述

本文档提供增强 CLI App 无头模式的详细实施计划和路线图，包括优先级划分、实施步骤和验收标准。

## 实施优先级

### P0 - 关键（立即实施）

解决当前影响测试稳定性和 CI/CD 集成的问题。

| 任务 | 说明 | 影响范围 |
|------|------|----------|
| 修复无头模式退出问题 | 解决测试超时和输出捕获失败 | 测试框架、CI/CD |
| 分离 TEST_MODE 与 HEADLESS | 清晰的模式定义和环境变量 | 配置管理 |

### P1 - 高优先级（短期实施）

增强无头模式的功能性和稳定性。

| 任务 | 说明 | 影响范围 |
|------|------|----------|
| 实现执行模式管理器 | 统一的模式检测和配置 | 核心架构 |
| 增强输出管理器 | 支持 JSON 输出和结构化数据 | 输出系统 |
| 完善退出机制 | 安全退出和错误处理 | 生命周期管理 |

### P2 - 中优先级（中期实施）

支持跨应用复用和程序化调用。

| 任务 | 说明 | 影响范围 |
|------|------|----------|
| 提取 CoreService 层 | 业务逻辑与 CLI 框架解耦 | 架构重构 |
| 实现程序化 API | 支持其他应用直接调用 | 公共 API |
| 文档和示例 | 使用文档和最佳实践 | 开发者体验 |

### P3 - 低优先级（长期规划）

优化和扩展功能。

| 任务 | 说明 | 影响范围 |
|------|------|----------|
| 批处理模式优化 | 支持批量操作和并行执行 | 性能优化 |
| 远程执行支持 | 通过网络调用 CLI 功能 | 分布式支持 |
| 插件系统 | 支持自定义输出格式 | 扩展性 |

## 详细实施计划

### Phase 1: 基础修复（P0）

**目标**：解决当前无头模式的稳定性问题

**时间估算**：3-5 天

#### 1.1 修复退出机制

**文件**：`src/index.ts`, `src/utils/exit-manager.ts` (新建)

**改动内容**：

```typescript
// src/utils/exit-manager.ts (新建文件)
export class ExitManager {
  static async exit(code: number = 0): Promise<never> {
    const output = getOutput();
    await output.ensureDrained();
    process.exit(code);
  }
}

// src/index.ts 修改
program.hook("postAction", async () => {
  const isHeadless = process.env["HEADLESS"] === "true" || 
                     process.env["TEST_MODE"] === "true";
  if (isHeadless) {
    await ExitManager.exit(0);
  }
});
```

**验收标准**：
- [ ] 测试用例不再出现 30 秒超时
- [ ] 输出内容完整捕获
- [ ] 退出码正确返回

#### 1.2 分离环境变量

**文件**：`src/index.ts`

**改动内容**：

引入新的 `CLI_MODE` 环境变量，保持 `HEADLESS` 和 `TEST_MODE` 向后兼容。

```typescript
function detectMode(): 'interactive' | 'headless' | 'programmatic' {
  if (process.env['CLI_MODE']) {
    return process.env['CLI_MODE'] as any;
  }
  if (process.env['HEADLESS'] === 'true' || process.env['TEST_MODE'] === 'true') {
    return 'headless';
  }
  return 'interactive';
}
```

**验收标准**：
- [ ] `CLI_MODE=headless` 正常工作
- [ ] `HEADLESS=true` 继续兼容
- [ ] `TEST_MODE=true` 继续兼容

### Phase 2: 架构增强（P1）

**目标**：建立完善的执行模式管理体系

**时间估算**：5-7 天

#### 2.1 实现执行模式管理器

**文件**：
- `src/types/execution-mode.ts` (新建)
- `src/execution/execution-mode-manager.ts` (新建)

**实现内容**：

参考 [headless-mode-design.md](./headless-mode-design.md) 中的设计实现。

**验收标准**：
- [ ] 所有模式检测通过单元测试
- [ ] 配置覆盖逻辑正确
- [ ] 环境变量解析正确

#### 2.2 增强输出管理器

**文件**：`src/utils/output.ts`

**改动内容**：

```typescript
export class CLIOutput {
  // 新增方法
  structuredOutput(data: unknown): void {
    if (this.isJsonMode()) {
      this._stdout.write(JSON.stringify(data) + "\n");
    }
  }
  
  result(data: unknown, options?: { message?: string; success?: boolean }): void {
    // 根据模式自动选择输出格式
  }
  
  async ensureDrained(): Promise<void> {
    // 等待所有输出流完成
  }
}
```

**验收标准**：
- [ ] JSON 输出格式正确
- [ ] 结构化数据包含必要字段
- [ ] 输出流正确刷新

#### 2.3 适配器层改造

**文件**：`src/adapters/*.ts`

**改动内容**：

为适配器添加模式感知输出：

```typescript
// src/adapters/workflow-adapter.ts 示例
async registerFromFile(filePath: string): Promise<Workflow> {
  return this.executeWithErrorHandling(async () => {
    const workflow = await this.configManager.loadWorkflow(fullPath);
    await this.sdk.workflows.create(workflow);
    
    // 根据模式输出
    if (this.modeManager.isJsonOutput()) {
      this.output.structuredOutput({ success: true, data: workflow });
    } else {
      this.output.success(`Workflow registered: ${workflow.name}`);
    }
    
    return workflow;
  }, "register-workflow");
}
```

**验收标准**：
- [ ] 所有适配器支持 JSON 输出
- [ ] 错误信息结构化
- [ ] 向后兼容交互模式

### Phase 3: 程序化 API（P2）

**目标**：支持其他应用直接调用 CLI 功能

**时间估算**：7-10 天

#### 3.1 提取 CoreService 层

**文件**：
- `src/core/workflow-service.ts` (新建)
- `src/core/thread-service.ts` (新建)
- ... 其他服务

**设计原则**：

```typescript
// 业务逻辑与 CLI 框架分离
export class WorkflowService {
  async register(filePath: string, parameters?: Record<string, unknown>): Promise<Workflow> {
    // 纯业务逻辑，无输出操作
    const workflow = await this.loadWorkflow(filePath);
    await this.validate(workflow);
    await this.save(workflow);
    return workflow;
  }
}

// 适配器负责输出
export class WorkflowAdapter {
  async registerFromFile(filePath: string): Promise<Workflow> {
    const workflow = await this.service.register(filePath);
    // 根据模式输出结果
    this.outputResult(workflow);
    return workflow;
  }
}
```

**验收标准**：
- [ ] 服务层可独立测试
- [ ] 无 CLI 框架依赖
- [ ] 适配器正确代理服务调用

#### 3.2 实现程序化 API

**文件**：`src/api/programmatic-api.ts` (新建)

**实现内容**：

参考 [headless-mode-design.md](./headless-mode-design.md) 中的设计实现。

**验收标准**：
- [ ] API 可独立初始化
- [ ] 所有主要功能可通过 API 调用
- [ ] 类型定义完整

#### 3.3 导出公共 API

**文件**：`src/api/index.ts` (新建), `package.json`

**改动内容**：

```json
// package.json
{
  "exports": {
    ".": "./dist/index.js",
    "./api": "./dist/api/index.js"
  }
}
```

```typescript
// src/api/index.ts
export { createAPI, ProgrammaticAPI } from './programmatic-api.js';
export type { ProgrammaticAPIOptions } from './programmatic-api.js';
```

**验收标准**：
- [ ] 其他应用可通过包引用使用 API
- [ ] 类型定义正确导出
- [ ] 文档示例可运行

### Phase 4: 文档和优化（P3）

**目标**：完善文档和优化性能

**时间估算**：5 天

#### 4.1 编写使用文档

**文件**：
- `docs/apps/cli-app/headless-mode-usage.md` (新建)
- `docs/apps/cli-app/programmatic-api-guide.md` (新建)

**内容**：
- 无头模式使用指南
- 程序化 API 参考
- 最佳实践
- 故障排除

#### 4.2 性能优化

**优化项**：
- 程序化模式下跳过终端初始化
- 减少不必要的日志输出
- 优化 JSON 序列化

## 测试策略

### 单元测试

```typescript
// __tests__/execution/execution-mode-manager.test.ts
describe('ExecutionModeManager', () => {
  test('should detect headless mode from CLI_MODE', () => {
    process.env['CLI_MODE'] = 'headless';
    const manager = ExecutionModeManager.getInstance();
    expect(manager.isHeadless()).toBe(true);
  });
  
  test('should maintain backward compatibility with HEADLESS', () => {
    process.env['HEADLESS'] = 'true';
    const manager = ExecutionModeManager.getInstance();
    expect(manager.isHeadless()).toBe(true);
  });
});
```

### 集成测试

```typescript
// __tests__/integration/headless-mode.test.ts
describe('Headless Mode Integration', () => {
  test('should output JSON in headless mode', async () => {
    const result = await runCLI(['workflow', 'list'], {
      env: { CLI_MODE: 'headless', CLI_OUTPUT_FORMAT: 'json' }
    });
    
    const output = JSON.parse(result.stdout);
    expect(output).toHaveProperty('success');
    expect(output).toHaveProperty('data');
  });
  
  test('should exit cleanly in headless mode', async () => {
    const result = await runCLI(['workflow', 'list'], {
      env: { CLI_MODE: 'headless' },
      timeout: 5000
    });
    
    expect(result.exitCode).toBe(0);
    expect(result.duration).toBeLessThan(1000);
  });
});
```

### 程序化 API 测试

```typescript
// __tests__/api/programmatic-api.test.ts
describe('ProgrammaticAPI', () => {
  test('should register workflow via API', async () => {
    const api = createAPI({ logLevel: 'silent' });
    await api.initialize();
    
    const workflow = await api.workflows.register('./test-workflow.toml');
    expect(workflow.id).toBeDefined();
  });
});
```

## 风险评估

### 高风险

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 向后兼容性破坏 | 现有脚本失效 | 保持旧环境变量兼容，渐进式迁移 |
| 测试不稳定 | 开发效率下降 | 充分测试后再合并，提供回滚方案 |

### 中风险

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 性能下降 | 响应变慢 | 性能基准测试，优化关键路径 |
| 代码复杂度增加 | 维护困难 | 清晰的模块划分，完善文档 |

### 低风险

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 文档不完整 | 使用困难 | 同步更新文档，提供示例 |

## 时间线

```
Week 1-2: Phase 1 (P0)
  - 修复退出机制
  - 分离环境变量
  - 集成测试验证

Week 3-4: Phase 2 (P1)
  - 执行模式管理器
  - 输出管理器增强
  - 适配器层改造

Week 5-6: Phase 3 (P2)
  - CoreService 提取
  - 程序化 API 实现
  - 公共 API 导出

Week 7: Phase 4 (P3)
  - 文档编写
  - 性能优化
  - 最终验收
```

## 验收标准汇总

### 功能验收

- [ ] 无头模式稳定退出，无超时
- [ ] JSON 输出格式正确且完整
- [ ] 程序化 API 可独立使用
- [ ] 向后兼容现有用法

### 性能验收

- [ ] 无头模式启动时间 < 500ms
- [ ] 程序化 API 调用开销 < 100ms
- [ ] 内存占用无显著增加

### 质量验收

- [ ] 单元测试覆盖率 > 80%
- [ ] 集成测试全部通过
- [ ] 文档完整且准确

## 相关文档

- [execution-modes-analysis.md](./execution-modes-analysis.md) - 执行模式架构分析
- [headless-mode-design.md](./headless-mode-design.md) - 无头模式设计方案
- [headless-mode-usage.md](./headless-mode-usage.md) - 无头模式使用指南（待创建）
- [programmatic-api-guide.md](./programmatic-api-guide.md) - 程序化 API 指南（待创建）
