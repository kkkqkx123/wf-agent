# CLI-App 集成测试设计文档

## 1. 测试目标

本集成测试以 `apps/cli-app` 为简易执行框架，验证 `sdk` 模块的核心功能通过 CLI 接口正常工作。测试重点在于：

- 验证 SDK 各模块的功能正确性
- 测试 CLI 命令与 SDK API 的集成
- 确保端到端的用户场景能够正常执行
- 发现和修复集成问题

## 2. 测试架构设计

### 2.1 测试框架选择

- **测试框架**：Vitest（与 SDK 单元测试保持一致）
- **测试执行方式**：通过 CLI 命令行调用
- **断言库**：Vitest 内置断言
- **Mock 工具**：Vitest 内置 mock 功能

### 2.2 测试目录结构

```
apps/cli-app/
├── __tests__/
│   ├── integration/
│   │   ├── workflow/
│   │   │   ├── workflow-lifecycle.test.ts
│   │   │   └── workflow-execution.test.ts
│   │   ├── agent/
│   │   │   ├── agent-loop-lifecycle.test.ts
│   │   │   ├── agent-loop-execution.test.ts
│   │   │   └── agent-loop-checkpoint.test.ts
│   │   ├── thread/
│   │   │   ├── thread-lifecycle.test.ts
│   │   │   └── thread-execution.test.ts
│   │   ├── checkpoint/
│   │   │   └── checkpoint-management.test.ts
│   │   ├── llm/
│   │   │   └── llm-profile.test.ts
│   │   ├── tools/
│   │   │   └── tool-execution.test.ts
│   │   ├── scripts/
│   │   │   └── script-execution.test.ts
│   │   ├── message/
│   │   │   └── message-management.test.ts
│   │   ├── variable/
│   │   │   └── variable-management.test.ts
│   │   ├── event/
│   │   │   └── event-management.test.ts
│   │   ├── trigger/
│   │   │   └── trigger-management.test.ts
│   │   ├── human-relay/
│   │   │   └── human-relay.test.ts
│   │   ├── skill/
│   │   │   └── skill-management.test.ts
│   │   └── e2e/
│   │       ├── complete-workflow.test.ts
│   │       └── complete-agent-loop.test.ts
│   ├── fixtures/
│   │   ├── workflows/
│   │   │   ├── simple-workflow.toml
│   │   │   ├── conditional-workflow.toml
│   │   │   └── complex-workflow.toml
│   │   ├── agent-loops/
│   │   │   ├── simple-agent.toml
│   │   │   ├── multi-tool-agent.toml
│   │   │   └── checkpoint-agent.toml
│   │   ├── scripts/
│   │   │   ├── simple-script.js
│   │   │   └── complex-script.ts
│   │   └── tools/
│   │       └── custom-tool.toml
│   ├── utils/
│   │   ├── cli-runner.ts
│   │   ├── test-helpers.ts
│   │   └── mock-sdk.ts
│   └── setup.ts
├── vitest.config.mjs
└── vitest.integration.config.mjs
```

### 2.3 测试执行流程

```
测试用例
  ↓
调用 CLI 命令（通过子进程）
  ↓
CLI 命令执行 SDK API
  ↓
SDK 执行业务逻辑
  ↓
返回结果到 CLI
  ↓
CLI 输出结果
  ↓
测试用例验证结果
```

### 2.4 测试数据管理

- **测试fixtures**：存储在 `__tests__/fixtures/` 目录
- **临时文件**：使用 `tmpdir` 创建临时目录
- **测试隔离**：每个测试用例独立运行，避免相互影响
- **清理机制**：测试完成后自动清理临时数据和资源

## 3. 需要测试的功能模块

### 3.1 Workflow 模块

#### 功能清单
- [ ] 从文件注册工作流
- [ ] 从目录批量注册工作流
- [ ] 列出所有工作流
- [ ] 查看工作流详情
- [ ] 删除工作流
- [ ] 工作流参数替换
- [ ] 工作流验证

