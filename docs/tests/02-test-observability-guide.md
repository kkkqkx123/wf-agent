# 测试结果可观测性方案

## 1. 概述

本文档说明如何让 CLI-App 集成测试的结果可观测，包括输出重定向、日志记录、结果可视化等方面，方便人工验证和问题排查。

## 2. 可观测性需求

### 2.1 核心需求

1. **完整输出记录**：记录每次测试的完整 CLI 输出（stdout + stderr）
2. **结构化日志**：以结构化方式记录测试执行过程
3. **结果汇总**：提供测试结果汇总和统计
4. **失败分析**：详细记录失败测试的上下文信息
5. **历史对比**：支持历史测试结果对比

### 2.2 使用场景

- 测试执行后人工验证输出格式
- 调试失败的测试用例
- 回顾历史测试结果
- 分析测试趋势和性能
- 生成测试报告

## 3. 输出重定向方案

### 3.1 方案设计

**目标**：将每次 CLI 命令的完整输出保存到文件中，方便人工查看

**实现方式**：
1. 在 `CLIRunner` 中增加输出重定向功能
2. 为每次测试执行创建独立的输出文件
3. 使用规范的文件命名和目录结构

### 3.2 目录结构

```
apps/cli-app/
├── __tests__/
│   ├── integration/
│   │   ├── workflow/
│   │   │   └── workflow-lifecycle.test.ts
│   │   └── ...
│   └── outputs/                    # 测试输出目录
│       ├── 2024-01-01_120000/      # 按时间戳分组
│       │   ├── workflow/
│       │   │   ├── 001_register.log
│       │   │   ├── 002_list.log
│       │   │   └── 003_show.log
│       │   ├── agent/
│       │   │   ├── 001_run.log
│       │   │   └── 002_start.log
│       │   └── summary.json        # 测试结果汇总
│       ├── 2024-01-01_130000/
│       │   └── ...
│       └── latest/                 # 最新测试结果的符号链接
│           └── -> 2024-01-01_130000/
```

### 3.3 CLI Runner 增强

**扩展 CLIRunResult 接口**：
```typescript
interface CLIRunResult {
  exitCode: number | null;
  stdout: string;
  stderr: string;
  duration: number;
  outputFilePath?: string;  // 输出文件路径
}
```

**增强 CLIRunner 类**：
```typescript
export class CLIRunner {
  private outputDir: string;
  private outputFileCounter: number;

  constructor(cliPath?: string, outputDir?: string) {
    this.cliPath = cliPath || this.findCLIPath();
    this.outputDir = outputDir || './__tests__/outputs';
    this.outputFileCounter = 0;
  }

  async run(args: string[], options: {
    timeout?: number;
    input?: string;
    cwd?: string;
    env?: Record<string, string>;
    saveOutput?: boolean;        // 是否保存输出到文件
    outputSubdir?: string;       // 输出子目录
  } = {}): Promise<CLIRunResult> {
    const {
      saveOutput = true,
      outputSubdir = 'general',
      ...otherOptions
    } = options;

    const result = await this.executeCommand(args, otherOptions);

    // 保存输出到文件
    if (saveOutput) {
      const outputFilePath = await this.saveOutput(
        result,
        args,
        outputSubdir
      );
      result.outputFilePath = outputFilePath;
    }

    return result;
  }

  private async saveOutput(
    result: any,
    args: string[],
    subdir: string
  ): Promise<string> {
    // 创建输出目录
    const timestamp = this.getTimestamp();
    const outputDir = join(
      this.outputDir,
      timestamp,
      subdir
    );
    mkdirSync(outputDir, { recursive: true });

    // 生成文件名
    this.outputFileCounter++;
    const filename = `${String(this.outputFileCounter).padStart(3, '0')}_${args.join('_')}.log`;
    const filepath = join(outputDir, filename);

    // 写入输出文件
    const content = this.formatOutput(result, args);
    writeFileSync(filepath, content, 'utf-8');

    return filepath;
  }

  private formatOutput(result: any, args: string[]): string {
    const lines: string[] = [];

    // 标题
    lines.push('='.repeat(80));
    lines.push(`Command: modular-agent ${args.join(' ')}`);
    lines.push('='.repeat(80));
    lines.push('');

    // 元数据
    lines.push('Metadata:');
    lines.push(`  Exit Code: ${result.exitCode}`);
    lines.push(`  Duration: ${result.duration}ms`);
    lines.push(`  Timestamp: ${new Date().toISOString()}`);
    lines.push('');

    // 标准输出
    lines.push('STDOUT:');
    lines.push('-'.repeat(80));
    lines.push(result.stdout || '(empty)');
    lines.push('');

    // 标准错误
    lines.push('STDERR:');
    lines.push('-'.repeat(80));
    lines.push(result.stderr || '(empty)');
    lines.push('');

    return lines.join('\n');
  }

  private getTimestamp(): string {
    const now = new Date();
    const year = now.getFullYear();
    const month = String(now.getMonth() + 1).padStart(2, '0');
    const day = String(now.getDate()).padStart(2, '0');
    const hour = String(now.getHours()).padStart(2, '0');
    const minute = String(now.getMinutes()).padStart(2, '0');
    const second = String(now.getSeconds()).padStart(2, '0');

    return `${year}-${month}-${day}_${hour}${minute}${second}`;
  }
}
```

