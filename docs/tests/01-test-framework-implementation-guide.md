# 基础测试框架实现指南

## 1. 概述

本文档说明如何实现 CLI-App 集成测试的基础测试框架，包括测试工具类、测试配置和辅助函数的实现。

## 2. 核心组件

### 2.1 CLI Runner（CLI 运行器）

**功能**：封装子进程调用 CLI 命令的功能

**设计要点**：
- 使用 Node.js `spawn` API 启动子进程
- 支持 stdout/stderr 捕获
- 支持超时控制
- 支持输入数据（stdin）
- 返回结构化的执行结果

**实现思路**：
```typescript
class CLIRunner {
  // 构造函数：指定 CLI 脚本路径
  constructor(cliPath: string)

  // 执行 CLI 命令
  async run(args: string[], options: {
    timeout?: number;      // 超时时间（毫秒）
    input?: string;        // 标准输入
    cwd?: string;          // 工作目录
    env?: Record<string, string>;  // 环境变量
  }): Promise<CLIRunResult>
}

interface CLIRunResult {
  exitCode: number | null;  // 退出码
  stdout: string;           // 标准输出
  stderr: string;           // 标准错误
  duration: number;         // 执行时长（毫秒）
}
```

**使用示例**：
```typescript
const runner = new CLIRunner('./scripts/modular-agent.js');
const result = await runner.run(['workflow', 'list', '-t'], {
  timeout: 30000,
  cwd: '/path/to/project'
});

console.log(result.exitCode);  // 0
console.log(result.stdout);    // 包含工作流列表
```

### 2.2 Test Helper（测试辅助类）

**功能**：提供测试辅助功能

**设计要点**：
- 临时目录管理（创建、清理）
- Fixture 文件路径解析
- ID 提取（从输出中提取 ID）
- 测试数据准备

**实现思路**：
```typescript
class TestHelper {
  // 构造函数：创建临时目录
  constructor(testName: string)

  // 获取临时目录路径
  getTempDir(): string

  // 获取 fixture 文件路径
  getFixturePath(...parts: string[]): string

  // 清理临时目录
  async cleanup(): Promise<void>

  // 从输出中提取 ID
  extractId(output: string, pattern: RegExp): string | null

  // 写入临时文件
  async writeTempFile(filename: string, content: string): Promise<string>
}
```

**使用示例**：
```typescript
const helper = new TestHelper('workflow-test');
const workflowFile = helper.getFixturePath('workflows', 'simple.toml');
const tempFile = await helper.writeTempFile('test.toml', '[workflow]\nname = "test"');

// 测试完成后清理
await helper.cleanup();
```

### 2.3 ID Extractor（ID 提取器）

**功能**：从 CLI 输出中提取各种资源的 ID

**设计要点**：
- 支持多种 ID 类型
- 使用正则表达式匹配
- 处理提取失败的情况

**实现思路**：
```typescript
// 便捷函数
function extractAgentId(output: string): string | null
function extractCheckpointId(output: string): string | null
function extractThreadId(output: string): string | null
function extractWorkflowId(output: string): string | null
```

**使用示例**：
```typescript
const result = await runCLI(['agent', 'start', ...]);
const agentId = extractAgentId(result.stdout);
if (agentId) {
  console.log(`Agent ID: ${agentId}`);
}
```

### 2.4 Mock SDK 配置

**功能**：配置测试环境的 Mock SDK

**设计要点**：
- 使用 Mock LLM Profile
- 禁用调试日志
- 配置超时时间

**实现思路**：
```typescript
function setupMockSDK(): void {
  getSDK({
    debug: false,
    logLevel: 'error',
    presets: {
      profiles: {
        DEFAULT: {
          provider: 'mock',
          model: 'mock-model',
          apiKey: 'mock-api-key',
        },
      },
    },
  });
}
```

## 3. 测试配置

### 3.1 Vitest 集成测试配置

**配置文件**：`vitest.integration.config.mjs`

**关键配置项**：
```javascript
export default defineConfig({
  test: {
    environment: 'node',
    include: ['**/__tests__/integration/**/*.test.ts'],
    exclude: ['node_modules', 'dist', 'coverage'],
    testTimeout: 60000,           // 集成测试超时时间
    teardownTimeout: 10000,       // 清理超时时间
    reporters: ['verbose'],       // 详细输出
    clearMocks: true,
    restoreMocks: true,
    globals: true,
    setupFiles: ['./__tests__/setup.ts'],
  },
  resolve: {
    alias: {
      // 配置路径别名
      '@modular-agent/sdk': resolve(__dirname, '../../sdk'),
      '@modular-agent/types': resolve(__dirname, '../../packages/types/src'),
      // ...
    },
  },
});
```

### 3.2 测试设置文件

**文件**：`__tests__/setup.ts`

**功能**：测试环境初始化

**实现思路**：
```typescript
import { beforeAll, afterAll } from 'vitest';

beforeAll(async () => {
  // 初始化测试环境
  console.log('Setting up integration test environment...');
  setupMockSDK();
});

afterAll(async () => {
  // 清理测试环境
  console.log('Cleaning up integration test environment...');
});
```