#### 测试方法
```typescript
// 1. 注册单个工作流
await runCLI(['workflow', 'register', 'simple-workflow.toml'])
expect(result.exitCode).toBe(0)
expect(result.stdout).toContain('工作流已注册')

// 2. 批量注册工作流
await runCLI(['workflow', 'register-batch', './fixtures/workflows', '-r'])
expect(result.exitCode).toBe(0)
expect(result.stdout).toContain('成功注册')

// 3. 列出工作流
await runCLI(['workflow', 'list', '-t'])
expect(result.exitCode).toBe(0)
expect(result.stdout).toContain('ID')
expect(result.stdout).toContain('名称')

// 4. 查看工作流详情
await runCLI(['workflow', 'show', 'workflow-id'])
expect(result.exitCode).toBe(0)
expect(result.stdout).toContain('workflow-id')

// 5. 删除工作流
await runCLI(['workflow', 'delete', 'workflow-id', '-f'])
expect(result.exitCode).toBe(0)
```

### 3.2 Agent Loop 模块

#### 功能清单
- [ ] 创建 Agent Loop 实例
- [ ] 同步执行 Agent Loop
- [ ] 流式执行 Agent Loop
- [ ] 异步启动 Agent Loop
- [ ] 暂停 Agent Loop
- [ ] 恢复 Agent Loop
- [ ] 停止 Agent Loop
- [ ] 查看状态
- [ ] 查看详情
- [ ] 列出实例
- [ ] 创建检查点
- [ ] 从检查点恢复
- [ ] 克隆实例
- [ ] 清理资源
- [ ] 查看消息历史
- [ ] 查看变量
- [ ] 设置变量

#### 测试方法
```typescript
// 1. 同步执行 Agent Loop
await runCLI(['agent', 'run', '-p', 'DEFAULT', '-s', 'You are a helpful assistant', '-i', '{"message": "Hello"}'])
expect(result.exitCode).toBe(0)
expect(result.stdout).toContain('Agent Loop 执行完成')

// 2. 流式执行 Agent Loop
await runCLI(['agent', 'run', '--stream', '-p', 'DEFAULT', '-s', 'You are a helpful assistant'])
expect(result.exitCode).toBe(0)
expect(result.stdout).toContain('[迭代')

// 3. 异步启动 Agent Loop
await runCLI(['agent', 'start', '-p', 'DEFAULT', '-s', 'You are a helpful assistant'])
expect(result.exitCode).toBe(0)
const agentId = extractAgentId(result.stdout)

// 4. 暂停 Agent Loop
await runCLI(['agent', 'pause', agentId])
expect(result.exitCode).toBe(0)

// 5. 恢复 Agent Loop
await runCLI(['agent', 'resume', agentId])
expect(result.exitCode).toBe(0)

// 6. 创建检查点
await runCLI(['agent', 'checkpoint', agentId, '-n', 'test-checkpoint'])
expect(result.exitCode).toBe(0)
const checkpointId = extractCheckpointId(result.stdout)

// 7. 从检查点恢复
await runCLI(['agent', 'restore', checkpointId])
expect(result.exitCode).toBe(0)

// 8. 查看消息历史
await runCLI(['agent', 'messages', agentId, '-v'])
expect(result.exitCode).toBe(0)
expect(result.stdout).toContain('role')

// 9. 查看变量
await runCLI(['agent', 'variables', agentId, '-t'])
expect(result.exitCode).toBe(0)
```

### 3.3 Thread 模块

#### 功能清单
- [ ] 执行线程
- [ ] 暂停线程
- [ ] 恢复线程
- [ ] 取消线程
- [ ] 查看线程状态
- [ ] 查看线程详情
- [ ] 列出线程
- [ ] 订阅线程事件

