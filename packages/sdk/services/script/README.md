# Script Service Module

脚本服务模块提供了多种执行模式的脚本执行能力，包括直接执行、共享会话执行和沙盒隔离执行。

## 架构

```
services/script/
├── engine/                  # 脚本执行引擎
│   ├── script-engine.ts     # 基础脚本引擎
│   ├── script-flow-engine.ts # 流程控制引擎（分支、循环）
│   ├── script-template.ts   # 模板渲染引擎
│   └── index.ts
├── executors/               # 执行模式实现
│   ├── base-executor.ts     # 基础执行器接口
│   ├── direct-executor.ts   # 直接执行模式
│   ├── shared-executor.ts   # 共享会话执行模式
│   ├── sandbox-shell-executor.ts    # 沙盒 Shell 执行
│   ├── sandbox-python-executor.ts   # 沙盒 Python 执行
│   ├── sandbox-javascript-executor.ts # 沙盒 JavaScript 执行
│   └── index.ts
├── resolvers/               # 变量和上下文解析
│   ├── context-resolver.ts
│   └── index.ts
└── index.ts
```

## 分层设计

### 1. 引擎层（Engine）
**职责**：提供脚本执行的核心能力

实现：
- **ScriptEngine** - 基础脚本执行引擎
  - 简单的顺序脚本执行
  - 结果收集和错误处理

- **ScriptFlowEngine** - 流程控制引擎
  - 条件分支 (if/else)
  - 循环控制 (for/while)
  - 脚本编排

- **ScriptTemplateEngine** - 模板渲染引擎
  - 变量插值
  - 条件模板
  - 循环模板

### 2. 执行器层（Executors）
**职责**：在不同的隔离级别执行脚本

#### BaseExecutor 接口
```typescript
interface BaseExecutor {
  execute(script: Script, context: ExecutionContext): Promise<ScriptExecutionResult>
}
```

#### 执行模式对比

| 模式 | 隔离级别 | 速度 | 安全性 | 用途 |
|------|--------|------|-------|------|
| **Direct** | 无隔离 | 最快 | 低 | 开发/调试 |
| **Shared** | 进程隔离 | 快 | 中 | 生产环境 |
| **Sandbox-Shell** | 容器隔离 | 慢 | 高 | 不信任脚本 |
| **Sandbox-Python** | VM 隔离 | 慢 | 高 | Python 脚本 |
| **Sandbox-JS** | VM 隔离 | 慢 | 高 | JavaScript 脚本 |

**直接执行 (Direct)**
- 在 Node.js 进程中直接执行
- 最小开销，最快性能
- 无隔离，可访问完整环境
- 仅用于开发/信任的代码

**共享执行 (Shared)**
- 通过 Terminal Service 的持久会话执行
- 性能中等，可维护会话状态
- 进程级隔离，适合生产环境
- 支持跨脚本共享变量

**沙盒执行 (Sandbox-*)**
- 在沙盒运行时执行
- 最安全，隔离最强
- 性能开销大
- 用于不信任或不安全的代码

### 3. 解析层（Resolvers）
**职责**：解析脚本中的变量和上下文

实现：
- **ContextResolver** - 解析执行上下文
- 变量插值
- 条件解析

## 依赖关系

```
core/executors/script-executor.ts（高级编排）
    ↓ (选择执行模式)
services/script/engine/script-engine.ts
    ↓ (委托给)
services/script/executors/
├── direct-executor.ts（无隔离）
│   └── Node.js eval
├── shared-executor.ts（进程隔离）
│   └── services/terminal/ (持久会话)
├── sandbox-shell-executor.ts（容器隔离）
│   └── services/sandbox/ (Shell 运行时)
├── sandbox-python-executor.ts（VM 隔离）
│   └── services/sandbox/ (Python 运行时)
└── sandbox-javascript-executor.ts（VM 隔离）
    └── services/sandbox/ (JavaScript 运行时)
```

## 执行模式对比

### Direct 执行
```typescript
const executor = new DirectExecutor(terminalService)
const result = await executor.execute({
  type: 'shell',
  content: 'echo "Hello"'
})
// 直接在 Node.js 中运行，最快但无隔离
```

### Shared 执行
```typescript
const executor = new SharedExecutor(terminalService)
const result = await executor.execute({
  type: 'shell',
  content: 'export VAR=value; echo $VAR'
})
// 通过持久 Terminal 会话运行，支持跨脚本状态
```

### Sandbox 执行
```typescript
const executor = new SandboxShellExecutor(terminalService)
const result = await executor.execute({
  type: 'shell',
  content: 'rm -rf /',  // 会被沙盒阻止
  timeout: 5000
})
// 在隔离的沙盒中运行，安全但慢
```

## 流程控制示例

### ScriptFlowEngine
```typescript
import { ScriptFlowEngine } from '@sdk/services/script'

const engine = new ScriptFlowEngine()

const result = await engine.execute({
  type: 'flow',
  branches: [
    {
      condition: 'count > 10',
      script: { content: 'echo "Large"' }
    },
    {
      script: { content: 'echo "Small"' }
    }
  ]
}, { count: 15 })

// 输出: "Large"
```

### ScriptTemplateEngine
```typescript
import { ScriptTemplateEngine } from '@sdk/services/script'

const engine = new ScriptTemplateEngine()

const result = await engine.render(
  'echo "Hello {{name}}, you have {{count}} items"',
  { name: 'Alice', count: 5 }
)

// 输出: "echo "Hello Alice, you have 5 items""
```

## 执行生命周期

```
1. 解析 (Parse)
   ↓
2. 验证 (Validate)
   ├─ 权限检查
   ├─ 语法验证
   └─ 资源检查
   ↓
3. 执行 (Execute)
   ├─ 选择执行模式
   ├─ 设置超时
   ├─ 运行脚本
   └─ 捕获输出
   ↓
4. 清理 (Cleanup)
   ├─ 关闭资源
   ├─ 记录日志
   └─ 更新状态
```

## 安全特性

1. **隔离执行** - 沙盒保护宿主环境
2. **超时控制** - 防止无限循环
3. **资源限制** - 限制内存和 CPU 使用
4. **权限控制** - 细粒度的命令白名单
5. **输出安全** - 清理敏感信息

## 使用场景

### 场景 1: 快速脚本测试
```typescript
const executor = new DirectExecutor()
await executor.execute({ type: 'shell', content: 'ls -la' })
```

### 场景 2: 持久会话执行
```typescript
const executor = new SharedExecutor()
// 脚本 1 设置变量
await executor.execute({ type: 'shell', content: 'export MY_VAR=value' })
// 脚本 2 使用之前的变量
await executor.execute({ type: 'shell', content: 'echo $MY_VAR' })
```

### 场景 3: 不信任代码执行
```typescript
const executor = new SandboxShellExecutor()
// 即使脚本包含危险命令也能安全执行
await executor.execute({ 
  type: 'shell', 
  content: userProvidedScript,
  timeout: 5000
})
```

## 性能特征

- **Direct 模式**: 10ms 基础延迟
- **Shared 模式**: 50ms 基础延迟
- **Sandbox 模式**: 200ms+ 基础延迟

## 设计原则

1. **分层清晰** - 引擎 → 执行器 → 实现
2. **模式独立** - 支持多种隔离级别
3. **安全可靠** - 多层安全防护
4. **高效可扩展** - 共享会话节省资源
5. **易于集成** - 统一的执行接口
