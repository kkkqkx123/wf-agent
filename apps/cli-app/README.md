# WF-Agent CLI Application

工作流智能体框架命令行应用

## 概述

WF-Agent CLI 是工作流智能体框架的命令行工具，提供了完整的工作流管理、工作流执行、检查点管理和模板管理功能。

## 安装

```bash
# 在项目根目录安装依赖
pnpm install

# 构建 CLI 应用
pnpm --filter @wf-agent/cli-app build
```

## 使用方法

### 基本命令

```bash
# 显示帮助信息
wf-agent --help

# 显示版本信息
wf-agent --version

# 启用详细输出模式
wf-agent --verbose <command>

# 启用调试模式
wf-agent --debug <command>
```

### 工作流管理

```bash
# 管理工作流
wf-agent workflow

# 从文件注册工作流
wf-agent workflow register <file>

# 从目录批量注册工作流
wf-agent workflow register-batch <directory> [options]
  -r, --recursive          递归加载子目录
  -p, --pattern <pattern>  文件模式 (正则表达式)
  --params <params>        运行时参数 (JSON 格式)

# 列出所有工作流
wf-agent workflow list [options]
  -t, --table              以表格格式输出
  -v, --verbose            详细输出

# 查看工作流详情
wf-agent workflow show <id> [options]
  -v, --verbose            详细输出

# 删除工作流
wf-agent workflow delete <id> [options]
  -f, --force              强制删除，不提示确认
```

### 工作流执行管理

```bash
# 管理工作流执行
wf-agent execution

# 执行工作流（默认在独立终端中运行，不阻塞当前终端）
wf-agent execution run <workflow-id> [options]
  -i, --input <json>       输入数据(JSON格式)
  -v, --verbose            详细输出
  -b, --blocking           在当前终端中运行（阻塞方式）
  --background             在后台运行（不显示终端窗口）
  --log-file <path>        后台运行时的日志文件路径

# 查看任务状态
wf-agent execution status <task-id>

# 取消任务执行
wf-agent execution cancel <task-id>

# 列出所有活跃终端
wf-agent execution terminals

# 暂停工作流执行
wf-agent execution pause <execution-id>

# 恢复工作流执行
wf-agent execution resume <execution-id>

# 停止工作流执行
wf-agent execution stop <execution-id>

# 列出所有工作流执行
wf-agent execution list [options]
  -t, --table              以表格格式输出
  -v, --verbose            详细输出

# 查看工作流执行详情
wf-agent execution show <execution-id> [options]
  -v, --verbose            详细输出

# 删除工作流执行
wf-agent execution delete <execution-id> [options]
  -f, --force              强制删除，不提示确认
```

#### 终端分离执行

CLI 应用支持终端分离执行功能，允许工作流在独立的终端窗口中运行，而不阻塞主终端。

**特性：**

- 🚀 非阻塞执行 - 主终端保持可用，可同时执行多个任务
- 🌍 跨平台支持 - 支持 Windows、macOS 和 Linux
- 📊 任务管理 - 完整的任务状态监控和管理
- 🔧 灵活配置 - 支持前台、后台和阻塞三种运行模式
- 📝 日志记录 - 后台运行时自动记录日志

**运行模式：**

1. **前台模式（默认）** - 在独立的终端窗口中运行，可以看到实时输出
2. **后台模式** - 在后台运行，不显示终端窗口，输出记录到日志文件
3. **阻塞模式** - 在当前终端中运行，阻塞终端直到任务完成

**使用示例：**

```bash
# 前台模式：在独立终端中运行（默认）
wf-agent execution run my-workflow

# 带输入数据运行
wf-agent execution run my-workflow --input '{"name":"test"}'

# 后台模式：在后台运行，不显示终端窗口
wf-agent execution run my-workflow --background

# 后台模式：指定日志文件路径
wf-agent execution run my-workflow --background --log-file ./logs/my-task.log

# 阻塞模式：在当前终端中运行
wf-agent execution run my-workflow --blocking

# 查看任务状态
wf-agent execution status <task-id>

# 取消任务
wf-agent execution cancel <task-id>

# 列出活跃终端
wf-agent execution terminals
```

