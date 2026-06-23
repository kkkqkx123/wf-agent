# Tool Executors Service

Tool Executors 负责执行各种类型的工具，是 SDK 执行引擎的关键部分。

## 架构

```
services/tools/
├── core/
│   ├── interfaces.ts    # IToolExecutor 接口定义
│   ├── base.ts          # BaseExecutor + ParameterValidator + RetryStrategy + TimeoutController
│   └── types.ts         # 共享类型定义
├── executors/
│   ├── rest.ts          # HTTP/HTTPS API 调用
│   ├── stateless.ts     # JavaScript 函数注册表
│   ├── stateful.ts      # 维持状态的执行器
│   ├── builtin.ts       # 内置工具
│   └── mcp.ts           # Model Context Protocol 服务包装器
├── logger.ts            # 日志工具
├── utils.ts             # 工具定义转换工具
└── index.ts             # 公开导出
```

## 核心概念

### BaseExecutor
所有工具执行器的基类，提供：
- **参数验证** (ParameterValidator) - 基于 Zod schema
- **重试机制** (RetryStrategy) - 指数退避
- **超时控制** (TimeoutController) - 基于信号和定时器
- **结果格式化** - 统一的执行结果结构
- **错误处理** - 工具特定的错误转换

### 执行器类型

| 执行器 | 用途 | 特点 |
|-------|------|------|
| **RestExecutor** | HTTP/HTTPS API 调用 | 支持请求拦截器、响应拦截器、错误处理 |
| **StatelessExecutor** | JavaScript 函数 | 运行时注册函数，无状态 |
| **StatefulExecutor** | JavaScript 函数 + 状态 | 可维护执行上下文状态 |
| **BuiltinExecutor** | 内置工具 | SDK 原生支持的工具 |
| **McpExecutor** | MCP 服务器工具 | 通过 MCP 协议调用远程工具 |

## 依赖关系

```
core/registry/tool-registry.ts
    ↓ (导入所有执行器)
services/tools/
    ├── 核心基类 (BaseExecutor)
    └── 执行器实现 (RestExecutor, StatelessExecutor, etc.)
        ├── 依赖 services/transport/http/ (HTTP 客户端)
        ├── 依赖 services/executors/mcp/ (MCP 核心)
        └── 依赖 core/utils/ (工具库)
```

## 职责边界

✅ **工具执行器负责**：
- 参数验证和转换
- 执行失败重试
- 超时管理
- 错误处理和转换

❌ **工具执行器不负责**：
- 工具注册和管理（→ core/registry/tool-registry.ts）
- 工具审批和权限检查（→ core/coordinators/tool-approval-coordinator.ts）
- 工具结果后处理和状态管理（→ core/state-managers/）

## 使用示例

```typescript
import { RestExecutor, StatelessExecutor } from '@sdk/services/tools'

// HTTP 工具
const restExecutor = new RestExecutor()
const result = await restExecutor.execute(
  {
    id: 'http-tool',
    type: 'REST',
    description: 'Fetch API',
    parameters: { /* ... */ }
  },
  { url: 'https://api.example.com/data' }
)

// JavaScript 函数工具
const jsExecutor = new StatelessExecutor({
  functions: {
    myFunc: (x: number) => x * 2
  }
})
const result = await jsExecutor.execute(
  { id: 'js-tool', /* ... */ },
  { /* ... */ }
)
```

## 扩展执行器

创建新的执行器类型：

```typescript
import { BaseExecutor } from './core/base.js'

export class CustomExecutor extends BaseExecutor {
  override getExecutorType(): string {
    return 'CUSTOM'
  }

  protected override async doExecute(
    tool: Tool,
    parameters: Record<string, unknown>,
    executionId?: string,
    context?: Record<string, unknown>
  ): Promise<unknown> {
    // 实现具体执行逻辑
    return { success: true, data: 'result' }
  }
}
```

## 设计原则

1. **单一职责** - 每个执行器只负责一种工具类型的执行
2. **易于扩展** - 通过继承 BaseExecutor 添加新的执行器
3. **参数验证** - 在执行前进行全面的参数校验
4. **错误安全** - 统一的错误处理和转换
5. **超时安全** - 通过信号和定时器防止长时间运行
