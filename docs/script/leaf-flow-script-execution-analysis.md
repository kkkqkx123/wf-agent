# Leaf-flow 脚本执行设计借鉴分析

## 概述

本文档分析 [leaf-flow](../ref/leaf-flow) 项目的架构设计，评估哪些思路可以借鉴到当前项目（Modular Agent Framework）的脚本执行模块设计中。

Leaf-flow 是一个基于 **Bud 蓝图系统** 的自动化执行引擎，支持通过 YAML 定义项目模块（Project）和流程（Flow），并通过多层执行器（SharedExec / PtyExec / DirectExec）执行 shell 命令。

---

## 一、Leaf-flow 核心架构概览

### 1.1 双层蓝图系统（Bud）

Leaf-flow 提出 **Bud（蓝图）** 概念，分为两层：

| 层次 | 文件 | 职责 |
|------|------|------|
| **Project** | `*.yaml` | 定义可复用的模块（Module），每个模块包含命令模板+参数声明 |
| **Flow** | `*.flow.yaml` | 引用 Project 中的模块，组成多分支执行流程，支持参数覆盖 |

**Project 示例（简化）：**

```yaml
name: 基础项目演示
modules:
  - key: string-input
    template: 'echo "hello #{name}"'
    arguments:
      - key: name
        dtype: string
        value: default
        method: select    # 输入方式：select/input/radio 等
        options: [A, B]
```

**Flow 示例（简化）：**

```yaml
name: 基础流
branches:
  - key: branch-1
    modules:
      - key: demo.string-input           # 引用 project.module
        arguments:
          - key: name                     # 覆盖参数
            value: override
```

### 1.2 命令模板与参数替换

Leaf-flow 使用 `#{varname}` 语法在命令模板中标记参数位置：

```
template: 'echo "你好: #{name}"'
```

运行时，执行引擎将参数值替换到模板中，生成最终命令。这类似于 Handlebars/Mustache 风格。

### 1.3 三层执行器体系

| 执行器 | Shell 配置 | 特点 |
|--------|-----------|------|
| **SharedExecutor** | `shell: powershell` / `bash` | 共享 Shell 进程，通过 stdin 写入命令，支持状态保持 |
| **PtyExecutor** | `shell: pty:bash` | 伪终端，支持交互式命令，可处理 Ctrl+C |
| **Executor** | `shell: no` | 直接执行，每条命令独立进程 |

### 1.4 任务队列（TaskQueue）

任务队列管理执行的生命周期：

```
AddTask → process() → 逐条执行 → 广播状态/日志
```

- 串行执行，支持取消
- 通过 SSE 广播日志到前端
- 支持进度条渲染（`\r` 覆盖）

### 1.5 mmap 数据传递

Leaf-flow 使用 **内存映射文件（mmap）** 在模块间传递数据：

- 固定大小的临时文件（`temp.leaf.mmap`）
- 模块通过 `#{mmap}` 占位符获取路径
- Python 脚本可通过 `sys.stdin` / `mmap` 直接读写

---

## 二、当前项目脚本执行现状

### 2.1 现有实现

当前项目的脚本执行节点流程：

```
Script Node (workflow)
  → scriptHandler (node-handler)
    → ScriptRegistry.execute(scriptName)
      → ScriptExecutor.execute(script, options)
        → TerminalService.executeOneOff(command, options)
```

**核心组件：**

| 组件 | 位置 | 职责 |
|------|------|------|
| `Script` | `packages/types/src/script/script.ts` | 脚本类型定义 |
| `ScriptExecutor` | `sdk/core/executors/script-executor.ts` | 执行器，调用 TerminalService |
| `scriptHandler` | `sdk/workflow/execution/handlers/node-handlers/script-handler.ts` | Workflow 节点处理器 |
| `ScriptRegistry` | `sdk/core/registry/script-registry.ts` | 脚本注册、管理 |

### 2.2 现有表达式/DSL/变量系统

项目已有完善的表达式评估体系：

| 组件 | 位置 | 能力 |
|------|------|------|
| **表达式引擎** | `sdk/workflow/evaluation/` | AST 编译、求值、安全验证 |
| **DSL 解析器** | `sdk/workflow/evaluation/dsl/` | Chevrotain 词法/语法分析 |
| **条件评估器** | `sdk/workflow/evaluation/condition-evaluator.ts` | 条件表达式评估 |
| **模板渲染器** | `packages/common-utils/src/template/template-renderer.ts` | `{{var}}` 替换、`{{#if}}`、`{{#each}}` |
| **变量管理器** | Workflow 执行上下文 | 类型化变量定义、作用域管理 |
| **配置解析器** | `sdk/api/shared/config/` | TOML/JSON 解析、参数替换 |