### 3.4 使用示例

```typescript
const runner = new CLIRunner();

// 执行命令并保存输出
const result = await runner.run(['workflow', 'list', '-t'], {
  outputSubdir: 'workflow'
});

console.log(`Output saved to: ${result.outputFilePath}`);
```

**输出文件示例**：
```
================================================================================
Command: modular-agent workflow list -t
================================================================================

Metadata:
  Exit Code: 0
  Duration: 1234ms
  Timestamp: 2024-01-01T12:00:00.000Z

STDOUT:
--------------------------------------------------------------------------------
ID          名称              描述                    状态
-------------------------------------------------------------
wf-001      simple-workflow   简单工作流示例           active
wf-002      complex-workflow  复杂工作流示例           active

STDERR:
--------------------------------------------------------------------------------
(empty)
```

## 4. 结构化日志

### 4.1 日志格式

使用 JSON 格式记录结构化日志，便于机器解析和分析。

**日志结构**：
```typescript
interface TestLog {
  timestamp: string;
  testSuite: string;
  testName: string;
  status: 'passed' | 'failed' | 'skipped';
  duration: number;
  commands: CommandLog[];
  error?: {
    message: string;
    stack?: string;
  };
  outputFiles: string[];
}

interface CommandLog {
  command: string[];
  exitCode: number;
  duration: number;
  stdoutPreview: string;
  stderrPreview: string;
  outputFile: string;
}
```

### 4.2 日志记录器实现

```typescript
export class TestLogger {
  private logs: TestLog[] = [];
  private currentTest: TestLog | null = null;
  private logFilePath: string;

  constructor(outputDir: string) {
    const timestamp = this.getTimestamp();
    this.logFilePath = join(outputDir, timestamp, 'test-logs.jsonl');
    mkdirSync(dirname(this.logFilePath), { recursive: true });
  }

  startTest(testSuite: string, testName: string): void {
    this.currentTest = {
      timestamp: new Date().toISOString(),
      testSuite,
      testName,
      status: 'passed',
      duration: 0,
      commands: [],
      outputFiles: [],
    };
  }

  recordCommand(command: string[], result: CLIRunResult): void {
    if (!this.currentTest) return;

    this.currentTest.commands.push({
      command,
      exitCode: result.exitCode!,
      duration: result.duration,
      stdoutPreview: result.stdout.substring(0, 500),
      stderrPreview: result.stderr.substring(0, 500),
      outputFile: result.outputFilePath || '',
    });

    if (result.outputFilePath) {
      this.currentTest.outputFiles.push(result.outputFilePath);
    }
  }

  endTest(status: 'passed' | 'failed' | 'skipped', error?: Error): void {
    if (!this.currentTest) return;

    this.currentTest.status = status;
    this.currentTest.duration = Date.now() - new Date(this.currentTest.timestamp).getTime();

    if (error) {
      this.currentTest.error = {
        message: error.message,
        stack: error.stack,
      };
    }

    // 写入日志文件
    this.appendLog(this.currentTest);

    // 添加到内存日志
    this.logs.push(this.currentTest);

    // 重置当前测试
    this.currentTest = null;
  }

  private appendLog(log: TestLog): void {
    const line = JSON.stringify(log) + '\n';
    appendFileSync(this.logFilePath, line, 'utf-8');
  }

  private getTimestamp(): string {
    const now = new Date();
    const year = now.getFullYear();
    const month = String(now.getMonth() + 1).padStart(2, '0');
    const day = String(now.getDate()).padStart(2, '0');
    const hour = String(now.getHours()).padStart(2, '0');
    const minute = String(now.getMinutes()).padStart(2, '0');
    const second = String(now.getSeconds()).padStart(2, '0');

    return `${year}-${month}-${day}_${hour}${minute}${second}`;
  }

  getLogs(): TestLog[] {
    return this.logs;
  }

  getSummary(): TestSummary {
    const total = this.logs.length;
    const passed = this.logs.filter(l => l.status === 'passed').length;
    const failed = this.logs.filter(l => l.status === 'failed').length;
    const skipped = this.logs.filter(l => l.status === 'skipped').length;

    return {
      total,
      passed,
      failed,
      skipped,
      duration: this.logs.reduce((sum, log) => sum + log.duration, 0),
      timestamp: new Date().toISOString(),
    };
  }
}

interface TestSummary {
  total: number;
  passed: number;
  failed: number;
  skipped: number;
  duration: number;
  timestamp: string;
}
```

