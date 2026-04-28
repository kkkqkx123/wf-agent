# CLI 应用分析与实现方案

## 项目概述

当前项目是一个模块化的工作流引擎，具有完整的SDK层，包含API层、核心层、类型层和工具层。项目已经具备了完善的功能体系，包括工作流执行引擎、多模型LLM集成、灵活的工具系统、Fork/Join支持和检查点机制。

## CLI 应用功能需求

CLI应用需要提供以下核心功能：

1. **工作流管理**：
   - 注册工作流
   - 列出所有工作流
   - 查看工作流详情
   - 删除工作流

2. **节点模板和工作流模板管理**：
   - 注册节点模板
   - 注册工作流模板
   - 列出模板
   - 查看模板详情

3. **线程执行管理**：
   - 执行工作流线程
   - 暂停线程
   - 恢复线程
   - 停止线程

4. **检查点管理**：
   - 创建检查点
   - 载入检查点
   - 列出检查点

5. **基础CRUD操作**：
   - 对各种资源进行创建、读取、更新、删除操作

## API 层功能分析

根据 `sdk/api/index.ts` 的内容，API层提供了丰富的功能接口：

### 资源管理API
- `WorkflowRegistryAPI` - 工作流管理
- `ThreadRegistryAPI` - 线程管理
- `NodeRegistryAPI` - 节点模板管理
- `TriggerTemplateRegistryAPI` - 触发器模板管理
- `ToolRegistryAPI` - 工具管理
- `ScriptRegistryAPI` - 脚本管理
- `ProfileRegistryAPI` - 配置文件管理
- `CheckpointResourceAPI` - 检查点管理
- `MessageResourceAPI` - 消息管理
- `VariableResourceAPI` - 变量管理
- `TriggerResourceAPI` - 触发器管理
- `EventResourceAPI` - 事件管理

### 命令API（有副作用操作）
- `ExecuteThreadCommand` - 执行线程
- `PauseThreadCommand` - 暂停线程
- `ResumeThreadCommand` - 恢复线程
- `CancelThreadCommand` - 取消线程
- `GenerateCommand` - LLM生成命令
- `ExecuteScriptCommand` - 执行脚本命令
- `ExecuteToolCommand` - 执行工具命令

### 查询API（纯查询操作）
- 各种资源的查询接口

### 订阅API（事件订阅）
- `OnEventSubscription` - 事件监听
- `OnceEventSubscription` - 单次事件监听

## 推荐的CLI框架

基于项目需求和现有架构，推荐使用 **Commander.js** 作为CLI框架，原因如下：

1. **TypeScript兼容性**：优秀的TypeScript支持，与现有代码库保持一致
2. **API集成**：易于与现有SDK API层集成
3. **命令组织**：支持分层命令结构，适合复杂CLI应用
4. **成熟稳定**：广泛使用，文档丰富，社区支持好
5. **异步支持**：原生支持async/await，适合API调用

## 额外依赖项

### 核心依赖
- `commander` - CLI框架
- `@modular-agent/sdk` - 项目SDK（工作区引用）

### 输入验证和配置
- `cosmiconfig` - 配置文件加载
- `zod` - 运行时验证（已在SDK中使用）

### 异步操作处理
- `p-map` - 并发映射
- `p-limit` - 并发限制

### 错误处理和日志
- `winston` - 日志记录
- `ora` - 终端加载动画

### 文件操作
- `fs-extra` - 增强文件系统操作
- `@iarna/toml` - TOML解析
- `toml` - TOML解析

### 用户界面
- `inquirer` - 交互式输入
- `chalk` - 终端颜色输出
- `cli-progress` - 进度条

## CLI命令结构设计

```bash
# 工作流管理
modular-agent workflow register <file> [options]  # 注册工作流
modular-agent workflow list [options]             # 列出工作流
modular-agent workflow show <id>                  # 查看工作流详情
modular-agent workflow delete <id>                # 删除工作流

# 线程管理
modular-agent thread run <workflow-id> [options]  # 执行线程
modular-agent thread pause <thread-id>            # 暂停线程
modular-agent thread resume <thread-id>           # 恢复线程
modular-agent thread stop <thread-id>             # 停止线程

# 检查点管理
modular-agent checkpoint create <thread-id> [options]  # 创建检查点
modular-agent checkpoint load <checkpoint-id>          # 载入检查点
modular-agent checkpoint list [options]                # 列出检查点

# 模板管理
modular-agent template register <file> [options]       # 注册模板
modular-agent template list [options]                  # 列出模板
```

## 实现示例

```typescript
#!/usr/bin/env node

import { program } from 'commander';
import { sdk } from '@modular-agent/sdk';
import chalk from 'chalk';
import ora from 'ora';

async function initializeSDK() {
  // 初始化SDK
  await sdk.init();
}

// 工作流相关命令
program
  .command('workflow')
  .description('管理工作流')
  .alias('wf')
  .action(() => {
    program.outputHelp();
  });

program
  .command('workflow register <file>')
  .description('从文件注册工作流')
  .option('-n, --name <name>', '工作流名称')
  .option('-v, --version <version>', '工作流版本')
  .action(async (file, options) => {
    const spinner = ora('正在注册工作流...');
    try {
      spinner.start();
      
      await initializeSDK();
      
      // 读取工作流文件
      const workflowData = JSON.parse(fs.readFileSync(file, 'utf8'));
      
      // 使用SDK API注册工作流
      const result = await sdk.workflows.create(workflowData);
      
      spinner.succeed(`工作流已成功注册: ${result.id}`);
    } catch (error) {
      spinner.fail(chalk.red(`注册失败: ${error.message}`));
      process.exit(1);
    }
  });

// 线程执行命令
program
  .command('thread run <workflow-id>')
  .description('执行工作流')
  .option('-i, --input <json>', '输入数据(JSON格式)')
  .action(async (workflowId, options) => {
    const spinner = ora('正在启动线程...');
    try {
      spinner.start();
      
      await initializeSDK();
      
      // 解析输入数据
      const inputData = options.input ? JSON.parse(options.input) : {};
      
      // 使用SDK API执行线程
      const result = await sdk.executor.execute({
        workflowId,
        input: inputData
      });
      
      spinner.succeed(`线程已启动: ${result.threadId}`);
      console.log(chalk.green(`状态: ${result.status}`));
    } catch (error) {
      spinner.fail(chalk.red(`执行失败: ${error.message}`));
      process.exit(1);
    }
  });

program.parse();
```

## API层扩展建议

当前API层功能已经比较完善，但为了更好地支持CLI应用，可以考虑以下扩展：

1. **批量操作API**：
   - `createMany()` - 批量创建资源
   - `deleteMany()` - 批量删除资源
   - `updateMany()` - 批量更新资源

2. **导入导出功能**：
   - `export()` - 导出资源为文件
   - `import()` - 从文件导入资源

3. **增强的查询API**：
   - 支持更复杂的过滤条件
   - 支持分页查询

4. **状态监控API**：
   - 实时获取线程执行状态
   - 获取资源统计信息

## 总结

基于现有API层，我们可以构建一个功能完善的CLI应用，利用Commander.js作为CLI框架，结合适当的辅助库来提供良好的用户体验。API层已经提供了大部分所需功能，只需按照上述结构组织CLI命令即可实现所需功能。

CLI应用将作为SDK功能的直接前端，使用户能够方便地通过命令行管理整个工作流引擎系统。