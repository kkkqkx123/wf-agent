# MCP (Model Context Protocol) Executor Module

MCP 执行器模块提供了 Model Context Protocol 服务器的集成和工具执行能力。

## 架构

```
services/executors/mcp/
├── core/                    # 核心连接和状态管理
│   ├── connection-manager.ts
│   ├── mcp-client.ts
│   ├── server-registry.ts
│   ├── connection-state.ts
│   └── index.ts
├── transport/               # 传输层（Stdio、SSE、HTTP）
│   ├── stdio-transport.ts
│   ├── sse-transport.ts
│   ├── http-transport.ts
│   ├── types.ts
│   └── index.ts
├── features/                # 功能模块
│   ├── metadata/           # 元数据导出和缓存
│   ├── registration/       # 动态工具注册
│   ├── approval/           # 审批系统
│   └── analytics/          # 使用分析
├── mcp-server-executor.ts  # 单服务器执行器
├── mcp-executor-factory.ts # 执行器工厂和连接池
├── mcp-connection-processor.ts # 配置处理
├── types.ts                # 共享类型
└── index.ts                # 公开导出
```

## 分层设计

### 1. 核心层 (Core)
**职责**：低级连接管理和状态机
- `McpConnectionManager` - 单个服务器的连接生命周期管理
- `McpClient` - MCP RPC 调用的低级接口
- `McpServerRegistry` - 全局服务器注册表和生命周期管理
- 连接状态和错误历史管理

**特点**：
- 无业务逻辑，仅处理 MCP 协议
- 可独立测试
- 不依赖 Tool Executor

### 2. 传输层 (Transport)
**职责**：MCP 协议通信通道
- `StdioTransport` - 标准输入/输出
- `SseTransport` - Server-Sent Events
- `StreamableHttpTransport` - HTTP 流
- `IMcpTransport` 接口 - 传输抽象

**特点**：
- 插拔式传输方式
- 处理序列化和反序列化
- 事件驱动

### 3. 执行器层 (Executor)
**职责**：高级工具执行和连接管理
- `McpServerExecutor` - 执行单个服务器的工具
- `McpExecutorFactory` - 管理执行器生命周期
- 连接池和重用

**特点**：
- 基于 `BaseRemoteExecutor` 模式
- 参数验证和错误处理
- 与工具注册集成

### 4. 功能模块 (Features)
**职责**：特定功能的增强
- **元数据** - 工具信息导出和缓存
- **注册** - 动态注册 MCP 工具到 SDK
- **审批** - 参数级别的权限控制
- **分析** - 工具使用统计

**特点**：
- 可选功能，可独立启用/禁用
- 不修改核心层

## 依赖关系

```
services/tools/executors/mcp.ts (工具执行器包装)
    ↓
services/executors/mcp/ (MCP 核心)
├── core/ ← 核心连接管理
├── transport/ ← 通信协议
├── mcp-server-executor.ts ← 高级执行器
└── features/ ← 增强功能
    ├── metadata/ → tools registry (动态注册)
    ├── registration/
    ├── approval/
    └── analytics/

core/registry/tool-registry.ts
    ↓ (使用 McpExecutor)
services/tools/executors/mcp.ts
    ↓ (委托给)
services/executors/mcp/McpServerExecutor
```

## 关键类型

### McpServerState
MCP 服务器的状态机：
```
DISCONNECTED → CONNECTING → CONNECTED
     ↑            ↓             ↓
     └────────────┴─────────────┘
            DISCONNECTING
```

### MCP 工具配置
```typescript
interface McpToolConfig {
  serverName: string      // 服务器名称
  toolName: string        // 工具名称
  timeout?: number        // 可选超时
  createCheckpointOnSuccess?: boolean
  checkpointDescriptionTemplate?: string
}
```

## 使用示例

### 配置 MCP 服务器

```typescript
import { loadServerConfigs, createDefaultMcpSettings } from '@sdk/services/executors/mcp'

const settings = await createDefaultMcpSettings({
  servers: {
    'my-server': {
      command: 'npx',
      args: ['my-mcp-server'],
      env: { DEBUG: 'true' }
    }
  }
})
```

### 执行 MCP 工具

```typescript
import { McpServerRegistry } from '@sdk/services/executors/mcp'

const manager = await McpServerRegistry.getInstance()
const result = await manager.callTool('my-server', 'get_weather', {
  location: 'San Francisco'
})
```

## 避免循环导入

⚠️ **关键**：防止以下循环导入
- `core/` 不应导入 `features/`
- `features/metadata` 不应导入执行器
- `mcp-executor-factory.ts` 不应导入 `features/` 中的业务逻辑

使用 **type-only imports** 当导入需要出现循环时：
```typescript
import type { ToolRegistry } from '../../../core/registry'
```

## 设计原则

1. **分层明确** - 核心 → 传输 → 执行 → 功能
2. **依赖单向** - 高层可依赖低层，反之不行
3. **可扩展** - 新传输方式无需修改核心
4. **可测试** - 每层可独立单元测试
5. **类型安全** - 使用 TypeScript 确保类型安全