#### 测试方法
```typescript
// 1. 执行线程
await runCLI(['thread', 'execute', 'workflow-id', '-p', '{"key": "value"}'])
expect(result.exitCode).toBe(0)
const threadId = extractThreadId(result.stdout)

// 2. 暂停线程
await runCLI(['thread', 'pause', threadId])
expect(result.exitCode).toBe(0)

// 3. 恢复线程
await runCLI(['thread', 'resume', threadId])
expect(result.exitCode).toBe(0)

// 4. 查看线程状态
await runCLI(['thread', 'status', threadId])
expect(result.exitCode).toBe(0)
expect(result.stdout).toContain('running')

// 5. 列出线程
await runCLI(['thread', 'list', '-t'])
expect(result.exitCode).toBe(0)
```

### 3.4 Checkpoint 模块

#### 功能清单
- [ ] 创建检查点
- [ ] 列出检查点
- [ ] 查看检查点详情
- [ ] 删除检查点
- [ ] 从检查点恢复线程
- [ ] 从检查点恢复 Agent Loop

#### 测试方法
```typescript
// 1. 创建检查点
await runCLI(['checkpoint', 'create', 'thread-id', '-n', 'test-checkpoint'])
expect(result.exitCode).toBe(0)
const checkpointId = extractCheckpointId(result.stdout)

// 2. 列出检查点
await runCLI(['checkpoint', 'list', 'thread-id'])
expect(result.exitCode).toBe(0)
expect(result.stdout).toContain(checkpointId)

// 3. 查看检查点详情
await runCLI(['checkpoint', 'show', checkpointId])
expect(result.exitCode).toBe(0)

// 4. 从检查点恢复线程
await runCLI(['checkpoint', 'restore-thread', checkpointId])
expect(result.exitCode).toBe(0)
```

### 3.5 LLM Profile 模块

#### 功能清单
- [ ] 注册 LLM Profile
- [ ] 列出 LLM Profile
- [ ] 查看 LLM Profile 详情
- [ ] 删除 LLM Profile
- [ ] 更新 LLM Profile
- [ ] 设置默认 Profile

#### 测试方法
```typescript
// 1. 注册 LLM Profile
await runCLI(['llm-profile', 'register', 'profile.toml'])
expect(result.exitCode).toBe(0)

// 2. 列出 LLM Profile
await runCLI(['llm-profile', 'list'])
expect(result.exitCode).toBe(0)
expect(result.stdout).toContain('DEFAULT')

// 3. 查看 LLM Profile 详情
await runCLI(['llm-profile', 'show', 'DEFAULT'])
expect(result.exitCode).toBe(0)
```

### 3.6 Tool 模块

#### 功能清单
- [ ] 注册工具
- [ ] 列出工具
- [ ] 查看工具详情
- [ ] 删除工具
- [ ] 执行工具
- [ ] 测试工具

#### 测试方法
```typescript
// 1. 注册工具
await runCLI(['tool', 'register', 'tool.toml'])
expect(result.exitCode).toBe(0)

// 2. 列出工具
await runCLI(['tool', 'list'])
expect(result.exitCode).toBe(0)

// 3. 执行工具
await runCLI(['tool', 'execute', 'tool-id', '-p', '{"arg": "value"}'])
expect(result.exitCode).toBe(0)
```

### 3.7 Script 模块

#### 功能清单
- [ ] 注册脚本
- [ ] 列出脚本
- [ ] 查看脚本详情
- [ ] 删除脚本
- [ ] 执行脚本
- [ ] 测试脚本
- [ ] 批量测试脚本

#### 测试方法
```typescript
// 1. 注册脚本
await runCLI(['script', 'register', 'script.js'])
expect(result.exitCode).toBe(0)

// 2. 执行脚本
await runCLI(['script', 'execute', 'script-id', '-p', '{"arg": "value"}'])
expect(result.exitCode).toBe(0)

// 3. 测试脚本
await runCLI(['script', 'test', 'script-id'])
expect(result.exitCode).toBe(0)
```

