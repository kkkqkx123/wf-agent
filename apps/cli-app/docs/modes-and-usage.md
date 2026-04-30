# CLI应用模式与使用指南

## 概述

Modular Agent CLI应用提供了多种执行模式，以适应不同的使用场景，包括交互式使用、自动化测试、批处理等。本文档详细介绍了所有可用的模式及其使用方法。

---

## 一、执行模式（Execution Modes）

### 1.1 交互模式（Interactive Mode）

**描述**：默认模式，适用于用户直接在终端中使用CLI。

**特点**：
- 支持ANSI颜色和格式化输出
- 支持交互式提示和确认
- 输出到stdout和stderr
- 进程在命令执行后保持运行

**启用方式**：
```bash
# 默认模式，无需特殊配置
modular-agent <command>
```

**环境变量**：
```bash
CLI_MODE=interactive  # 显式指定（可选）
```

**使用示例**：
```bash
# 注册工作流
modular-agent workflow register ./my-workflow.toml

# 查看工作流列表（表格格式）
modular-agent workflow list --table

# 执行线程（在独立终端中运行）
modular-agent thread run my-workflow

# 运行Agent Loop（流式输出）
modular-agent agent run --stream
```

---

### 1.2 无头模式（Headless Mode）

**描述**：适用于自动化脚本、CI/CD、测试等非交互场景。

**特点**：
- 禁用ANSI颜色和格式化
- 纯文本输出，便于程序解析
- 命令执行完成后自动退出进程
- 输出重定向到指定流（便于测试捕获）

**启用方式**：
```bash
# 方式1：环境变量
export CLI_MODE=headless
modular-agent <command>

# 方式2：环境变量（兼容旧版）
export HEADLESS=true
modular-agent <command>

# 方式3：测试模式
export TEST_MODE=true
modular-agent <command>
```

**环境变量配置**：
```bash
CLI_MODE=headless              # 执行模式
CLI_OUTPUT_FORMAT=text         # 输出格式：text | json | silent
CLI_LOG_LEVEL=info            # 日志级别：debug | verbose | info | warn | error
CLI_LOG_FILE=./logs/cli.log   # 日志文件路径
NO_COLOR=1                    # 禁用颜色输出
```

**使用示例**：
```bash
# 在CI/CD中使用
export CLI_MODE=headless
modular-agent workflow list --json > workflows.json

# 在测试中使用
export TEST_MODE=true
modular-agent workflow register ./test-workflow.toml

# 禁用颜色输出
export NO_COLOR=1
modular-agent agent run --input '{"message":"test"}'
```

**输出格式选项**：
```bash
# JSON格式输出（便于程序解析）
modular-agent workflow show <id> --json

# 静默模式（最小化输出）
modular-agent workflow delete <id> --force --silent
```

---

### 1.3 编程模式（Programmatic Mode）

**描述**：适用于通过代码调用CLI的场景。

**特点**：
- 与无头模式类似，但提供额外的编程接口
- 支持通过API调用CLI功能
- 返回结构化数据

**启用方式**：
```bash
export CLI_MODE=programmatic
```

**使用示例**：
```bash
export CLI_MODE=programmatic
modular-agent workflow list
```

---

## 二、线程执行模式（Thread Execution Modes）

### 2.1 前台分离模式（Foreground Detached Mode）- 默认

**描述**：在独立的终端窗口中运行工作流，不阻塞当前终端。

**特点**：
- 非阻塞执行，主终端保持可用
- 显示实时输出在独立终端窗口
- 支持跨平台（Windows/macOS/Linux）

**使用方法**：
```bash
# 默认模式，无需额外参数
modular-agent thread run <workflow-id>

# 带输入数据
modular-agent thread run my-workflow --input '{"name":"test"}'
```

**输出示例**：
```
Thread started in separate terminal.
  Task ID: task-abc123
  Terminal ID: session-xyz789
  Process ID: 12345
  Startup time: 2026-04-22T10:30:00.000Z

Use 'modular-agent thread status task-abc123' to check task status
```

---

### 2.2 后台模式（Background Mode）

**描述**：在后台运行，不显示终端窗口，输出记录到日志文件。