### 4.3 在测试中使用

```typescript
import { beforeAll, afterAll, beforeEach, afterEach } from 'vitest';

describe('Workflow Tests', () => {
  let logger: TestLogger;

  beforeAll(() => {
    logger = new TestLogger('./__tests__/outputs');
  });

  afterAll(() => {
    // 生成汇总报告
    const summary = logger.getSummary();
    console.log('Test Summary:', summary);
  });

  beforeEach(() => {
    logger.startTest('Workflow', 'Register workflow');
  });

  afterEach((context) => {
    const status = context.task.result?.state || 'passed';
    const error = context.task.result?.error;
    logger.endTest(status, error);
  });

  it('should register a workflow', async () => {
    const result = await runCLI(['workflow', 'register', 'test.toml']);
    logger.recordCommand(['workflow', 'register', 'test.toml'], result);
    expect(result.exitCode).toBe(0);
  });
});
```

## 5. 结果汇总和可视化

### 5.1 汇总报告生成

生成 HTML 格式的汇总报告，方便查看测试结果。

**报告模板**：
```html
<!DOCTYPE html>
<html>
<head>
  <title>Test Report</title>
  <style>
    body { font-family: Arial, sans-serif; padding: 20px; }
    .summary { background: #f5f5f5; padding: 15px; border-radius: 5px; }
    .passed { color: green; }
    .failed { color: red; }
    .skipped { color: orange; }
    table { border-collapse: collapse; width: 100%; }
    th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
    th { background-color: #4CAF50; color: white; }
  </style>
</head>
<body>
  <h1>Integration Test Report</h1>
  <div class="summary">
    <h2>Summary</h2>
    <p>Total: {{total}}</p>
    <p>Passed: <span class="passed">{{passed}}</span></p>
    <p>Failed: <span class="failed">{{failed}}</span></p>
    <p>Skipped: <span class="skipped">{{skipped}}</span></p>
    <p>Duration: {{duration}}ms</p>
  </div>

  <h2>Test Results</h2>
  <table>
    <tr>
      <th>Test Suite</th>
      <th>Test Name</th>
      <th>Status</th>
      <th>Duration</th>
      <th>Commands</th>
      <th>Output Files</th>
    </tr>
    {{#each tests}}
    <tr>
      <td>{{testSuite}}</td>
      <td>{{testName}}</td>
      <td class="{{status}}">{{status}}</td>
      <td>{{duration}}ms</td>
      <td>{{commands.length}}</td>
      <td>
        {{#each outputFiles}}
        <a href="{{this}}">{{this}}</a><br>
        {{/each}}
      </td>
    </tr>
    {{/each}}
  </table>
</body>
</html>
```

**生成报告**：
```typescript
export function generateHTMLReport(logger: TestLogger, outputPath: string): void {
  const summary = logger.getSummary();
  const logs = logger.getLogs();

  const template = readFileSync('./report-template.html', 'utf-8');
  const html = template
    .replace('{{total}}', String(summary.total))
    .replace('{{passed}}', String(summary.passed))
    .replace('{{failed}}', String(summary.failed))
    .replace('{{skipped}}', String(summary.skipped))
    .replace('{{duration}}', String(summary.duration));

  writeFileSync(outputPath, html, 'utf-8');
}
```