### 3.8 Message 模块

#### 功能清单
- [ ] 发送消息
- [ ] 列出消息
- [ ] 查看消息详情
- [ ] 删除消息
- [ ] 导出消息

#### 测试方法
```typescript
// 1. 发送消息
await runCLI(['message', 'send', 'thread-id', 'Hello world'])
expect(result.exitCode).toBe(0)

// 2. 列出消息
await runCLI(['message', 'list', 'thread-id'])
expect(result.exitCode).toBe(0)
```

### 3.9 Variable 模块

#### 功能清单
- [ ] 设置变量
- [ ] 获取变量
- [ ] 列出变量
- [ ] 删除变量
- [ ] 批量设置变量

#### 测试方法
```typescript
// 1. 设置变量
await runCLI(['variable', 'set', 'thread-id', 'var-name', 'var-value'])
expect(result.exitCode).toBe(0)

// 2. 获取变量
await runCLI(['variable', 'get', 'thread-id', 'var-name'])
expect(result.exitCode).toBe(0)
expect(result.stdout).toContain('var-value')

// 3. 列出变量
await runCLI(['variable', 'list', 'thread-id'])
expect(result.exitCode).toBe(0)
```

### 3.10 Event 模块

#### 功能清单
- [ ] 分发事件
- [ ] 订阅事件
- [ ] 取消订阅
- [ ] 查询事件历史
- [ ] 查看事件统计

#### 测试方法
```typescript
// 1. 分发事件
await runCLI(['event', 'dispatch', 'event-type', '{"data": "value"}'])
expect(result.exitCode).toBe(0)

// 2. 查询事件历史
await runCLI(['event', 'history', 'event-type'])
expect(result.exitCode).toBe(0)

// 3. 查看事件统计
await runCLI(['event', 'stats'])
expect(result.exitCode).toBe(0)
```

### 3.11 Trigger 模块

#### 功能清单
- [ ] 注册触发器
- [ ] 启用触发器
- [ ] 禁用触发器
- [ ] 列出触发器
- [ ] 查看触发器详情
- [ ] 删除触发器

#### 测试方法
```typescript
// 1. 注册触发器
await runCLI(['trigger', 'register', 'trigger.toml'])
expect(result.exitCode).toBe(0)

// 2. 启用触发器
await runCLI(['trigger', 'enable', 'trigger-id'])
expect(result.exitCode).toBe(0)

// 3. 列出触发器
await runCLI(['trigger', 'list'])
expect(result.exitCode).toBe(0)
```

### 3.12 Human Relay 模块

#### 功能清单
- [ ] 创建人工中继
- [ ] 查询待处理请求
- [ ] 处理请求
- [ ] 拒绝请求
- [ ] 列出历史记录

#### 测试方法
```typescript
// 1. 查询待处理请求
await runCLI(['human-relay', 'pending'])
expect(result.exitCode).toBe(0)

// 2. 处理请求
await runCLI(['human-relay', 'approve', 'request-id', '{"response": "approved"}'])
expect(result.exitCode).toBe(0)

// 3. 拒绝请求
await runCLI(['human-relay', 'reject', 'request-id', '{"reason": "rejected"}'])
expect(result.exitCode).toBe(0)
```

### 3.13 Skill 模块

#### 功能清单
- [ ] 注册 Skill
- [ ] 列出 Skill
- [ ] 查看 Skill 详情
- [ ] 删除 Skill
- [ ] 执行 Skill
- [ ] 测试 Skill

#### 测试方法
```typescript
// 1. 注册 Skill
await runCLI(['skill', 'register', 'skill-path'])
expect(result.exitCode).toBe(0)

// 2. 列出 Skill
await runCLI(['skill', 'list'])
expect(result.exitCode).toBe(0)

// 3. 执行 Skill
await runCLI(['skill', 'execute', 'skill-id', '-p', '{"input": "value"}'])
expect(result.exitCode).toBe(0)
```