### 2.3 现有脚本执行的不足

1. **无参数模板**：Script 定义仅有 `content` 字段，不支持参数占位符
2. **无输入变量注入**：无法将 workflow 变量注入到脚本命令中
3. **单命令模式**：每个脚本节点只能执行一条命令，不支持多步骤
4. **无执行流程编排**：没有 Flow 概念，脚本间只能通过 workflow 边连接
5. **无 Shell 类型选择**：默认使用系统 Shell，不支持指定 PowerShell/bash/cmd
6. **无交互支持**：不支持需要用户输入的交互式命令
7. **无数据传递机制**：脚本间没有 mmap 或类似的数据共享设施

---

## 三、可借鉴的设计思路

### 3.1 蓝图化脚本定义（借鉴 Bud 体系）

Leaf-flow 的 **Project + Module** 双层结构适合改造为当前项目的 **Script Blueprint**：

**设计思路：**

```toml
# scripts/example.toml
[meta]
name = "data-processor"
version = "1.0.0"

[script]
template = '''
echo "processing {{input.name}}"
python process.py --input {{input.file}} --output {{mmap.path}}
'''

[script.arguments]
# 运行时可注入的参数声明
name = { type = "string", required = true, desc = "Name for processing" }
file = { type = "string", required = true, desc = "Input file path" }

[script.environment]
# 预定义的静态环境变量
PYTHONUTF8 = "1"
```

**优势：**

- 将脚本参数声明与脚本内容分离，类似于函数签名
- 支持运行时注入，workflow 可通过变量系统传递参数
- 参数声明可以用于 UI 自动生成输入表单（类似 Leaf-flow 前端）

**可效仿程度：★★★★★**

### 3.2 多分支 Flow 编排（借鉴 Bud Flow）

Leaf-flow 的 **Flow** 概念允许组织多分支执行流程。当前项目虽然有 workflow 编排，但可以借鉴 **脚本级 Flow**：

**设计思路：**

```toml
[flow]
name = "deploy-pipeline"

[[flow.branches]]
key = "build"
modules = [
  "project-a.compile",
  "project-a.test",
  { key = "project-a.package", args = { format = "zip" } }
]

[[flow.branches]]
key = "deploy"
depends_on = "build"
modules = [
  { key = "project-b.deploy", args = { env = "staging" } }
]
```

**与现有 workflow 的关系：**

- 当前 workflow 是**图结构**编排（nodes + edges）
- Flow 是**线性/树形结构**编排，更接近脚本执行场景
- 可以将 Flow 作为 workflow 中 **SCRIPT 节点的子编排层**，或作为独立执行单元

**可效仿程度：★★★★☆**

### 3.3 Shell 执行器体系（借鉴 Executor 三层架构）

Leaf-flow 的三种执行器设计清晰：

| 执行器类型 | 含义 | 适用场景 |
|-----------|------|---------|
| `shell: no` | 独立进程，每条命令 `exec.Command` | 一次性脚本，隔离性要求高 |
| `shell: powershell` | 共享 Shell 进程，stdin 写入 | 需要状态保持的连续命令 |
| `shell: pty:bash` | 伪终端 | 交互式命令，需要 TTY |

**当前项目**的 `TerminalService.executeOneOff` 仅支持 `shell: no` 模式。

**设计思路：**

```toml
[script.executor]
type = "shared"          # shared | pty | direct
shell = "powershell"     # powershell | bash | cmd
```

**优势：**

- `direct` 模式与当前行为兼容，无需改动
- `shared` 模式支持多条命令在同一个 Shell 会话中执行，保持状态（cd, 变量）
- `pty` 模式支持交互式脚本（如需要用户确认的部署脚本）

**可效仿程度：★★★★★**

### 3.4 命令模板变量替换（借鉴 `#{}` + 当前 `{{}}`）

Leaf-flow 使用 `#{varname}` 在命令模板中标记参数位置。当前项目已有 `{{var}}` 模板语法和表达式引擎。

**设计思路：**

沿用当前项目的 `{{}}` 模板语法（已有完整实现），增加脚本模板功能：