### 5.2 命令行输出增强

增强 Vitest 的命令行输出，显示输出文件路径。

**自定义 Reporter**：
```typescript
import type { Reporter } from 'vitest';
import { relative } from 'path';

export class OutputFileReporter implements Reporter {
  onFinished(files?: any) {
    console.log('\n' + '='.repeat(80));
    console.log('Test Output Files:');
    console.log('='.repeat(80));

    files?.forEach((file: any) => {
      file.tasks?.forEach((task: any) => {
        if (task.logs) {
          task.logs.forEach((log: any) => {
            if (log.outputFilePath) {
              const relativePath = relative(process.cwd(), log.outputFilePath);
              console.log(`  ${relativePath}`);
            }
          });
        }
      });
    });

    console.log('='.repeat(80));
  }
}
```

**配置 Reporter**：
```javascript
export default defineConfig({
  test: {
    reporters: [
      'verbose',
      new OutputFileReporter()
    ],
  },
});
```

## 6. 失败测试分析

### 6.1 失败详情记录

为失败的测试记录详细的上下文信息。

**失败详情格式**：
```typescript
interface FailureDetail {
  testName: string;
  errorMessage: string;
  errorStack: string;
  commands: Array<{
    command: string[];
    exitCode: number;
    stdout: string;
    stderr: string;
    outputFile: string;
  }>;
  environment: {
    nodeVersion: string;
    platform: string;
    timestamp: string;
  };
}
```

**记录失败详情**：
```typescript
export class FailureAnalyzer {
  static recordFailure(
    testName: string,
    error: Error,
    commands: CommandLog[]
  ): void {
    const detail: FailureDetail = {
      testName,
      errorMessage: error.message,
      errorStack: error.stack || '',
      commands: commands.map(cmd => ({
        command: cmd.command,
        exitCode: cmd.exitCode,
        stdout: cmd.stdoutPreview,
        stderr: cmd.stderrPreview,
        outputFile: cmd.outputFile,
      })),
      environment: {
        nodeVersion: process.version,
        platform: process.platform,
        timestamp: new Date().toISOString(),
      },
    };

    const outputDir = './__tests__/outputs/failures';
    mkdirSync(outputDir, { recursive: true });

    const filename = `${testName.replace(/\s+/g, '_')}_${Date.now()}.json`;
    const filepath = join(outputDir, filename);

    writeFileSync(filepath, JSON.stringify(detail, null, 2), 'utf-8');
  }
}
```

### 6.2 失败重试机制

对于不稳定的测试，可以实现自动重试机制。

```typescript
export async function retryCommand(
  fn: () => Promise<CLIRunResult>,
  maxRetries: number = 3
): Promise<CLIRunResult> {
  let lastError: Error | null = null;

  for (let i = 0; i < maxRetries; i++) {
    try {
      const result = await fn();
      if (result.exitCode === 0) {
        return result;
      }
    } catch (error) {
      lastError = error as Error;
    }

    if (i < maxRetries - 1) {
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
  }

  throw lastError || new Error('Max retries exceeded');
}

// 使用示例
it('should register workflow with retry', async () => {
  const result = await retryCommand(() =>
    runCLI(['workflow', 'register', 'test.toml'])
  );
  expect(result.exitCode).toBe(0);
});
```

## 7. 历史对比

### 7.1 结果对比工具

对比不同测试运行的结果，发现趋势和异常。

```typescript
export class TestComparator {
  static compare(
    previous: TestSummary,
    current: TestSummary
  ): ComparisonResult {
    return {
      totalDiff: current.total - previous.total,
      passedDiff: current.passed - previous.passed,
      failedDiff: current.failed - previous.failed,
      skippedDiff: current.skipped - previous.skipped,
      durationDiff: current.duration - previous.duration,
      passedRateChange: this.calculateRateChange(previous, current),
    };
  }

  private static calculateRateChange(
    previous: TestSummary,
    current: TestSummary
  ): number {
    const prevRate = previous.total > 0 ? previous.passed / previous.total : 0;
    const currRate = current.total > 0 ? current.passed / current.total : 0;
    return currRate - prevRate;
  }
}

interface ComparisonResult {
  totalDiff: number;
  passedDiff: number;
  failedDiff: number;
  skippedDiff: number;
  durationDiff: number;
  passedRateChange: number;
}
```

### 7.2 历史结果存储