## 4. 测试方法和验证策略

### 4.1 测试工具类设计

```typescript
// __tests__/utils/cli-runner.ts
import { spawn, ChildProcess } from 'child_process'
import { join } from 'path'

export interface CLIRunResult {
  exitCode: number | null
  stdout: string
  stderr: string
}

export class CLIRunner {
  private cliPath: string
  private env: Record<string, string>

  constructor(cliPath?: string) {
    this.cliPath = cliPath || join(process.cwd(), 'scripts/modular-agent.js')
    this.env = {
      ...process.env,
      NODE_ENV: 'test',
    }
  }

  async run(args: string[], options: {
    timeout?: number
    input?: string
    cwd?: string
  } = {}): Promise<CLIRunResult> {
    const { timeout = 30000, input, cwd } = options

    return new Promise((resolve) => {
      const child = spawn('node', [this.cliPath, ...args], {
        env: this.env,
        cwd: cwd || process.cwd(),
        stdio: ['pipe', 'pipe', 'pipe'],
      })

      let stdout = ''
      let stderr = ''

      child.stdout?.on('data', (data) => {
        stdout += data.toString()
      })

      child.stderr?.on('data', (data) => {
        stderr += data.toString()
      })

      if (input && child.stdin) {
        child.stdin.write(input)
        child.stdin.end()
      }

      const timer = setTimeout(() => {
        child.kill()
        resolve({
          exitCode: -1,
          stdout,
          stderr: `Timeout after ${timeout}ms`,
        })
      }, timeout)

      child.on('close', (code) => {
        clearTimeout(timer)
        resolve({
          exitCode: code,
          stdout,
          stderr,
        })
      })
    })
  }
}

// 便捷函数
export async function runCLI(args: string[], options?: any): Promise<CLIRunResult> {
  const runner = new CLIRunner()
  return runner.run(args, options)
}
```

### 4.2 测试辅助函数

```typescript
// __tests__/utils/test-helpers.ts
import { mkdirSync, rmSync, existsSync } from 'fs'
import { join } from 'path'
import { tmpdir } from 'os'

export class TestHelper {
  private tempDir: string

  constructor(testName: string) {
    this.tempDir = join(tmpdir(), `cli-app-test-${testName}-${Date.now()}`)
    mkdirSync(this.tempDir, { recursive: true })
  }

  getTempDir(): string {
    return this.tempDir
  }

  getFixturePath(...parts: string[]): string {
    return join(__dirname, 'fixtures', ...parts)
  }

  async cleanup(): Promise<void> {
    if (existsSync(this.tempDir)) {
      rmSync(this.tempDir, { recursive: true, force: true })
    }
  }

  extractId(output: string, pattern: RegExp): string | null {
    const match = output.match(pattern)
    return match ? match[1] : null
  }
}

export function extractAgentId(output: string): string | null {
  const match = output.match(/Agent Loop 已启动: ([\w-]+)/)
  return match ? match[1] : null
}

export function extractCheckpointId(output: string): string | null {
  const match = output.match(/检查点已创建: ([\w-]+)/)
  return match ? match[1] : null
}

export function extractThreadId(output: string): string | null {
  const match = output.match(/线程已创建: ([\w-]+)/)
  return match ? match[1] : null
}
```

### 4.3 Mock SDK 配置

```typescript
// __tests__/utils/mock-sdk.ts
import { getSDK } from '@modular-agent/sdk'

export function setupMockSDK() {
  // 使用 Mock LLM Profile
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
  })
}
```

### 4.4 测试配置文件