**特点**：
- 完全后台运行，不干扰用户
- 自动记录日志到文件
- 适合长时间运行的任务

**使用方法**：
```bash
# 后台运行（使用默认日志路径）
modular-agent thread run <workflow-id> --background

# 指定自定义日志文件
modular-agent thread run <workflow-id> --background --log-file ./logs/my-task.log
```

**输出示例**：
```
Thread started in background.
  Task ID: task-abc123
  Process ID: 12345
  Log file: logs/task-abc123.log
  Startup time: 2026-04-22T10:30:00.000Z

Use 'modular-agent thread status task-abc123' to check task status
```

**日志文件说明**：
- 默认路径：`logs/task-<task-id>.log`
- 日志目录自动创建（如果不存在）
- 查看日志了解任务执行情况

---

### 2.3 阻塞模式（Blocking Mode）

**描述**：在当前终端中运行，阻塞终端直到任务完成。

**特点**：
- 同步执行，等待任务完成
- 实时显示输出
- 适合需要立即看到结果的场景

**使用方法**：
```bash
# 阻塞模式运行
modular-agent thread run <workflow-id> --blocking

# 带输入数据和详细输出
modular-agent thread run my-workflow --input '{"name":"test"}' --blocking --verbose
```

---

## 三、Agent Loop执行模式

### 3.1 同步执行模式（Sync Execution）

**描述**：同步执行Agent Loop，等待完成后返回结果。

**使用方法**：
```bash
# 基本用法
modular-agent agent run

# 使用配置文件
modular-agent agent run --config ./agent-config.toml

# 使用命令行参数
modular-agent agent run \
  --profile gpt-4 \
  --system-prompt "You are a helpful assistant" \
  --max-iterations 10 \
  --tools "readFile,writeFile,bash" \
  --input '{"message":"Hello"}' \
  --verbose
```

**参数说明**：
- `-c, --config <file>`: 配置文件路径（TOML或JSON）
- `-p, --profile <profileId>`: LLM Profile ID
- `-s, --system-prompt <prompt>`: 系统提示词
- `-m, --max-iterations <number>`: 最大迭代次数（默认10）
- `-t, --tools <tools>`: 工具列表（逗号分隔）
- `-i, --input <json>`: 初始输入数据（JSON格式）
- `-v, --verbose`: 详细输出

---

### 3.2 流式执行模式（Stream Execution）

**描述**：实时流式输出Agent Loop的执行过程。

**使用方法**：
```bash
# 启用流式输出
modular-agent agent run --stream

# 流式执行带配置
modular-agent agent run --config ./agent-config.toml --stream
```

**流式输出示例**：
```
Executing Agent Loop...
I'll help you with that task.

Calling tool: readFile
Tool call completed: call-123

Iteration complete: 1

Thinking about the next step...
```

**流式事件类型**：
- `text`: 文本增量输出
- `tool_call_start`: 工具调用开始
- `tool_call_end`: 工具调用结束
- `iteration_complete`: 迭代完成

---

### 3.3 异步执行模式（Async Execution）

**描述**：异步启动Agent Loop，立即返回任务ID。

**使用方法**：
```bash
# 异步启动
modular-agent agent start \
  --profile gpt-4 \
  --system-prompt "You are a helpful assistant" \
  --tools "readFile,writeFile" \
  --input '{"message":"Hello"}'
```

**输出示例**：
```
Agent Loop started.
  ID: agent-loop-abc123

Agent Loop ID: agent-loop-abc123
```

**后续操作**：
```bash
# 查看状态
modular-agent agent status <id>

# 暂停
modular-agent agent pause <id>

# 恢复
modular-agent agent resume <id>

# 停止
modular-agent agent stop <id>
```

---

## 四、输出格式模式

### 4.1 表格格式（Table Format）

**描述**：以表格形式输出，便于阅读。

**使用方法**：
```bash
# 工作流列表
modular-agent workflow list --table

# Agent Loop列表
modular-agent agent list --table

# 检查点列表
modular-agent checkpoint list --table
```

**输出示例**：
```
ID        Name      Status    Creation time
--------  --------  --------  ------------------
abc123    My Workflow  active  2026-04-22T10:00:00
def456    Test Workflow  paused  2026-04-21T15:30:00
```