### 检查点管理

```bash
# 管理检查点
wf-agent checkpoint

# 创建工作流执行检查点
wf-agent checkpoint create <execution-id> [options]
  -n, --name <name>        检查点名称
  -v, --verbose            详细输出

# 加载检查点
wf-agent checkpoint load <checkpoint-id>

# 列出所有检查点
wf-agent checkpoint list [options]
  -t, --table              以表格格式输出
  -v, --verbose            详细输出

# 查看检查点详情
wf-agent checkpoint show <checkpoint-id> [options]
  -v, --verbose            详细输出

# 删除检查点
wf-agent checkpoint delete <checkpoint-id> [options]
  -f, --force              强制删除，不提示确认
```

### 模板管理

```bash
# 管理模板
wf-agent template

# 注册节点模板
wf-agent template register-node <file> [options]
  -v, --verbose            详细输出

# 批量注册节点模板
wf-agent template register-nodes-batch <directory> [options]
  -r, --recursive          递归加载子目录
  -p, --pattern <pattern>  文件模式 (正则表达式)

# 注册触发器模板
wf-agent template register-trigger <file> [options]
  -v, --verbose            详细输出

# 批量注册触发器模板
wf-agent template register-triggers-batch <directory> [options]
  -r, --recursive          递归加载子目录
  -p, --pattern <pattern>  文件模式 (正则表达式)

# 列出所有节点模板
wf-agent template list-nodes [options]
  -t, --table              以表格格式输出
  -v, --verbose            详细输出

# 列出所有触发器模板
wf-agent template list-triggers [options]
  -t, --table              以表格格式输出
  -v, --verbose            详细输出

# 查看节点模板详情
wf-agent template show-node <id> [options]
  -v, --verbose            详细输出

# 查看触发器模板详情
wf-agent template show-trigger <id> [options]
  -v, --verbose            详细输出

# 删除节点模板
wf-agent template delete-node <id> [options]
  -f, --force              强制删除，不提示确认

# 删除触发器模板
wf-agent template delete-trigger <id> [options]
  -f, --force              强制删除，不提示确认
```

## 配置

CLI 应用支持多种配置文件格式：

- `.modular-agentrc`
- `.modular-agentrc.json`
- `.modular-agentrc.yaml`
- `.modular-agentrc.yml`
- `modular-agent.config.js`
- `modular-agent.config.ts`

### 配置选项

```json
{
  "apiUrl": "https://api.example.com",
  "apiKey": "your-api-key",
  "defaultTimeout": 30000,
  "verbose": false,
  "debug": false,
  "logLevel": "warn",
  "outputFormat": "table",
  "maxConcurrentExecutions": 5
}
```

## 开发

### 项目结构

```
apps/cli-app/
├── src/
│   ├── commands/           # 命令实现
│   │   ├── workflow/       # 工作流命令
│   │   ├── workflow-execution/  # 工作流执行命令
│   │   ├── checkpoint/     # 检查点命令
│   │   └── template/       # 模板命令
│   ├── adapters/           # 适配器层
│   │   ├── base-adapter.ts       # 基础适配器类
│   │   ├── workflow-adapter.ts
│   │   ├── workflow-execution-adapter.ts
│   │   ├── checkpoint-adapter.ts
│   │   └── template-adapter.ts
│   ├── terminal/           # 终端管理模块
│   │   ├── types.ts        # 终端相关类型定义
│   │   ├── terminal-manager.ts    # 终端管理器
│   │   ├── task-executor.ts       # 任务执行器
│   │   └── communication-bridge.ts # 通信桥接
│   ├── utils/              # 工具函数
│   │   ├── logger.ts       # 日志工具
│   │   ├── validator.ts    # 输入验证工具
│   │   ├── error-handler.ts # 错误处理工具
│   │   └── formatter.ts    # 格式化工具
│   ├── types/              # 类型定义
│   │   └── cli-types.ts
│   ├── config/             # 配置管理
│   │   ├── config-loader.ts
│   │   └── config-manager.ts
│   └── index.ts            # 入口文件
├── scripts/
│   └── modular-agent.js    # 可执行脚本入口
├── package.json
├── tsconfig.json
└── README.md
```