```javascript
// vitest.integration.config.mjs
import { defineConfig } from 'vitest/config'
import { fileURLToPath } from 'url'
import { dirname, resolve } from 'path'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)

export default defineConfig({
  test: {
    environment: 'node',
    include: ['**/__tests__/integration/**/*.test.ts'],
    exclude: ['node_modules', 'dist', 'coverage'],
    testTimeout: 60000,
    reporters: ['verbose'],
    clearMocks: true,
    restoreMocks: true,
    globals: true,
    setupFiles: ['./__tests__/setup.ts'],
    teardownTimeout: 10000,
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, '.'),
      '@modular-agent/sdk': resolve(__dirname, '../../sdk'),
      '@modular-agent/types': resolve(__dirname, '../../packages/types/src'),
      '@modular-agent/common-utils': resolve(__dirname, '../../packages/common-utils/src'),
      '@modular-agent/tool-executors': resolve(__dirname, '../../packages/tool-executors/src'),
    },
  },
})
```

### 4.5 测试设置文件

```typescript
// __tests__/setup.ts
import { beforeAll, afterAll } from 'vitest'
import { getSDK } from '@modular-agent/sdk'

beforeAll(async () => {
  // 初始化测试环境
  console.log('Setting up integration test environment...')

  // 初始化 SDK
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
  })
})

afterAll(async () => {
  // 清理测试环境
  console.log('Cleaning up integration test environment...')
})
```

## 5. 测试用例示例

### 5.1 Workflow 生命周期测试

```typescript
// __tests__/integration/workflow/workflow-lifecycle.test.ts
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { runCLI } from '../../utils/cli-runner'
import { TestHelper, extractAgentId } from '../../utils/test-helpers'

describe('Workflow Lifecycle Integration Tests', () => {
  let helper: TestHelper

  beforeEach(() => {
    helper = new TestHelper('workflow-lifecycle')
  })

  afterEach(async () => {
    await helper.cleanup()
  })

  it('should register a workflow from file', async () => {
    const workflowFile = helper.getFixturePath('workflows', 'simple-workflow.toml')
    const result = await runCLI(['workflow', 'register', workflowFile])

    expect(result.exitCode).toBe(0)
    expect(result.stdout).toContain('工作流已注册')
    expect(result.stderr).toBe('')
  })

  it('should list all registered workflows', async () => {
    const workflowFile = helper.getFixturePath('workflows', 'simple-workflow.toml')
    await runCLI(['workflow', 'register', workflowFile])

    const result = await runCLI(['workflow', 'list', '-t'])

    expect(result.exitCode).toBe(0)
    expect(result.stdout).toContain('ID')
    expect(result.stdout).toContain('名称')
  })

  it('should show workflow details', async () => {
    const workflowFile = helper.getFixturePath('workflows', 'simple-workflow.toml')
    await runCLI(['workflow', 'register', workflowFile])

    const listResult = await runCLI(['workflow', 'list'])
    const workflowId = helper.extractId(listResult.stdout, /simple-workflow/)

    if (workflowId) {
      const result = await runCLI(['workflow', 'show', workflowId])

      expect(result.exitCode).toBe(0)
      expect(result.stdout).toContain(workflowId)
    }
  })

  it('should delete a workflow', async () => {
    const workflowFile = helper.getFixturePath('workflows', 'simple-workflow.toml')
    await runCLI(['workflow', 'register', workflowFile])

    const listResult = await runCLI(['workflow', 'list'])
    const workflowId = helper.extractId(listResult.stdout, /simple-workflow/)

    if (workflowId) {
      const result = await runCLI(['workflow', 'delete', workflowId, '-f'])

      expect(result.exitCode).toBe(0)
      expect(result.stdout).toContain('工作流已删除')
    }
  })
})
```

### 5.2 Agent Loop 执行测试

