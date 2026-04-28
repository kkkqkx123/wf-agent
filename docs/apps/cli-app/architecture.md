# CLI 应用架构设计

## 整体架构

CLI 应用将遵循单一职责原则，作为现有 SDK API 层的命令行前端。整体架构分为以下几个层次：

```
┌─────────────────┐
│   CLI Layer     │  ← Commander.js 命令定义和参数解析
├─────────────────┤
│  Adapter Layer  │  ← 将 CLI 参数转换为 SDK API 调用
├─────────────────┤
│   SDK Layer     │  ← 现有的 API、Core、Types、Utils 层
└─────────────────┘
```

## 目录结构

```
apps/cli-app/
├── src/
│   ├── commands/           # 各个命令实现
│   │   ├── workflow/       # 工作流相关命令
│   │   ├── thread/         # 线程相关命令
│   │   ├── checkpoint/     # 检查点相关命令
│   │   └── template/       # 模板相关命令
│   ├── adapters/           # CLI 适配器层
│   │   ├── base-adapter.ts       # 基础适配器类
│   │   ├── workflow-adapter.ts
│   │   ├── thread-adapter.ts
│   │   ├── checkpoint-adapter.ts
│   │   └── template-adapter.ts
│   ├── utils/              # CLI 专用工具函数
│   │   ├── logger.ts       # CLI 日志工具
│   │   ├── validator.ts    # 输入验证工具（可选）
│   │   └── formatter.ts    # 输出格式化工具
│   ├── types/              # CLI 专用类型定义
│   │   └── cli-types.ts
│   ├── config/             # 配置管理
│   │   ├── config-loader.ts
│   │   └── config-manager.ts
│   └── index.ts            # CLI 入口文件
├── scripts/
│   └── modular-agent.js    # 可执行脚本入口
├── package.json
├── tsconfig.json
└── README.md
```

**注意**: 可执行文件位于 `scripts/` 目录而非 `bin/` 目录，`package.json` 中的 bin 配置应指向 `./scripts/modular-agent.js`。

## 命令实现模式

每个命令都将遵循相同的实现模式：

```typescript
// 示例：src/commands/workflow/index.ts
import { Command } from 'commander';
import { WorkflowAdapter } from '../../adapters/workflow-adapter.js';
import { createLogger } from '../../utils/logger.js';
import { formatWorkflow } from '../../utils/formatter.js';
import type { CommandOptions } from '../../types/cli-types.js';

const logger = createLogger();

export function createWorkflowCommands(): Command {
  const workflowCmd = new Command('workflow')
    .description('管理工作流')
    .alias('wf');

  // 注册工作流命令
  workflowCmd
    .command('register <file>')
    .description('从文件注册工作流')
    .option('-v, --verbose', '详细输出')
    .action(async (file, options: CommandOptions) => {
      try {
        logger.info(`正在注册工作流: ${file}`);
        
        const adapter = new WorkflowAdapter();
        const workflow = await adapter.registerFromFile(file);

        console.log(formatWorkflow(workflow, { verbose: options.verbose }));
      } catch (error) {
        logger.error(`注册失败: ${error instanceof Error ? error.message : String(error)}`);
        process.exit(1);
      }
    });

  return workflowCmd;
}
```

## 适配器层设计

适配器层统一继承 `BaseAdapter`，负责将 CLI 参数转换为 SDK API 调用：

```typescript
// src/adapters/base-adapter.ts
import { getSDK } from '@modular-agent/sdk';
import { createLogger } from '../utils/logger.js';

export class BaseAdapter {
  protected logger: ReturnType<typeof createLogger>;
  protected sdk: ReturnType<typeof getSDK>;

  constructor() {
    this.logger = createLogger();
    this.sdk = getSDK();
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

  protected handleError(error: unknown, context: string): never {
    const message = error instanceof Error ? error.message : String(error);
    this.logger.error(`${context}: ${message}`);
    throw error;
  }
}
```

```typescript
// src/adapters/workflow-adapter.ts
import { BaseAdapter } from './base-adapter.js';
import { ConfigManager, type ConfigLoadOptions } from '../config/config-manager.js';
import { resolve } from 'path';

export class WorkflowAdapter extends BaseAdapter {
  private configManager: ConfigManager;

  constructor(configManager?: ConfigManager) {
    super();
    this.configManager = configManager || new ConfigManager();
  }

  async registerFromFile(
    filePath: string,
    parameters?: Record<string, any>
  ): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      // 使用 ConfigManager 加载配置
      const fullPath = resolve(process.cwd(), filePath);
      const workflow = await this.configManager.loadWorkflow(fullPath, parameters);
      
      // 使用继承的 sdk 实例
      const api = this.sdk.workflows;
      await api.create(workflow);
      
      this.logger.success(`工作流已注册: ${workflow.id}`);
      return workflow;
    }, '注册工作流');
  }

  // 其他方法...
}
```