存储历史测试结果，用于趋势分析。

```typescript
export class HistoryStorage {
  private storagePath: string;

  constructor(storagePath: string = './__tests__/outputs/history') {
    this.storagePath = storagePath;
    mkdirSync(storagePath, { recursive: true });
  }

  save(summary: TestSummary): void {
    const timestamp = new Date().toISOString();
    const filename = `${timestamp}.json`;
    const filepath = join(this.storagePath, filename);

    writeFileSync(filepath, JSON.stringify(summary, null, 2), 'utf-8');
  }

  loadHistory(limit: number = 10): TestSummary[] {
    const files = readdirSync(this.storagePath)
      .sort()
      .slice(-limit);

    return files.map(file => {
      const filepath = join(this.storagePath, file);
      const content = readFileSync(filepath, 'utf-8');
      return JSON.parse(content) as TestSummary;
    });
  }
}
```

## 8. 配置选项

### 8.1 可观测性配置

在测试配置中添加可观测性选项。

```javascript
export default defineConfig({
  test: {
    // ... 其他配置

    // 可观测性配置
    observability: {
      outputDir: './__tests__/outputs',
      saveOutput: true,
      generateReport: true,
      reportFormat: 'html',  // html, json, markdown
      historyLimit: 10,
      retryOnFailure: false,
      maxRetries: 3,
    },
  },
});
```

### 8.2 环境变量

支持通过环境变量控制可观测性功能。

```bash
# 启用输出保存
export SAVE_TEST_OUTPUT=true

# 输出目录
export TEST_OUTPUT_DIR=./__tests__/outputs

# 生成报告
export GENERATE_TEST_REPORT=true

# 报告格式
export TEST_REPORT_FORMAT=html
```

## 9. 使用示例

### 9.1 完整的测试执行流程

```typescript
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { CLIRunner } from '../../utils/cli-runner';
import { TestLogger } from '../../utils/test-logger';
import { generateHTMLReport } from '../../utils/report-generator';

describe('Workflow Integration Tests', () => {
  let runner: CLIRunner;
  let logger: TestLogger;

  beforeAll(() => {
    runner = new CLIRunner();
    logger = new TestLogger('./__tests__/outputs');
  });

  afterAll(() => {
    // 生成汇总报告
    const summary = logger.getSummary();
    console.log('Test Summary:', summary);

    // 生成 HTML 报告
    generateHTMLReport(logger, './__tests__/outputs/report.html');
  });

  it('should register a workflow', async () => {
    logger.startTest('Workflow', 'Register workflow');

    const result = await runner.run(['workflow', 'register', 'test.toml'], {
      outputSubdir: 'workflow',
    });

    logger.recordCommand(['workflow', 'register', 'test.toml'], result);

    expect(result.exitCode).toBe(0);
    expect(result.stdout).toContain('工作流已注册');
    expect(result.stderr).toBe('');

    logger.endTest('passed');
  });
});
```

### 9.2 查看测试输出

```bash
# 查看最新的测试输出
ls -la apps/cli-app/__tests__/outputs/latest/

# 查看特定测试的输出
cat apps/cli-app/__tests__/outputs/latest/workflow/001_register.log

# 查看测试日志
cat apps/cli-app/__tests__/outputs/latest/test-logs.jsonl

# 查看失败详情
ls -la apps/cli-app/__tests__/outputs/failures/

# 查看测试报告
open apps/cli-app/__tests__/outputs/report.html
```

## 10. 最佳实践

### 10.1 输出管理

- 定期清理旧的测试输出
- 限制历史记录数量
- 使用压缩存储旧输出

### 10.2 日志分析

- 定期分析测试日志，发现趋势
- 关注失败率和执行时间变化
- 及时修复不稳定的测试

### 10.3 团队协作

- 在 CI/CD 中生成测试报告
- 将报告上传到共享位置
- 定期回顾测试结果

## 11. 总结

测试结果可观测性方案提供了：

1. **完整输出记录**：保存每次 CLI 命令的完整输出
2. **结构化日志**：JSON 格式的结构化日志
3. **结果汇总**：测试结果汇总和统计
4. **失败分析**：详细的失败测试上下文
5. **历史对比**：支持历史测试结果对比
6. **可视化报告**：HTML 格式的测试报告

通过这些功能，可以方便地验证测试结果、排查问题和分析趋势。