---

### 4.2 JSON格式（JSON Format）

**描述**：以JSON格式输出，便于程序解析。

**使用方法**：
```bash
# 工作流详情
modular-agent workflow show <id> --json

# Agent Loop详情
modular-agent agent show <id> --json

# 线程详情
modular-agent thread show <id> --json
```

**使用场景**：
- CI/CD流水线
- 自动化脚本
- 数据处理管道

---

### 4.3 详细格式（Verbose Format）

**描述**：显示完整详细信息。

**使用方法**：
```bash
# 工作流详情
modular-agent workflow show <id> --verbose

# Agent Loop详情
modular-agent agent show <id> --verbose

# Agent Loop消息历史
modular-agent agent messages <id> --verbose
```

---

## 五、配置文件模式

### 5.1 配置文件位置

CLI应用支持多种配置文件格式，按优先级顺序查找：

1. `.modular-agentrc`
2. `.modular-agentrc.json`
3. `.modular-agentrc.yaml`
4. `.modular-agentrc.yml`
5. `modular-agent.config.js`
6. `modular-agent.config.ts`

**查找路径**：
- 当前工作目录
- 用户主目录
- 项目根目录

---

### 5.2 配置文件示例

**JSON格式**：
```json
{
  "apiUrl": "https://api.example.com",
  "apiKey": "your-api-key",
  "defaultTimeout": 30000,
  "verbose": false,
  "debug": false,
  "logLevel": "warn",
  "outputFormat": "table",
  "maxConcurrentExecutions": 5,
  "storage": {
    "type": "sqlite",
    "sqlite": {
      "dbPath": "./storage/cli-app.db",
      "enableWAL": true
    }
  },
  "output": {
    "dir": "./outputs",
    "logFilePattern": "cli-app-{date}.log",
    "enableLogTerminal": true,
    "enableSDKLogs": true,
    "sdkLogLevel": "silent"
  },
  "presets": {
    "contextCompression": {
      "enabled": true
    },
    "predefinedTools": {
      "enabled": true,
      "config": {
        "readFile": {
          "workspaceDir": "./workspace",
          "maxFileSize": 1024000
        }
      }
    }
  }
}
```

**YAML格式**：
```yaml
apiUrl: https://api.example.com
apiKey: your-api-key
defaultTimeout: 30000
verbose: false
debug: false
logLevel: warn
outputFormat: table
maxConcurrentExecutions: 5

storage:
  type: sqlite
  sqlite:
    dbPath: ./storage/cli-app.db
    enableWAL: true

output:
  dir: ./outputs
  logFilePattern: cli-app-{date}.log
  enableLogTerminal: true
  enableSDKLogs: true
  sdkLogLevel: silent

presets:
  contextCompression:
    enabled: true
  predefinedTools:
    enabled: true
    config:
      readFile:
        workspaceDir: ./workspace
        maxFileSize: 1024000
```

---

### 5.3 命令行参数覆盖

命令行参数优先级高于配置文件：

```bash
# 配置文件设置 verbose=false，但命令行启用
modular-agent workflow list --verbose

# 配置文件设置 logLevel=warn，但命令行设置为debug
modular-agent workflow list --debug
```

---

## 六、日志模式

### 6.1 日志级别

**可用级别**：
- `debug`: 最详细的调试信息
- `verbose`: 详细输出
- `info`: 一般信息
- `warn`: 警告信息（默认）
- `error`: 仅错误信息

**配置方式**：
```bash
# 通过命令行
modular-agent <command> --debug    # 启用debug级别
modular-agent <command> --verbose  # 启用verbose级别

# 通过配置文件
{
  "logLevel": "debug"
}

# 通过环境变量
export CLI_LOG_LEVEL=debug
```

---

### 6.2 日志文件

**配置方式**：
```bash
# 通过命令行
modular-agent <command> --log-file ./logs/my-log.log

# 通过配置文件
{
  "output": {
    "dir": "./outputs",
    "logFilePattern": "cli-app-{date}.log"
  }
}

# 通过环境变量
export CLI_LOG_FILE=./logs/my-log.log
```