**重要**: 所有适配器都应继承 `BaseAdapter`，使用父类提供的 `sdk` 实例，避免重复导入 `getSDK`。

## 错误处理策略

CLI 应用通过 `BaseAdapter` 实现统一的错误处理策略：

```typescript
// src/adapters/base-adapter.ts
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

protected handleError(error: unknown, context: string): never {
  const message = error instanceof Error ? error.message : String(error);
  this.logger.error(`${context}: ${message}`);

  if (error instanceof Error && error.stack) {
    this.logger.debug(error.stack);
  }

  throw error;
}
```

在命令中使用：

```typescript
// 适配器内部自动处理错误
async someMethod(): Promise<void> {
  return this.executeWithErrorHandling(async () => {
    // 执行操作
    await this.sdk.someApi.call();
  }, '操作上下文');
}
```

## 配置管理

CLI 应用支持多种配置来源：

```typescript
// src/config/config-loader.ts
import { cosmiconfig } from 'cosmiconfig';
import { z } from 'zod';

const ConfigSchema = z.object({
  apiUrl: z.string().url().optional(),
  apiKey: z.string().optional(),
  defaultTimeout: z.number().positive().optional(),
  verbose: z.boolean().optional(),
  debug: z.boolean().optional(),
  logLevel: z.enum(['error', 'warn', 'info', 'debug']).optional(),
  outputFormat: z.enum(['json', 'table', 'plain']).optional(),
  maxConcurrentThreads: z.number().positive().optional(),
});

export type CLIConfig = z.infer<typeof ConfigSchema>;

export class ConfigLoader {
  private explorer = cosmiconfig('modular-agent', {
    searchPlaces: [
      'package.json',
      '.modular-agentrc',
      '.modular-agentrc.json',
      '.modular-agentrc.ts',
      '.modular-agentrc.js',
      'modular-agent.config.js',
      'modular-agent.config.ts',
    ],
  });

  async load(): Promise<CLIConfig> {
    const result = await this.explorer.search();
    
    if (result?.config) {
      return ConfigSchema.parse(result.config);
    }
    
    return {}; // 返回默认配置
  }
}
```

## 输出格式化

CLI 应用支持多种输出格式：

```typescript
// src/utils/formatter.ts
import chalk from 'chalk';

export function formatWorkflow(workflow: any, options: { verbose?: boolean } = {}) {
  if (options.verbose) {
    return JSON.stringify(workflow, null, 2);
  } else {
    return `${chalk.blue(workflow.name)} (${workflow.id}) - ${formatStatus(workflow.status)}`;
  }
}

function formatStatus(status: string): string {
  switch (status?.toLowerCase()) {
    case 'running':
    case 'active':
      return chalk.green(status);
    case 'paused':
    case 'suspended':
      return chalk.yellow(status);
    case 'stopped':
    case 'failed':
      return chalk.red(status);
    case 'completed':
    case 'success':
      return chalk.green.bold(status);
    default:
      return chalk.gray(status || 'unknown');
  }
}
```

## 测试策略

CLI 应用将采用分层测试策略：

1. **单元测试**：测试适配器层和工具函数
2. **集成测试**：测试命令与适配器的集成
3. **端到端测试**：测试完整命令流程

```typescript
// 示例测试
import { describe, it, expect, vi } from 'vitest';
import { WorkflowAdapter } from '../src/adapters/workflow-adapter.js';

describe('WorkflowAdapter', () => {
  it('should register workflow from file', async () => {
    const adapter = new WorkflowAdapter();
    
    // 模拟 SDK 和文件系统
    vi.mock('@modular-agent/sdk', () => ({
      getSDK: () => ({
        workflows: {
          create: vi.fn().mockResolvedValue({ id: 'test-workflow' })
        }
      })
    }));

    const result = await adapter.registerFromFile('./test.toml');
    
    expect(result).toBeDefined();
    expect(result.id).toBe('test-workflow');
  });
});
```

## 扩展性考虑

架构设计考虑了未来的扩展需求：

1. **插件系统**：预留插件接口，允许第三方扩展命令
2. **国际化**：支持多语言输出
3. **主题定制**：支持输出样式的自定义
4. **API 版本管理**：兼容不同版本的 SDK API

这种架构确保了 CLI 应用既能够充分利用现有 SDK 功能，又具有良好的可维护性和扩展性。