```typescript
// __tests__/integration/agent/agent-loop-execution.test.ts
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { runCLI } from '../../utils/cli-runner'
import { TestHelper } from '../../utils/test-helpers'

describe('Agent Loop Execution Integration Tests', () => {
  let helper: TestHelper

  beforeEach(() => {
    helper = new TestHelper('agent-loop-execution')
  })

  afterEach(async () => {
    await helper.cleanup()
  })

  it('should execute agent loop synchronously', async () => {
    const result = await runCLI([
      'agent', 'run',
      '-p', 'DEFAULT',
      '-s', 'You are a helpful assistant',
      '-i', '{"message": "Hello"}',
    ])

    expect(result.exitCode).toBe(0)
    expect(result.stdout).toContain('Agent Loop 执行完成')
  })

  it('should execute agent loop with streaming', async () => {
    const result = await runCLI([
      'agent', 'run',
      '--stream',
      '-p', 'DEFAULT',
      '-s', 'You are a helpful assistant',
    ])

    expect(result.exitCode).toBe(0)
    expect(result.stdout).toContain('[迭代')
  })

  it('should start agent loop asynchronously', async () => {
    const result = await runCLI([
      'agent', 'start',
      '-p', 'DEFAULT',
      '-s', 'You are a helpful assistant',
    ])

    expect(result.exitCode).toBe(0)
    expect(result.stdout).toContain('Agent Loop 已启动')

    const agentId = helper.extractAgentId(result.stdout)
    expect(agentId).toBeTruthy()
  })

  it('should pause and resume agent loop', async () => {
    // Start agent loop
    const startResult = await runCLI([
      'agent', 'start',
      '-p', 'DEFAULT',
      '-s', 'You are a helpful assistant',
      '-m', '100', // Use high max iterations to allow pause
    ])

    expect(startResult.exitCode).toBe(0)
    const agentId = helper.extractAgentId(startResult.stdout)
    expect(agentId).toBeTruthy()

    // Pause agent loop
    const pauseResult = await runCLI(['agent', 'pause', agentId!])
    expect(pauseResult.exitCode).toBe(0)
    expect(pauseResult.stdout).toContain('Agent Loop 已暂停')

    // Resume agent loop
    const resumeResult = await runCLI(['agent', 'resume', agentId!])
    expect(resumeResult.exitCode).toBe(0)
    expect(resumeResult.stdout).toContain('Agent Loop 已恢复')
  })

  it('should create and restore from checkpoint', async () => {
    // Start agent loop
    const startResult = await runCLI([
      'agent', 'start',
      '-p', 'DEFAULT',
      '-s', 'You are a helpful assistant',
      '-m', '100',
    ])

    expect(startResult.exitCode).toBe(0)
    const agentId = helper.extractAgentId(startResult.stdout)
    expect(agentId).toBeTruthy()

    // Create checkpoint
    const checkpointResult = await runCLI([
      'agent', 'checkpoint', agentId!,
      '-n', 'test-checkpoint',
    ])

    expect(checkpointResult.exitCode).toBe(0)
    expect(checkpointResult.stdout).toContain('检查点已创建')

    const checkpointId = helper.extractCheckpointId(checkpointResult.stdout)
    expect(checkpointId).toBeTruthy()

    // Restore from checkpoint
    const restoreResult = await runCLI(['agent', 'restore', checkpointId!])
    expect(restoreResult.exitCode).toBe(0)
    expect(restoreResult.stdout).toContain('Agent Loop 已从检查点恢复')
  })
})
```

### 5.3 端到端测试