**日志文件模式**：
- `{date}`: 当前日期（YYYY-MM-DD）
- `{time}`: 当前时间（HH-MM-SS）
- `{pid}`: 进程ID

**示例**：
```bash
# 生成日志文件：cli-app-2026-04-22.log
logFilePattern: "cli-app-{date}.log"

# 生成日志文件：cli-app-2026-04-22-10-30-00.log
logFilePattern: "cli-app-{date}-{time}.log"
```

---

### 6.3 SDK日志控制

**配置方式**：
```json
{
  "output": {
    "enableSDKLogs": true,
    "sdkLogLevel": "info"
  }
}
```

**SDK日志级别**：
- `silent`: 禁用SDK日志
- `error`: 仅错误
- `warn`: 警告和错误
- `info`: 信息、警告和错误
- `debug`: 所有日志

---

## 七、存储模式

### 7.1 JSON存储

**描述**：使用JSON文件存储数据。

**配置**：
```json
{
  "storage": {
    "type": "json",
    "json": {
      "baseDir": "./storage",
      "enableFileLock": false,
      "compression": {
        "enabled": false,
        "algorithm": "gzip",
        "threshold": 1024
      }
    }
  }
}
```

---

### 7.2 SQLite存储

**描述**：使用SQLite数据库存储数据（推荐）。

**配置**：
```json
{
  "storage": {
    "type": "sqlite",
    "sqlite": {
      "dbPath": "./storage/cli-app.db",
      "enableWAL": true,
      "enableLogging": false,
      "readonly": false,
      "fileMustExist": false,
      "timeout": 5000
    }
  }
}
```

**特点**：
- 更好的性能
- 支持并发访问
- WAL模式提高并发性能

---

### 7.3 内存存储

**描述**：数据存储在内存中，进程重启后丢失。

**配置**：
```json
{
  "storage": {
    "type": "memory"
  }
}
```

**使用场景**：
- 测试环境
- 临时数据处理
- 不需要持久化的场景

---

## 八、Skill管理模式

### 8.1 初始化Skill目录

```bash
# 初始化Skill目录
modular-agent skill init ./skills
```

---

### 8.2 列出Skill

```bash
# 列出所有Skill
modular-agent skill list

# 表格格式
modular-agent skill list --table

# 按名称过滤
modular-agent skill list --name "frontend-design"

# 详细输出
modular-agent skill list --verbose
```

---

### 8.3 查看Skill详情

```bash
# 查看Skill元数据
modular-agent skill show <name>

# 显示完整内容
modular-agent skill show <name> --content

# 详细输出
modular-agent skill show <name> --verbose
```

---

### 8.4 加载Skill内容

```bash
# 加载原始内容
modular-agent skill load <name>

# 转换为prompt格式
modular-agent skill load <name> --prompt
```

---

### 8.5 搜索Skill

```bash
# 按描述搜索
modular-agent skill search "Create web components"
```

---

### 8.6 列出Skill资源

```bash
# 列出脚本资源
modular-agent skill resources <name>

# 列出引用资源
modular-agent skill resources <name> --type references

# 列出示例资源
modular-agent skill resources <name> --type examples

# 列出资产资源
modular-agent skill resources <name> --type assets
```

---

### 8.7 重载Skill

```bash
# 重载所有Skill
modular-agent skill reload

# 从指定目录重新初始化
modular-agent skill reload --dir ./skills
```

---

### 8.8 清除缓存

```bash
# 清除所有缓存
modular-agent skill clear-cache

# 清除指定Skill的缓存
modular-agent skill clear-cache --name <skill-name>
```

---

### 8.9 生成元数据Prompt

```bash
# 生成Skill元数据prompt（用于系统提示词）
modular-agent skill metadata-prompt
```

---

### 8.10 注册get_skill工具

```bash
# 注册get_skill工具到ToolService
# Agent可以使用get_skill工具按需加载Skill
modular-agent skill register-tool
```

---

## 九、命令组总览

### 9.1 工作流管理（workflow / wf）

