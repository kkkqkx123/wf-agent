# CLI 输出与日志使用规范

## 一、架构概述

CLI 应用使用统一的输出管理系统，分离三种输出流：

```
┌─────────────────────────────────────────────────────────┐
│                      CLIOutput                          │
├─────────────────┬─────────────────┬─────────────────────┤
│    stdout       │    stderr       │    log file         │
│    用户可见      │    错误信息      │    调试日志          │
│    可重定向      │    可重定向      │    持久化存储        │
└─────────────────┴─────────────────┴─────────────────────┘
```

## 二、导入方式

```typescript
import { getOutput, type CLIOutput } from "../utils/output.js";

const output = getOutput();
```

## 三、输出方法分类

### 3.1 用户输出 (stdout)

| 方法 | 用途 | 示例 |
|------|------|------|
| `output(content)` | 输出内容到 stdout | `output.output("操作完成")` |
| `write(content)` | 输出不带换行 | `output.write("处理中...")` |
| `stream(content)` | 流式输出（无换行） | `output.stream(event.delta)` |
| `newLine()` | 输出空行 | `output.newLine()` |
| `success(msg)` | 成功消息 ✓ | `output.success("创建成功")` |
| `info(msg)` | 信息消息 ℹ | `output.info("请稍候")` |
| `warn(msg)` | 警告消息 ⚠ | `output.warn("即将删除")` |

### 3.2 错误输出 (stderr)

| 方法 | 用途 | 示例 |
|------|------|------|
| `error(content)` | 输出到 stderr | `output.error("发生错误")` |
| `fail(msg)` | 失败消息 ✗ | `output.fail("操作失败")` |
| `errorWithLabel(label, msg)` | 带标签错误 | `output.errorWithLabel("ERROR", "连接失败")` |

### 3.3 日志输出 (log file)

| 方法 | 用途 | 条件 |
|------|------|------|
| `log(level, msg, ctx)` | 写日志文件 | 始终写入 |
| `infoLog(msg, ctx)` | INFO 级别日志 | 始终写入 |
| `warnLog(msg, ctx)` | WARN 级别日志 | 始终写入 |
| `errorLog(msg, ctx)` | ERROR 级别日志 | 始终写入 |
| `verboseLog(msg, ctx)` | 详细日志 | `--verbose` 或 `--debug` |
| `debugLog(msg, ctx)` | 调试日志 | `--debug` |

### 3.4 格式化输出

| 方法 | 用途 | 示例 |
|------|------|------|
| `json(data)` | JSON 格式输出 | `output.json(result)` |
| `table(headers, rows)` | 表格输出 | `output.table(["ID", "名称"], rows)` |
| `bulletList(items)` | 无序列表 | `output.bulletList(["a", "b"])` |
| `numberedList(items)` | 有序列表 | `output.numberedList(["a", "b"])` |
| `section(title)` | 章节标题 | `output.section("详情")` |
| `subsection(title)` | 子章节标题 | `output.subsection("配置")` |
| `keyValue(k, v)` | 键值对 | `output.keyValue("ID", "123")` |
| `keyValuePairs(pairs)` | 多键值对 | `output.keyValuePairs({a: "1", b: "2"})` |

## 四、使用场景指南

### 4.1 命令结果输出

```typescript
// 成功结果
output.success("工作流已创建");
output.keyValue("  ID", workflow.id);

// 列表输出
const workflows = await adapter.listWorkflows();
if (options.table) {
  output.table(["ID", "名称", "状态"], rows);
} else {
  workflows.forEach(w => output.writeLine(output.workflow(w)));
}

// JSON 输出
if (options.verbose) {
  output.json(workflow);
}
```

### 4.2 错误处理

```typescript
// 用户可见错误
output.fail("配置验证失败:");
errors.forEach(err => output.writeLine(`  - ${err}`));

// 内部错误日志
output.errorLog(`操作失败: ${error.message}`, { error: String(error) });
```

### 4.3 操作日志

```typescript
// 记录操作开始
output.infoLog(`正在注册工作流: ${file}`);

// 记录操作完成
output.infoLog(`工作流已注册: ${workflow.id}`);

// 记录警告
output.warnLog(`即将删除工作流: ${id}`);
```

### 4.4 流式输出

```typescript
// Agent 流式响应
adapter.executeAgentLoopStream(config, {}, event => {
  if (event.type === "text") {
    output.stream(event.delta);
  } else if (event.type === "tool_call_start") {
    output.newLine();
    output.writeLine(`[工具调用] ${event.data?.toolCall?.function?.name}`);
  }
});
```

## 五、初始化流程

### 5.1 入口文件 (index.ts)

```typescript
import { initializeOutput, getOutput } from "./utils/output.js";
import { initLogger, initSDKLogger } from "./utils/logger.js";

program
  .option("-v, --verbose", "详细输出")
  .option("-d, --debug", "调试模式")
  .option("-l, --log-file <path>", "日志文件路径")
  .hook("preAction", async (thisCommand) => {
    const opts = thisCommand.opts();

    // 1. 初始化输出系统
    initializeOutput({
      logFile: opts.logFile,
      verbose: opts.verbose,
      debug: opts.debug,
    });

    // 2. 初始化日志系统
    initLogger(opts);
    initSDKLogger(opts);
  });
```

### 5.2 退出处理

```typescript
program.hook("postAction", async () => {
  const output = getOutput();
  await output.close();
});

process.on("SIGINT", async () => {
  await getOutput().close();
  process.exit(0);
});
```

## 六、日志文件

### 6.1 默认位置

```
logs/cli-app-{YYYY-MM-DD}.log
```

### 6.2 日志格式

```
[2026-04-06T10:30:00.000Z] [INFO] 工作流已注册: abc-123
[2026-04-06T10:30:05.000Z] [ERROR] 操作失败: 连接超时 {"code": "TIMEOUT"}
```

### 6.3 自定义日志路径

```bash
modular-agent workflow list --log-file /path/to/custom.log
```

## 七、最佳实践

### 7.1 输出选择原则

- **用户需要看到的结果** → `stdout` (output, success, info)
- **错误和警告** → `stderr` (error, fail)
- **调试和审计信息** → `log file` (infoLog, errorLog)

### 7.2 日志级别选择

| 级别 | 使用场景 |
|------|----------|
| `infoLog` | 常规操作记录 |
| `warnLog` | 潜在问题、确认提示 |
| `errorLog` | 错误和异常 |
| `verboseLog` | 详细操作步骤（需 --verbose） |
| `debugLog` | 调试信息（需 --debug） |

### 7.3 避免的做法

```typescript
// ❌ 不要直接使用 console
console.log("结果");  // 绕过输出系统

// ❌ 不要混用输出目标
output.output("错误信息");  // 错误信息应该用 stderr

// ❌ 不要在日志中使用用户数据
output.infoLog(`用户输入: ${userInput}`);  // 可能泄露敏感信息
```

## 八、API 速查表

```typescript
// 获取输出实例
const output = getOutput();

// 用户输出
output.output("普通消息");
output.success("成功");
output.info("提示");
output.warn("警告");

// 错误输出
output.error("错误");
output.fail("失败");

// 日志记录
output.infoLog("操作日志");
output.warnLog("警告日志");
output.errorLog("错误日志");
output.verboseLog("详细日志");  // 需要 -v
output.debugLog("调试日志");    // 需要 -d

// 格式化
output.json(data);
output.table(headers, rows);
output.keyValue("键", "值");
output.section("标题");
output.subsection("子标题");
```
