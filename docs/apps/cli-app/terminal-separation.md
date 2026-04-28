# 终端分离执行方案

## 技术调研总结

经过对相关技术和库的研究，我们找到了几种在Node.js环境中实现终端分离执行的方案。以下是主要的技术选项和实现方案：

### 1. 使用 node-pty 库

node-pty 是一个强大的库，可以在Node.js中创建和控制伪终端会话。它支持Linux、macOS和Windows平台。

特点：
- 跨平台支持
- 提供真正的终端仿真
- 可以与任何shell或终端程序交互
- 支持双向通信

### 2. 使用 child_process.spawn() 结合特定平台的终端命令

这种方法涉及使用Node.js内置的child_process模块，配合各平台的终端启动命令：

- Windows: `start cmd /k` 或 `start powershell`
- macOS: `open -a Terminal` 或 `osascript -e`
- Linux: `gnome-terminal -e` 或 `xterm -e`

### 3. 使用Electron实现图形化终端窗口

对于需要更强大终端功能的应用，可以使用Electron结合xterm.js创建图形化终端窗口。

## 推荐实现方案

基于我们的需求和项目架构，推荐使用 **node-pty** 作为核心技术，因为它提供了以下优势：

1. 跨平台兼容性
2. 与现有SDK API层的良好集成
3. 支持双向通信
4. 更好的终端仿真能力

## 实现架构

### 1. 终端管理器 (TerminalManager)

创建一个终端管理器类，负责：
- 启动新的终端会话
- 管理多个终端实例
- 处理终端间的通信
- 终端生命周期管理

### 2. 任务执行器 (TaskExecutor)

创建一个任务执行器，专门处理：
- 工作流线程的执行
- 将输出重定向到指定终端
- 监控任务状态
- 处理任务中断和清理

### 3. 通信桥接 (CommunicationBridge)

实现主CLI进程与终端进程之间的通信：
- 使用IPC机制传递消息
- 同步任务状态
- 处理用户输入

## 实现步骤

### 步骤1: 集成 node-pty 依赖

在CLI应用中添加 node-pty 作为依赖，用于创建和管理伪终端。

### 步骤2: 创建终端管理器

实现一个终端管理器类，封装node-pty的功能，提供简单易用的API来创建和管理终端会话。

### 步骤3: 修改线程执行命令

修改现有的线程执行命令，使其能够选择在新终端中运行，而不是阻塞当前终端。

### 步骤4: 实现通信机制

建立主进程与终端进程之间的通信机制，以便同步状态和处理用户输入。

### 步骤5: 测试跨平台兼容性

在Windows、macOS和Linux上测试终端分离功能，确保跨平台兼容性。

## 代码结构示例

```
apps/cli-app/
├── src/
│   ├── terminal/
│   │   ├── terminal-manager.ts    # 终端管理器
│   │   ├── task-executor.ts       # 任务执行器
│   │   ├── communication-bridge.ts # 通信桥接
│   │   └── types.ts               # 终端相关类型定义
│   └── commands/
│       └── thread/
│           └── run-detached.ts    # 在新终端中运行线程的命令
```

## 使用示例

用户可以通过以下命令在新终端中启动工作流线程：

```bash
# 在新终端中运行工作流，主终端保持可用
modular-agent thread run-detached <workflow-id>

# 或者使用标志
modular-agent thread run <workflow-id> --detached
```

这样，工作流的执行将在新的终端窗口中进行，而原始终端保持响应，可以继续接收其他命令。

## 注意事项

1. 需要处理好终端的清理和资源释放
2. 确保在任务完成后正确关闭终端会话
3. 考虑错误处理和异常情况下的终端状态管理
4. 实现适当的权限检查，确保安全执行外部命令