```bash
# 注册工作流
modular-agent workflow register <file> [-p, --params <params>]

# 批量注册
modular-agent workflow register-batch <directory> [-r, --recursive] [-p, --pattern <pattern>]

# 列出工作流
modular-agent workflow list [-t, --table] [-v, --verbose]

# 查看详情
modular-agent workflow show <id> [-v, --verbose]

# 删除工作流
modular-agent workflow delete <id> [-f, --force]
```

---

### 9.2 线程管理（thread）

```bash
# 执行线程
modular-agent thread run <workflow-id> [-i, --input <json>] [-v, --verbose] [-b, --blocking] [--background] [--log-file <path>]

# 查看任务状态
modular-agent thread status <task-id>

# 取消任务
modular-agent thread cancel <task-id>

# 列出活跃终端
modular-agent thread terminals

# 暂停线程
modular-agent thread pause <thread-id>

# 恢复线程
modular-agent thread resume <thread-id>

# 停止线程
modular-agent thread stop <thread-id>

# 列出线程
modular-agent thread list [-t, --table] [-v, --verbose]

# 查看详情
modular-agent thread show <thread-id> [-v, --verbose]

# 删除线程
modular-agent thread delete <thread-id> [-f, --force]
```

---

### 9.3 检查点管理（checkpoint）

```bash
# 创建检查点
modular-agent checkpoint create <thread-id> [-n, --name <name>] [-v, --verbose]

# 载入检查点
modular-agent checkpoint load <checkpoint-id>

# 列出检查点
modular-agent checkpoint list [-t, --table] [-v, --verbose]

# 查看详情
modular-agent checkpoint show <checkpoint-id> [-v, --verbose]

# 删除检查点
modular-agent checkpoint delete <checkpoint-id> [-f, --force]
```

---

### 9.4 模板管理（template）

```bash
# 注册节点模板
modular-agent template register-node <file> [-v, --verbose]

# 批量注册节点模板
modular-agent template register-nodes-batch <directory> [-r, --recursive] [-p, --pattern <pattern>]

# 注册触发器模板
modular-agent template register-trigger <file> [-v, --verbose]

# 批量注册触发器模板
modular-agent template register-triggers-batch <directory> [-r, --recursive] [-p, --pattern <pattern>]

# 列出节点模板
modular-agent template list-nodes [-t, --table] [-v, --verbose]

# 列出触发器模板
modular-agent template list-triggers [-t, --table] [-v, --verbose]

# 查看节点模板详情
modular-agent template show-node <id> [-v, --verbose]

# 查看触发器模板详情
modular-agent template show-trigger <id> [-v, --verbose]

# 删除节点模板
modular-agent template delete-node <id> [-f, --force]

# 删除触发器模板
modular-agent template delete-trigger <id> [-f, --force]
```

---

### 9.5 LLM Profile管理（llm-profile）

```bash
# 注册LLM Profile
modular-agent llm-profile register <file> [-v, --verbose]

# 列出LLM Profile
modular-agent llm-profile list [-t, --table] [-v, --verbose]

# 查看详情
modular-agent llm-profile show <id> [-v, --verbose]

# 删除LLM Profile
modular-agent llm-profile delete <id> [-f, --force]
```

---

### 9.6 脚本管理（script）

```bash
# 注册脚本
modular-agent script register <file> [-v, --verbose]

# 列出脚本
modular-agent script list [-t, --table] [-v, --verbose]

# 查看详情
modular-agent script show <id> [-v, --verbose]

# 删除脚本
modular-agent script delete <id> [-f, --force]
```

---

### 9.7 工具管理（tool）

```bash
# 注册工具
modular-agent tool register <file> [-v, --verbose]

# 列出工具
modular-agent tool list [-t, --table] [-v, --verbose]

# 查看详情
modular-agent tool show <id> [-v, --verbose]

# 删除工具
modular-agent tool delete <id> [-f, --force]
```

---

### 9.8 触发器管理（trigger）

```bash
# 注册触发器
modular-agent trigger register <file> [-v, --verbose]

# 列出触发器
modular-agent trigger list [-t, --table] [-v, --verbose]

# 查看详情
modular-agent trigger show <id> [-v, --verbose]

# 删除触发器
modular-agent trigger delete <id> [-f, --force]
```

---

### 9.9 消息管理（message）