```
template = "echo {{input.name}} && python {{input.script}} --mode {{input.mode}}"
```

通过 `renderTemplate()` 将 workflow 变量注入到脚本命令中。

**变量来源：**

| 来源 | 前缀 | 示例 |
|------|------|------|
| Workflow 变量 | `variables.` | `{{variables.userName}}` |
| 脚本参数 | `input.` | `{{input.filePath}}` |
| 环境变量 | `env.` | `{{env.PATH}}` |
| mmap 路径 | `mmap.` | `{{mmap.path}}` |
| 节点输出 | `output.` | `{{output.previousNode.result}}` |

**可效仿程度：★★★★★**

### 3.5 参数声明系统（借鉴 Leaf-flow arguments）

Leaf-flow 的参数声明系统：

```yaml
arguments:
  - key: basic_string
    name: 基础字符串
    dtype: string          # 类型：string/number/boolean/file/directory
    value: 默认值
    method: select         # 输入方式：select/input/radio/checkbox
    multiple: true         # 是否多值
    options: [A, B]        # select 选项
    template: "#{#{}, }"   # 多值模板
```

**设计思路：**

```toml
[script.arguments.name]
type = "string"
required = true
default = "default-value"
description = "用户名"
```

参数声明的好处：
1. **类型安全**：运行时校验参数类型
2. **UI 自动生成**：前端可以根据声明渲染表单
3. **文档自描述**：声明即文档
4. **模板替换**：声明中的 `type` 影响模板替换行为（如 number 的格式化）

**可效仿程度：★★★★☆**

### 3.6 mmap 数据传递（借鉴 Leaf-flow mmap）

Leaf-flow 的 mmap 机制用于大数据在模块间传递：

```
Module A 输出数据 → mmap 文件 → Module B 读取
```

**设计思路：**

当前项目可以引入**临时文件传递**机制，作为脚本间数据交换的补充方式：

```toml
[script.io]
input = "stdin"       # stdin | file | mmap
output = "stdout"     # stdout | file | mmap
```

- `input = "stdin"`：从 stdin 读取输入数据（JSON）
- `input = "file"`：从临时文件读取
- `output = "stdout"`：从 stdout 解析输出
- `output = "file"`：输出到临时文件

**优势：**

- 解决大数据传递问题（环境变量有大小限制）
- 支持 Python/node 脚本通过文件交换结构化数据
- 临时文件自动清理，无需用户管理

**可效仿程度：★★★☆☆**

### 3.7 任务队列与状态广播（借鉴 TaskQueue）

Leaf-flow 的 TaskQueue 设计：

```go
type TaskQueue struct {
    tasks        []*Task
    executor     IExecutor
    mutex        sync.Mutex
    isRunning    bool
}
```

- 串行执行队列
- 通过 SSE 广播标准输出/标准错误
- 支持取消操作

**设计思路：**

当前项目的 TerminalService 可以增加：

1. **命令序列执行**：支持多条命令顺序执行，而非仅单条
2. **执行状态回调**：通过事件或回调反馈执行进度
3. **标准输出流式输出**：支持长时间运行脚本的实时日志

**可效仿程度：★★★☆☆**

### 3.8 Dynamic Bind（借鉴动态绑定）

Leaf-flow 在执行时动态解析参数值，支持：

- 从 mmap 读取数据
- 从其他模块输出中获取值
- 选择文件路径等

**设计思路：**

结合当前项目的表达式引擎，脚本参数支持动态绑定：

```toml
[script.arguments.outputPath]
type = "string"
value = "{{output.previousStep.result}}"  # 动态绑定到前一个节点的输出
```

**可效仿程度：★★★★☆**

---

## 四、综合设计方案建议

### 4.1 架构分层

```
┌─────────────────────────────────────────┐
│           Workflow 编排层                │
│  (nodes + edges, WorkflowEngine)        │
├─────────────────────────────────────────┤
│           Script Flow 层                 │
│  (Flow 定义：多分支、多步骤编排)          │
├─────────────────────────────────────────┤
│           Script 定义层                  │
│  (TOML 定义：模板、参数、Shell 类型)      │
├─────────────────────────────────────────┤
│           Script 执行层                  │
│  (ScriptExecutor + ShellExecutor)       │
├─────────────────────────────────────────┤
│           Shell 适配器                   │
│  (direct / shared / pty)               │
└─────────────────────────────────────────┘
```

