# CLI App 执行模式架构分析

## 概述

本文档分析 `apps/cli-app` 包当前支持的执行模式，评估现有架构的优势与不足，为后续增强无头模式提供基础。

## 当前支持的执行模式

### 1. 交互模式 (Interactive Mode)

**启用方式**：默认模式，无需特殊配置

**特点**：
- 支持彩色输出（使用 ANSI 颜色码）
- 支持用户交互（确认提示、输入等）
- 实时进度显示
- 表格、JSON 等多种输出格式

**实现位置**：
- [src/index.ts](file:///d:/项目/agent/wf-agent/apps/cli-app/src/index.ts) - 主入口
- [src/utils/output.ts](file:///d:/项目/agent/wf-agent/apps/cli-app/src/utils/output.ts) - 输出管理
- [src/utils/formatter.ts](file:///d:/项目/agent/wf-agent/apps/cli-app/src/utils/formatter.ts) - 格式化工具

### 2. 无头模式 (Headless Mode)

**启用方式**：
```bash
# 方式1：环境变量
HEADLESS=true modular-agent workflow list

# 方式2：测试模式（自动启用无头特性）
TEST_MODE=true modular-agent workflow list
```

**特点**：
- 禁用交互式功能
- 自动退出（无需手动 Ctrl+C）
- 日志重定向到文件
- 适合自动化脚本和 CI/CD

**实现位置**：
- [src/index.ts#L115-L128](file:///d:/项目/agent/wf-agent/apps/cli-app/src/index.ts#L115-L128) - 无头模式退出逻辑

```typescript
const isHeadless = process.env["HEADLESS"] === "true" || process.env["TEST_MODE"] === "true";
if (isHeadless) {
  try {
    await output.close();
    process.exit(0);
  } catch (error) {
    process.exit(1);
  }
}
```

### 3. 后台模式 (Background Mode)

**启用方式**：通过 TerminalOptions 配置

```typescript
const terminal = terminalManager.createTerminal({
  background: true,  // 启用后台模式
  logFile: "logs/task.log"
});
```

**特点**：
- 使用 `child_process.spawn` 创建分离进程
- 支持日志文件重定向
- 进程独立运行，不受主进程影响
- 适合长时间运行的任务

**实现位置**：
- [src/terminal/terminal-manager.ts#L62-L130](file:///d:/项目/agent/wf-agent/apps/cli-app/src/terminal/terminal-manager.ts#L62-L130)

## 核心架构组件

### 1. 输出管理系统 (CLIOutput)

**文件**：[src/utils/output.ts](file:///d:/项目/agent/wf-agent/apps/cli-app/src/utils/output.ts)

**职责**：
- 统一管理 stdout、stderr、log file 三个输出流
- 支持彩色输出控制
- 提供结构化输出方法（JSON、表格、列表等）

**架构**：
```
┌─────────────────────────────────────────┐
│              CLIOutput                  │
├─────────────┬─────────────┬─────────────┤
│   stdout    │   stderr    │  log file   │
│   用户可见   │   错误信息   │   调试日志   │
│   可重定向   │   可重定向   │   持久化存储  │
└─────────────┴─────────────┴─────────────┘
```

**主要方法**：
- `output()` / `write()` / `stream()` - 标准输出
- `success()` / `info()` / `warn()` - 带图标的消息
- `error()` / `fail()` - 错误输出
- `log()` / `debugLog()` / `verboseLog()` - 日志记录
- `json()` / `table()` / `keyValue()` - 格式化输出

### 2. 终端管理器 (TerminalManager)

**文件**：[src/terminal/terminal-manager.ts](file:///d:/项目/agent/wf-agent/apps/cli-app/src/terminal/terminal-manager.ts)

**职责**：
- 创建和管理伪终端会话
- 支持前台/后台两种模式
- 事件监听和广播

**类型定义**：[src/terminal/types.ts](file:///d:/项目/agent/wf-agent/apps/cli-app/src/terminal/types.ts)

```typescript
export interface TerminalOptions {
  shell?: string;           // Shell 路径
  cwd?: string;             // 工作目录
  env?: Record<string, string>;  // 环境变量
  cols?: number;            // 终端列数
  rows?: number;            // 终端行数
  background?: boolean;     // 是否后台运行
  logFile?: string;         // 日志文件路径
}
```

### 3. 通信桥接 (CommunicationBridge)

**文件**：[src/terminal/communication-bridge.ts](file:///d:/项目/agent/wf-agent/apps/cli-app/src/terminal/communication-bridge.ts)

**职责**：
- 基于 RxJS 实现消息队列
- 支持主进程与终端进程间通信
- 消息广播和定向发送

### 4. 任务执行器 (TaskExecutor)

**文件**：[src/terminal/task-executor.ts](file:///d:/项目/agent/wf-agent/apps/cli-app/src/terminal/task-executor.ts)

**职责**：
- 在独立终端中执行工作流线程
- 任务状态监控
- 构建执行命令

## 测试基础设施

### CLIRunner

**文件**：[__tests__/utils/cli-runner.ts](file:///d:/项目/agent/wf-agent/apps/cli-app/__tests__/utils/cli-runner.ts)

**功能**：
- 封装子进程调用
- 自动设置测试环境变量
- 超时控制
- 输出捕获和保存

```typescript
export class CLIRunner {
  private defaultEnv = {
    NODE_ENV: "test",
    TEST_MODE: "true",
    LOG_DIR: this.outputDir,
    DISABLE_LOG_TERMINAL: "true",
  };

  async run(args: string[], options?: CLIRunOptions): Promise<CLIRunResult>
}
```

## 现有架构的优势

### 1. 分层清晰
- 输出流分离（stdout/stderr/log file）
- 命令层、适配器层、SDK 层职责明确
- 终端管理与业务逻辑解耦

### 2. 已有无头模式基础
- 通过环境变量可启用
- 自动退出机制已存在
- 测试基础设施完善

### 3. 终端管理完善
- 支持前后台两种终端模式
- 基于 RxJS 的事件通信
- 资源清理机制完整

### 4. 输出系统强大
- 多种输出格式支持
- 彩色输出控制
- 结构化数据展示

## 现有架构的不足

### 1. 无头模式与测试模式耦合

**问题**：`TEST_MODE` 和 `HEADLESS` 逻辑混用，缺乏清晰的模式定义

**代码位置**：[src/index.ts](file:///d:/项目/agent/wf-agent/apps/cli-app/src/index.ts)

```typescript
const isHeadless = process.env["HEADLESS"] === "true" || process.env["TEST_MODE"] === "true";
```

**影响**：
- 测试模式强制启用无头特性，可能不符合所有测试场景
- 无法单独配置无头模式行为

### 2. 缺乏独立的无头模式配置

**问题**：无头模式行为无法精细控制

**缺失配置**：
- 输出格式选择（text/json）
- 退出码控制
- 超时设置
- 日志级别独立控制

### 3. 退出机制不够完善

**问题**：参考 [headless-mode-design.md](../headless-mode-design.md) 中的分析

- Commander.js 的 postAction hook 可能在输出完成前触发
- 没有等待所有异步输出完成
- 缺少对事件循环的精确控制

### 4. SDK 日志重定向限制

**问题**：SDK 初始化后才配置 logger

**代码位置**：[src/index.ts#L34-L52](file:///d:/项目/agent/wf-agent/apps/cli-app/src/index.ts#L34-L52)

```typescript
.hook("preAction", async thisCommand => {
  // 1. 初始化输出系统
  const output = initializeOutput({...});
  
  // 2. 初始化日志
  initLogger({...});
  initSDKLogger({...});
  
  // 3. 加载配置并初始化 SDK
  const config = await loadConfig();
  getSDK({...});  // SDK 在这里初始化
})
```

**影响**：SDK 初始化期间的日志无法重定向

### 5. 命令逻辑与 CLI 框架耦合

**问题**：业务逻辑嵌入 Commander.js 命令定义中

**示例**：[src/commands/workflow/index.ts](file:///d:/项目/agent/wf-agent/apps/cli-app/src/commands/workflow/index.ts)

```typescript
workflowCmd
  .command("list")
  .action(async (options) => {
    // 业务逻辑与命令解析耦合
    const adapter = new WorkflowAdapter();
    const workflows = await adapter.listWorkflows();
    // ...
  });
```

**影响**：其他 app 无法直接复用业务逻辑

## 相关文档

- [output-and-logging-guide.md](../../../apps/cli-app/docs/output-and-logging-guide.md) - 输出与日志使用规范
- [headless-mode-design.md](../../../apps/cli-app/docs/headless-mode-design.md) - 无头模式设计分析
- [terminal-separation.md](./terminal-separation.md) - 终端分离设计
- [architecture.md](./architecture.md) - CLI App 整体架构