```typescript
// __tests__/integration/e2e/complete-workflow.test.ts
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { runCLI } from '../../utils/cli-runner'
import { TestHelper } from '../../utils/test-helpers'

describe('Complete Workflow E2E Integration Tests', () => {
  let helper: TestHelper

  beforeEach(() => {
    helper = new TestHelper('complete-workflow-e2e')
  })

  afterEach(async () => {
    await helper.cleanup()
  })

  it('should execute complete workflow lifecycle', async () => {
    // 1. Register workflow
    const workflowFile = helper.getFixturePath('workflows', 'simple-workflow.toml')
    const registerResult = await runCLI(['workflow', 'register', workflowFile])
    expect(registerResult.exitCode).toBe(0)

    // 2. List workflows
    const listResult = await runCLI(['workflow', 'list'])
    expect(listResult.exitCode).toBe(0)

    // 3. Execute workflow
    const workflowId = helper.extractId(listResult.stdout, /simple-workflow/)
    if (workflowId) {
      const executeResult = await runCLI([
        'thread', 'execute', workflowId,
        '-p', '{"input": "test"}',
      ])
      expect(executeResult.exitCode).toBe(0)

      const threadId = helper.extractThreadId(executeResult.stdout)
      expect(threadId).toBeTruthy()

      // 4. Check thread status
      const statusResult = await runCLI(['thread', 'status', threadId!])
      expect(statusResult.exitCode).toBe(0)

      // 5. View messages
      const messagesResult = await runCLI(['message', 'list', threadId!])
      expect(messagesResult.exitCode).toBe(0)

      // 6. Cleanup
      await runCLI(['workflow', 'delete', workflowId, '-f'])
    }
  })
})
```

## 6. 测试执行和验证

### 6.1 运行测试

```bash
# 运行所有集成测试
cd apps/cli-app
pnpm test:integration

# 运行特定模块的测试
pnpm test:integration workflow
pnpm test:integration agent
pnpm test:integration e2e

# 运行特定测试文件
pnpm vitest run __tests__/integration/workflow/workflow-lifecycle.test.ts

# 使用详细输出
pnpm test:integration --reporter=verbose

# 生成覆盖率报告
pnpm test:integration --coverage
```

### 6.2 验证策略

1. **退出码验证**：所有成功的 CLI 命令应该返回退出码 0
2. **输出验证**：验证 stdout 包含预期的内容
3. **错误验证**：验证 stderr 为空或包含预期的错误信息
4. **状态验证**：验证资源状态符合预期
5. **数据验证**：验证返回的数据结构和内容正确
6. **集成验证**：验证模块之间的协作正常

### 6.3 测试报告

测试执行后生成以下报告：

- **控制台输出**：实时显示测试进度和结果
- **覆盖率报告**：显示代码覆盖率统计
- **HTML 报告**：详细的测试结果可视化
- **JSON 报告**：机器可读的测试结果

## 7. 持续集成

### 7.1 CI/CD 配置

```yaml
# .github/workflows/integration-tests.yml
name: Integration Tests

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main, develop]

jobs:
  integration-tests:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v3

      - name: Setup Node.js
        uses: actions/setup-node@v3
        with:
          node-version: '22'

      - name: Install pnpm
        uses: pnpm/action-setup@v2
        with:
          version: 10.28.2

      - name: Install dependencies
        run: pnpm install

      - name: Build packages
        run: pnpm build

      - name: Run integration tests
        run: |
          cd apps/cli-app
          pnpm test:integration

      - name: Upload coverage reports
        uses: codecov/codecov-action@v3
        with:
          files: ./apps/cli-app/coverage/lcov.info
```

## 8. 最佳实践

1. **测试隔离**：每个测试用例独立运行，避免相互影响
2. **资源清理**：测试完成后自动清理临时数据和资源
3. **超时控制**：为每个测试设置合理的超时时间
4. **错误处理**：正确处理和验证错误场景
5. **Mock 数据**：使用 Mock 数据避免依赖外部服务
6. **测试命名**：使用描述性的测试名称
7. **文档更新**：及时更新测试文档
8. **定期运行**：定期运行集成测试，及时发现集成问题

## 9. 未来改进

1. **性能测试**：添加性能测试，评估 CLI 命令的执行时间
2. **压力测试**：测试大量并发请求下的系统稳定性
3. **可视化测试**：添加 UI 相关的集成测试
4. **自动化测试数据生成**：自动生成测试数据，提高测试效率
5. **测试结果分析**：添加测试结果分析和趋势分析