```bash
# 注册消息
modular-agent message register <file> [-v, --verbose]

# 列出消息
modular-agent message list [-t, --table] [-v, --verbose]

# 查看详情
modular-agent message show <id> [-v, --verbose]

# 删除消息
modular-agent message delete <id> [-f, --force]
```

---

### 9.10 变量管理（variable）

```bash
# 注册变量
modular-agent variable register <file> [-v, --verbose]

# 列出变量
modular-agent variable list [-t, --table] [-v, --verbose]

# 查看详情
modular-agent variable show <id> [-v, --verbose]

# 删除变量
modular-agent variable delete <id> [-f, --force]
```

---

### 9.11 事件管理（event）

```bash
# 注册事件
modular-agent event register <file> [-v, --verbose]

# 列出事件
modular-agent event list [-t, --table] [-v, --verbose]

# 查看详情
modular-agent event show <id> [-v, --verbose]

# 删除事件
modular-agent event delete <id> [-f, --force]
```

---

### 9.12 Human Relay管理（human-relay）

```bash
# 注册Human Relay
modular-agent human-relay register <file> [-v, --verbose]

# 列出Human Relay
modular-agent human-relay list [-t, --table] [-v, --verbose]

# 查看详情
modular-agent human-relay show <id> [-v, --verbose]

# 删除Human Relay
modular-agent human-relay delete <id> [-f, --force]
```

---

### 9.13 Agent Loop管理（agent）

```bash
# 执行Agent Loop（同步）
modular-agent agent run [-c, --config <file>] [-p, --profile <profileId>] [-s, --system-prompt <prompt>] [-m, --max-iterations <number>] [-t, --tools <tools>] [-i, --input <json>] [--stream] [-v, --verbose]

# 启动Agent Loop（异步）
modular-agent agent start [-p, --profile <profileId>] [-s, --system-prompt <prompt>] [-m, --max-iterations <number>] [-t, --tools <tools>] [-i, --input <json>]

# 暂停Agent Loop
modular-agent agent pause <id>

# 恢复Agent Loop
modular-agent agent resume <id>

# 停止Agent Loop
modular-agent agent stop <id>

# 查看状态
modular-agent agent status <id>

# 查看详情
modular-agent agent show <id> [-v, --verbose]

# 列出Agent Loop
modular-agent agent list [--running] [--paused] [-t, --table]

# 创建检查点
modular-agent agent checkpoint <id> [-n, --name <name>]

# 从检查点恢复
modular-agent agent restore <checkpoint-id>

# 克隆Agent Loop
modular-agent agent clone <id>

# 清理已完成的实例
modular-agent agent cleanup

# 查看消息历史
modular-agent agent messages <id> [-v, --verbose]

# 查看变量
modular-agent agent variables <id> [-t, --table]

# 设置变量
modular-agent agent set-var <id> <name> <value>

# 删除Agent Loop
modular-agent agent delete <id> [-f, --force]
```

---

### 9.14 Skill管理（skill）

```bash
# 初始化Skill目录
modular-agent skill init <directory>

# 列出Skill
modular-agent skill list [-n, --name <name>] [-t, --table] [-v, --verbose]

# 查看Skill详情
modular-agent skill show <name> [-v, --verbose] [-c, --content]

# 加载Skill内容
modular-agent skill load <name> [-p, --prompt]

# 搜索Skill
modular-agent skill search <query>

# 列出Skill资源
modular-agent skill resources <name> [-t, --type <type>]

# 重载Skill
modular-agent skill reload [-d, --dir <directory>]

# 清除缓存
modular-agent skill clear-cache [-n, --name <name>]

# 生成元数据prompt
modular-agent skill metadata-prompt

# 注册get_skill工具
modular-agent skill register-tool
```

---

## 十、使用场景示例

### 10.1 开发环境

```bash
# 交互式开发
modular-agent workflow register ./my-workflow.toml
modular-agent workflow list --table
modular-agent thread run my-workflow --verbose
```

---

### 10.2 CI/CD流水线

```bash
# 无头模式，JSON输出
export CLI_MODE=headless
modular-agent workflow list --json > workflows.json
modular-agent thread run my-workflow --background
modular-agent thread status <task-id>
```

