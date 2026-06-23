# CLI应用输出流和无头模式设计分析

## 问题概述

当前CLI应用在集成测试中存在以下问题：

1. **超时问题**：测试命令执行后进程不退出，导致30秒超时
2. **输出混乱**：SDK日志和命令输出混合，难以区分
3. **输出捕获失败**：测试框架无法正确捕获命令的标准输出

## 根本原因分析

### 1. 输出流架构问题

#### 当前架构
```
CLI应用
├── Logger (文件流)
│   └── 写入日志文件
├── Output (stdout/stderr)
│   └── 写入终端
└── SDK Logger (stdout)
    └── 写入终端（与Output混合）
```

#### 问题
- SDK的logger默认输出到stdout，导致日志和用户输出混合
- 日志初始化时机晚于SDK初始化，无法控制SDK的输出流
- 没有统一的输出流管理机制

### 2. 无头模式退出机制问题

#### 当前退出逻辑
```typescript
// 监听SIGINT和SIGTERM信号
process.on("SIGINT", cleanup);
process.on("SIGTERM", cleanup);

// 无头模式下手动退出
program.hook('postAction', async () => {
  process.exit(0);
});
```

#### 问题
- Commander.js的postAction hook可能在输出完成前触发
- 没有等待所有异步输出完成
- 缺少对事件循环的精确控制

## 设计方案

### 方案一：分层输出流管理

#### 架构设计
```
CLI应用
├── Logger (仅日志)
│   └── 写入日志文件
├── Output (仅用户输出)
│   ├── 模式1: 写入stdout (交互模式)
│   └── 模式2: 写入特定流 (无头模式)
└── SDK Logger
    ├── 模式1: 写入stdout (调试模式)
    └── 模式2: 写入日志文件 (生产模式)
```

#### 实现要点
1. **日志初始化优先**：在SDK初始化前配置好所有logger
2. **输出流隔离**：Logger、Output、SDK Logger使用不同的流
3. **模式切换**：根据运行模式切换输出目标

### 方案二：统一输出流管理器

#### 架构设计
```typescript
interface OutputMode {
  mode: 'interactive' | 'headless' | 'batch';
  stdoutStream: WritableStream;
  stderrStream: WritableStream;
  logStream: WritableStream;
}

class OutputManager {
  private mode: OutputMode;

  initialize(mode: OutputMode) {
    // 配置所有输出流
    this.configureLogger(mode.logStream);
    this.configureOutput(mode.stdoutStream, mode.stderrStream);
    this.configureSDKLogger(mode.logStream);
  }

  ensureDrained(): Promise<void> {
    // 确保所有输出都刷新完成
    return Promise.all([
      this.waitForStream(this.stdoutStream),
      this.waitForStream(this.stderrStream),
      this.waitForStream(this.logStream)
    ]);
  }
}
```

#### 实现要点
1. **统一管理**：所有输出流通过OutputManager管理
2. **模式切换**：支持交互模式、无头模式、批处理模式
3. **安全退出**：ensureDrained()确保所有输出完成

### 方案三：双进程架构（推荐）

#### 架构设计
```
主进程 (交互层)
├── 用户交互
├── 命令解析
└── 输出显示

工作进程 (执行层)
├── SDK初始化
├── 命令执行
└── 结果输出

通信机制
├── IPC通道 (进程间通信)
└── 输出流重定向
```

#### 实现要点
1. **进程隔离**：工作进程只负责执行，输出通过IPC传递
2. **输出控制**：主进程可以完全控制工作进程的输出
3. **安全退出**：工作进程执行完成后自动退出，主进程等待输出

## 推荐实现方案

基于当前代码库的实际情况，推荐**方案二的简化版本**：

### 核心设计

#### 1. 输出流配置
```typescript
// 在preAction hook中统一配置所有输出流
program.hook("preAction", async thisCommand => {
  const options = thisCommand.opts();
  const isHeadless = process.env['TEST_MODE'] === 'true';

  // 创建输出流
  const outputStreams = isHeadless 
    ? createHeadlessOutputStreams() 
    : createInteractiveOutputStreams();

  // 配置logger
  initializeLogger(outputStreams.logStream, options);

  // 配置SDK logger（在SDK初始化前）
  configureSDKLogger(outputStreams.logStream, options);

  // 配置output
  configureOutput(outputStreams.stdoutStream, outputStreams.stderrStream);

  // 保存到全局上下文
  setOutputStreams(outputStreams);
});
```

#### 2. 安全退出机制
```typescript
program.hook('postAction', async () => {
  const streams = getOutputStreams();
  
  // 等待所有异步操作完成
  await Promise.all([
    waitForEventLoopDrain(),
    streams.ensureAllDrained()
  ]);

  // 优雅退出
  process.exit(0);
});
```

#### 3. SDK日志重定向
```typescript
function configureSDKLogger(stream: WritableStream, options: any) {
  const { createPackageLogger, registerLogger } = await import("@modular-agent/common-utils");
  
  const sdkLogger = createPackageLogger("sdk", {
    level: options.debug ? "debug" : options.verbose ? "info" : "warn",
    stream: stream,
    timestamp: true,
  });
  
  registerLogger("sdk", sdkLogger);
}
```

### 关键改进点

1. **日志优先级**：SDK日志重定向到文件，保持stdout用于命令输出
2. **输出分离**：Logger、Output、SDK Logger使用不同的流
3. **安全退出**：确保所有输出完成后再退出进程
4. **模式支持**：支持交互模式和无头模式的切换

## 下一步行动

1. **重构Logger初始化**：确保在SDK初始化前完成所有logger配置
2. **实现OutputManager**：统一管理所有输出流
3. **完善退出机制**：确保输出完成后再退出
4. **更新测试配置**：确保测试能正确捕获输出

## 兼容性考虑

1. **向后兼容**：保持现有命令行参数和配置格式
2. **环境变量**：使用标准的环境变量（TEST_MODE, HEADLESS）
3. **输出格式**：保持现有的输出格式和风格