## 4. 测试工具实现

### 4.1 CLI Runner 实现细节

**核心代码结构**：
```typescript
import { spawn, ChildProcess } from 'child_process';

export class CLIRunner {
  private cliPath: string;
  private defaultEnv: Record<string, string>;

  constructor(cliPath?: string) {
    this.cliPath = cliPath || this.findCLIPath();
    this.defaultEnv = {
      ...process.env,
      NODE_ENV: 'test',
      // 可以添加其他测试环境变量
    };
  }

  async run(args: string[], options: {
    timeout?: number;
    input?: string;
    cwd?: string;
    env?: Record<string, string>;
  } = {}): Promise<CLIRunResult> {
    const startTime = Date.now();
    const {
      timeout = 30000,
      input,
      cwd = process.cwd(),
      env = {},
    } = options;

    return new Promise((resolve) => {
      const child = spawn('node', [this.cliPath, ...args], {
        env: { ...this.defaultEnv, ...env },
        cwd,
        stdio: ['pipe', 'pipe', 'pipe'],
      });

      let stdout = '';
      let stderr = '';

      // 捕获 stdout
      child.stdout?.on('data', (data) => {
        stdout += data.toString();
      });

      // 捕获 stderr
      child.stderr?.on('data', (data) => {
        stderr += data.toString();
      });

      // 发送输入
      if (input && child.stdin) {
        child.stdin.write(input);
        child.stdin.end();
      }

      // 超时处理
      const timer = setTimeout(() => {
        child.kill();
        resolve({
          exitCode: -1,
          stdout,
          stderr: `Timeout after ${timeout}ms`,
          duration: Date.now() - startTime,
        });
      }, timeout);

      // 进程结束处理
      child.on('close', (code) => {
        clearTimeout(timer);
        resolve({
          exitCode: code,
          stdout,
          stderr,
          duration: Date.now() - startTime,
        });
      });

      // 错误处理
      child.on('error', (error) => {
        clearTimeout(timer);
        resolve({
          exitCode: -1,
          stdout,
          stderr: error.message,
          duration: Date.now() - startTime,
        });
      });
    });
  }

  private findCLIPath(): string {
    // 查找 CLI 脚本路径的逻辑
    // 可以从多个位置查找
    return './scripts/modular-agent.js';
  }
}
```

### 4.2 Test Helper 实现细节

**核心代码结构**：
```typescript
import { mkdirSync, rmSync, existsSync, writeFileSync, readFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';

export class TestHelper {
  private tempDir: string;

  constructor(testName: string) {
    this.tempDir = join(tmpdir(), `cli-app-test-${testName}-${Date.now()}`);
    mkdirSync(this.tempDir, { recursive: true });
  }

  getTempDir(): string {
    return this.tempDir;
  }

  getFixturePath(...parts: string[]): string {
    return join(__dirname, 'fixtures', ...parts);
  }

  async cleanup(): Promise<void> {
    if (existsSync(this.tempDir)) {
      rmSync(this.tempDir, { recursive: true, force: true });
    }
  }

  extractId(output: string, pattern: RegExp): string | null {
    const match = output.match(pattern);
    return match ? match[1] : null;
  }

  async writeTempFile(filename: string, content: string): Promise<string> {
    const filepath = join(this.tempDir, filename);
    writeFileSync(filepath, content, 'utf-8');
    return filepath;
  }

  readTempFile(filename: string): string {
    const filepath = join(this.tempDir, filename);
    return readFileSync(filepath, 'utf-8');
  }
}
```

### 4.3 ID Extractor 实现

**核心代码结构**：
```typescript
export function extractAgentId(output: string): string | null {
  const match = output.match(/Agent Loop 已启动: ([\w-]+)/);
  return match ? match[1] : null;
}

export function extractCheckpointId(output: string): string | null {
  const match = output.match(/检查点已创建: ([\w-]+)/);
  return match ? match[1] : null;
}

export function extractThreadId(output: string): string | null {
  const match = output.match(/线程已创建: ([\w-]+)/);
  return match ? match[1] : null;
}

export function extractWorkflowId(output: string): string | null {
  const match = output.match(/工作流已注册: ([\w-]+)/);
  return match ? match[1] : null;
}
```

## 5. 测试用例模板

### 5.1 基础测试用例模板

```typescript
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { runCLI } from '../../utils/cli-runner';
import { TestHelper } from '../../utils/test-helpers';

describe('Module Name Integration Tests', () => {
  let helper: TestHelper;

  beforeEach(() => {
    helper = new TestHelper('module-name');
  });

  afterEach(async () => {
    await helper.cleanup();
  });

  it('should perform a specific operation', async () => {
    // 准备测试数据
    const fixture = helper.getFixturePath('module', 'test-file.toml');

    // 执行 CLI 命令
    const result = await runCLI(['command', 'subcommand', fixture]);

    // 验证结果
    expect(result.exitCode).toBe(0);
    expect(result.stdout).toContain('expected message');
    expect(result.stderr).toBe('');
  });
});
```