### 4.2 配置格式（TOML）

借鉴 Leaf-flow 的 YAML 配置，改造为当前项目熟悉的 TOML 格式（已有 TOML 解析能力）：

**Script 定义：**

```toml
[meta]
name = "deploy-script"
version = "1.0.0"
description = "部署脚本"

[executor]
type = "shared"
shell = "powershell"

[script]
template = '''
Write-Host "Deploying {{input.projectName}}"
cd {{input.workDir}}
./deploy.ps1 -Env {{input.env}}
'''

[script.arguments.projectName]
type = "string"
required = true
description = "项目名称"

[script.arguments.env]
type = "string"
default = "staging"
options = ["dev", "staging", "production"]

[script.arguments.workDir]
type = "string"
default = "./"

[script.environment]
NODE_ENV = "{{input.env}}"

[script.io]
input = "stdin"
output = "stdout"
```

**Flow 定义：**

```toml
[flow]
name = "ci-pipeline"

[[flow.branches]]
key = "lint"
modules = [
  "tools.eslint",
  "tools.prettier"
]

[[flow.branches]]
key = "test"
modules = [
  { key = "tools.unit-test", args = { coverage = true } },
  { key = "tools.integration-test" }
]

[[flow.branches]]
key = "build"
depends_on = ["lint", "test"]
modules = [
  { key = "tools.build", args = { target = "production" } }
]
```

### 4.3 执行器类型定义

```toml
# 直接执行（当前模式，兼容）
[executor]
type = "direct"
shell = "auto"  # 自动检测

# 共享 Shell 会话
[executor]
type = "shared"
shell = "powershell"

# 伪终端
[executor]
type = "pty"
shell = "bash"
```

---

## 五、可效仿程度总结

| 设计思路 | 效仿程度 | 已有基础 | 实施难度 |
|---------|---------|---------|---------|
| 蓝图化脚本定义（Module） | ★★★★★ | TOML 解析器 | 低 |
| 参数声明系统 | ★★★★★ | 类型系统 + 模板引擎 | 低 |
| 命令模板变量替换 | ★★★★★ | `renderTemplate()` | 低 |
| Dynamic Bind | ★★★★☆ | 表达式引擎 | 中 |
| Shell 执行器体系 | ★★★★☆ | TerminalService | 中 |
| 多分支 Flow 编排 | ★★★★☆ | Workflow 引擎 | 中高 |
| mmap 数据传递 | ★★★☆☆ | 无 | 高 |
| 任务队列与状态广播 | ★★★☆☆ | Event System | 中 |

### 优先级建议

**Phase 1（低难度，高价值）：**

1. 脚本参数声明 + 模板变量替换（利用现有 `renderTemplate`）
2. 脚本 TOML 定义格式（利用现有 TOML 解析）
3. 支持 Workflow 变量注入到脚本命令

**Phase 2（中等难度）：**

4. Shell 执行器类型选择（direct / shared）
5. 动态绑定参数到表达式引擎
6. Flow 定义 + 执行引擎

**Phase 3（高难度）：**

7. 交互式脚本（PTY）支持
8. mmap 文件数据传递
9. 实时日志流式广播

---

## 六、关键文件参考

| 文件 | 说明 |
|------|------|
| `ref/leaf-flow/docs/architecture.md` | Leaf-flow 整体架构 |
| `ref/leaf-flow/bud/leaf/demo.yaml` | Project 定义示例 |
| `ref/leaf-flow/bud/sprig/demo.flow.yaml` | Flow 定义示例 |
| `ref/leaf-flow/scheduler/execution/task_queue.go` | 任务队列实现 |
| `ref/leaf-flow/scheduler/execution/exec.go` | DirectExecutor |
| `ref/leaf-flow/scheduler/execution/exec_shared.go` | SharedExecutor |
| `ref/leaf-flow/scheduler/execution/mmap.go` | mmap 实现 |
| `sdk/workflow/evaluation/expression-evaluator.ts` | 表达式引擎 |
| `sdk/workflow/evaluation/condition-evaluator.ts` | 条件评估器 |
| `packages/common-utils/src/template/template-renderer.ts` | 模板渲染器 |
| `sdk/core/executors/script-executor.ts` | 当前脚本执行器 |
| `packages/types/src/script/script.ts` | 脚本类型定义 |
| `sdk/api/shared/config/parsers/toml-parser.ts` | TOML 解析器 |