---

### 10.3 自动化测试

```bash
# 测试模式，禁用颜色
export TEST_MODE=true
export NO_COLOR=1
modular-agent workflow register ./test-workflow.toml
modular-agent workflow list
```

---

### 10.4 批处理任务

```bash
# 后台运行，记录日志
modular-agent thread run batch-workflow --background --log-file ./logs/batch.log
```

---

### 10.5 Agent开发

```bash
# 流式执行Agent Loop
modular-agent agent run --stream --config ./agent-config.toml

# 异步启动并监控
modular-agent agent start --profile gpt-4 --tools "readFile,writeFile"
modular-agent agent status <id>
modular-agent agent messages <id> --verbose
```

---

### 10.6 Skill管理

```bash
# 初始化并加载Skill
modular-agent skill init ./skills
modular-agent skill list --table
modular-agent skill show frontend-design --content
modular-agent skill register-tool
```

---

## 十一、最佳实践

### 11.1 配置管理

1. **使用配置文件**：将常用配置保存到配置文件中
2. **环境变量覆盖**：使用环境变量覆盖特定配置
3. **版本控制**：将配置文件纳入版本控制（排除敏感信息）

---

### 11.2 日志管理

1. **开发环境**：使用`--verbose`或`--debug`查看详细信息
2. **生产环境**：使用`--log-file`记录日志到文件
3. **测试环境**：使用`TEST_MODE`禁用颜色输出

---

### 11.3 线程执行

1. **开发调试**：使用`--blocking`模式查看实时输出
2. **长时间任务**：使用`--background`模式后台运行
3. **并发任务**：使用前台分离模式同时运行多个任务

---

### 11.4 Agent Loop

1. **交互开发**：使用`--stream`模式实时查看执行过程
2. **异步任务**：使用`agent start`异步启动，然后监控状态
3. **状态管理**：使用检查点功能保存和恢复状态

---

### 11.5 错误处理

1. **查看详细错误**：使用`--verbose`查看完整错误信息
2. **调试模式**：使用`--debug`启用SDK日志
3. **日志分析**：查看日志文件了解问题原因

---

## 十二、故障排查

### 12.1 进程不退出

**问题**：命令执行后进程不退出

**解决方案**：
```bash
# 启用无头模式
export CLI_MODE=headless
modular-agent <command>

# 或使用测试模式
export TEST_MODE=true
modular-agent <command>
```

---

### 12.2 输出混乱

**问题**：SDK日志和命令输出混合

**解决方案**：
```bash
# 禁用SDK日志到终端
{
  "output": {
    "enableSDKLogs": false
  }
}

# 或将SDK日志级别设置为silent
{
  "output": {
    "sdkLogLevel": "silent"
  }
}
```

---

### 12.3 输出捕获失败

**问题**：测试框架无法捕获输出

**解决方案**：
```bash
# 使用无头模式和禁用颜色
export CLI_MODE=headless
export NO_COLOR=1
modular-agent <command>
```

---

### 12.4 终端窗口问题

**问题**：前台分离模式下终端窗口不显示

**解决方案**：
- Windows：确保终端支持伪终端
- macOS/Linux：检查终端权限
- 使用后台模式替代：`--background`

---

## 十三、相关文档

- [README.md](../README.md) - CLI应用概述
- [headless-mode-design.md](./headless-mode-design.md) - 无头模式设计文档
- [output-and-logging-guide.md](./output-and-logging-guide.md) - 输出和日志指南
- [configuration-design.md](./configuration-design.md) - 配置设计文档
- [storage-integration-design.md](./storage-integration-design.md) - 存储集成设计文档

---

## 十四、总结

Modular Agent CLI应用提供了丰富的模式和选项，以适应不同的使用场景：

- **执行模式**：交互、无头、编程
- **线程执行模式**：前台分离、后台、阻塞
- **Agent Loop模式**：同步、流式、异步
- **输出格式**：表格、JSON、详细
- **存储模式**：JSON、SQLite、内存
- **Skill管理**：完整的Skill生命周期管理

通过合理选择和组合这些模式，可以满足从开发调试到生产部署的各种需求。