### 5.2 复杂测试用例模板

```typescript
describe('Complex Operation Integration Tests', () => {
  let helper: TestHelper;
  let resourceId: string | null;

  beforeEach(() => {
    helper = new TestHelper('complex-operation');
  });

  afterEach(async () => {
    // 清理资源
    if (resourceId) {
      await runCLI(['command', 'delete', resourceId, '-f']);
    }
    await helper.cleanup();
  });

  it('should perform multi-step operation', async () => {
    // 步骤 1: 创建资源
    const createResult = await runCLI(['command', 'create', ...]);
    expect(createResult.exitCode).toBe(0);
    resourceId = extractId(createResult.stdout, /ID: (\w+)/);
    expect(resourceId).toBeTruthy();

    // 步骤 2: 修改资源
    const updateResult = await runCLI(['command', 'update', resourceId, ...]);
    expect(updateResult.exitCode).toBe(0);

    // 步骤 3: 验证资源状态
    const statusResult = await runCLI(['command', 'status', resourceId]);
    expect(statusResult.exitCode).toBe(0);
    expect(statusResult.stdout).toContain('expected status');

    // 步骤 4: 删除资源（在 afterEach 中统一清理）
  });
});
```

## 6. 测试执行

### 6.1 命令行执行

```bash
# 运行所有集成测试
cd apps/cli-app
pnpm test:integration

# 运行特定模块的测试
pnpm test:integration workflow
pnpm test:integration agent

# 运行特定测试文件
pnpm vitest run __tests__/integration/workflow/workflow-lifecycle.test.ts

# 使用详细输出
pnpm test:integration --reporter=verbose

# 监视模式
pnpm test:integration --watch

# 生成覆盖率报告
pnpm test:integration --coverage
```

### 6.2 NPM Scripts 配置

在 `apps/cli-app/package.json` 中添加：

```json
{
  "scripts": {
    "test:integration": "vitest run --config vitest.integration.config.mjs",
    "test:integration:watch": "vitest --config vitest.integration.config.mjs",
    "test:integration:coverage": "vitest run --config vitest.integration.config.mjs --coverage"
  }
}
```

## 7. 最佳实践

### 7.1 测试隔离

- 每个测试用例使用独立的临时目录
- 测试完成后清理所有资源
- 使用 `beforeEach` 和 `afterEach` 进行设置和清理

### 7.2 超时控制

- 为每个测试设置合理的超时时间
- 集成测试默认超时时间 60 秒
- 长时间运行的测试需要特殊处理

### 7.3 错误处理

- 验证错误场景的输出
- 检查 stderr 内容
- 验证退出码

### 7.4 资源清理

- 在 `afterEach` 中清理创建的资源
- 使用 `--force` 选项避免交互式提示
- 记录清理失败的资源

### 7.5 测试命名

- 使用描述性的测试名称
- 名称应该清楚说明测试的目的
- 使用 `should` 或 `should not` 等词汇

## 8. 常见问题

### 8.1 测试超时

**问题**：测试执行超时

**解决方案**：
- 增加测试超时时间
- 检查是否有死锁
- 优化测试逻辑

### 8.2 资源清理失败

**问题**：资源清理失败导致后续测试失败

**解决方案**：
- 使用 `--force` 选项
- 增加清理重试逻辑
- 记录清理失败的资源

### 8.3 ID 提取失败

**问题**：无法从输出中提取 ID

**解决方案**：
- 检查正则表达式是否正确
- 检查输出格式是否变化
- 增加容错处理

### 8.4 临时目录权限问题

**问题**：无法创建或访问临时目录

**解决方案**：
- 检查文件系统权限
- 使用系统临时目录
- 添加错误处理

## 9. 扩展功能

### 9.1 并发测试执行

可以使用 Vitest 的并发执行功能提高测试速度：

```javascript
export default defineConfig({
  test: {
    pool: 'threads',  // 使用线程池
    poolOptions: {
      threads: {
        singleThread: false,
        minThreads: 2,
        maxThreads: 4,
      },
    },
  },
});
```

### 9.2 测试数据生成

可以使用 Faker 等库生成测试数据：

```typescript
import { faker } from '@faker-js/faker';

const testWorkflow = {
  id: faker.string.uuid(),
  name: faker.word.noun(),
  description: faker.lorem.sentence(),
};
```

### 9.3 快照测试

可以使用 Vitest 的快照测试功能：

```typescript
it('should produce expected output', async () => {
  const result = await runCLI(['workflow', 'list']);
  expect(result.stdout).toMatchSnapshot();
});
```

## 10. 总结

基础测试框架提供了：

1. **CLI Runner**：封装 CLI 命令执行
2. **Test Helper**：提供测试辅助功能
3. **ID Extractor**：提取资源 ID
4. **Mock SDK**：配置测试环境
5. **测试配置**：Vitest 集成测试配置

使用这些工具可以快速编写和执行集成测试，验证 CLI-App 和 SDK 的功能正确性。