### 开发脚本

```bash
# 构建项目
pnpm --filter @modular-agent/cli-app build

# 监听模式构建
pnpm --filter @modular-agent/cli-app dev

# 运行 CLI
pnpm --filter @modular-agent/cli-app start

# 类型检查
pnpm --filter @modular-agent/cli-app typecheck

# 清理构建文件
pnpm --filter @modular-agent/cli-app clean
```

### 添加新命令

1. 在 `src/commands/` 下创建命令文件
2. 实现命令逻辑
3. 在 `src/index.ts` 中注册命令
4. 添加相应的适配器（如果需要）

## 依赖项

### 核心依赖

- `commander` - CLI 框架
- `@modular-agent/sdk` - 核心 SDK
- `@modular-agent/common-utils` - 公共工具

### 终端管理依赖

- `node-pty` - 伪终端创建和管理
- `rxjs` - 响应式编程库，用于进程间通信
- `uuid` - 唯一标识符生成

### 工具依赖

- `cosmiconfig` - 配置文件加载
- `zod` - 运行时验证
- `chalk` - 终端颜色输出
- `ora` - 加载动画
- `inquirer` - 交互式输入
- `yaml` - YAML 解析
- `@iarna/toml` - TOML 解析
- `fs-extra` - 增强版文件系统操作
- `p-map` - 并发数组映射
- `p-limit` - 并发限制
- `cli-progress` - 进度条组件

## 架构设计

CLI 应用采用分层架构：

1. **CLI Layer**: 使用 Commander.js 处理命令行参数解析
2. **Adapter Layer**: 将 CLI 参数转换为 SDK API 调用
3. **SDK Layer**: 调用核心 SDK 功能

所有适配器统一继承 `BaseAdapter`，提供统一的错误处理和 SDK 访问。

### 输出流架构

CLI 应用使用全新的输出流架构，提供灵活的输出处理能力：

**核心组件：**

- **OutputTarget**: 输出目标抽象（控制台、文件、内存等）
- **OutputTransform**: 转换流层（ANSI 处理、JSON 格式化等）
- **OutputRouter**: 输出路由器（多目标路由、过滤）
- **OutputSystem**: 统一输出接口
- **OutputFormatter**: 格式化器（状态消息、表格、列表等）

**输出模式：**

- `INTERACTIVE`: 交互模式，支持 ANSI 和颜色
- `HEADLESS`: 无头模式，纯文本输出
- `QUIET`: 静默模式，最小化输出
- `JSON`: JSON 格式输出

**使用示例：**

```typescript
import { OutputSystem, OutputMode } from "./utils/output";

// 创建输出系统
const output = new OutputSystem();
output.applyPreset(OutputMode.INTERACTIVE);

// 输出消息
output.info("信息消息");
output.success("成功消息");
output.warn("警告消息");
output.error("错误消息");

// 输出数据
output.data({ key: "value" });
output.table(["Name", "Status"], [["Item 1", "Active"]]);
```

详细的架构设计请参考 [架构文档](../../docs/apps/cli-app/architecture.md) 和 [输出流设计文档](./docs/output-stream-redesign.md)。

## 分阶段实施

CLI 应用的开发分为四个阶段：

1. **第一阶段**: 项目初始化和基础设置 ✅
2. **第二阶段**: 核心功能实现 ✅
3. **第三阶段**: 用户体验优化 🔄
4. **第四阶段**: 高级功能和扩展 ⏳

详细的阶段规划请参考 [阶段文档](../../docs/apps/cli-app/phases.md)。

## 贡献

欢迎贡献！请遵循项目的贡献指南。

## 许可证

MIT

## 联系方式

- 项目主页: [Modular Agent Framework](https://github.com/your-org/modular-agent-framework)
- 问题反馈: [GitHub Issues](https://github.com/your-org/modular-agent-framework/issues)
