# Tool Executors 架构设计

## 📐 整体架构

### 分层架构

```
┌─────────────────────────────────────────────────────────┐
│                    Apps Layer                            │
│  (web-app, cli-app, etc.)                                │
└────────────────────┬────────────────────────────────────┘
                     │ 使用
┌────────────────────▼────────────────────────────────────┐
│              SDK Layer (适配器层)                        │
│  - BaseToolExecutor (抽象基类)                           │
│  - McpToolExecutor (适配器)                              │
│  - RestToolExecutor (适配器)                             │
│  - StatefulToolExecutor (适配器)                         │
│  - StatelessToolExecutor (适配器)                        │
└────────────────────┬────────────────────────────────────┘
                     │ 调用
┌────────────────────▼────────────────────────────────────┐
│         Tool Executors Layer (实现层)                    │
│  - McpExecutor (核心实现)                                │
│  - RestExecutor (核心实现)                               │
│  - StatefulExecutor (核心实现)                           │
│  - StatelessExecutor (核心实现)                          │
│  - Transport implementations                             │
└────────────────────┬────────────────────────────────────┘
                     │ 依赖
┌────────────────────▼────────────────────────────────────┐
│              SDK Core Layer (核心层)                     │
│  - Types (Tool, ToolConfig, etc.)                       │
│  - HTTP Transport (HttpTransport, SseTransport)          │
│  - Errors (ToolError, NetworkError, etc.)               │
│  - Execution Context (ThreadContext)                    │
└─────────────────────────────────────────────────────────┘
```

### 模块依赖图

```mermaid
graph TB
    subgraph "Apps"
        APP1[web-app]
        APP2[cli-app]
    end
    
    subgraph "SDK - Tools Module"
        SDK_BASE[BaseToolExecutor]
        SDK_MCP[McpToolExecutor]
        SDK_REST[RestToolExecutor]
        SDK_STATEFUL[StatefulToolExecutor]
        SDK_STATELESS[StatelessToolExecutor]
    end
    
    subgraph "Tool Executors Package"
        EXEC_MCP[McpExecutor]
        EXEC_REST[RestExecutor]
        EXEC_STATEFUL[StatefulExecutor]
        EXEC_STATELESS[StatelessExecutor]
        
        subgraph "MCP Transports"
            TRANS_STDIO[StdioTransport]
            TRANS_SSE[SseTransport]
            SESSION[McpSession]
        end
        
        subgraph "REST Implementation"
            HTTP_CLIENT[HttpClient]
        end
        
        subgraph "Stateful Implementation"
            INSTANCE_MGR[InstanceManager]
        end
        
        subgraph "Stateless Implementation"
            FUNC_WRAPPER[FunctionWrapper]
        end
    end
    
    subgraph "SDK - Core"
        SDK_TYPES[Types]
        SDK_HTTP[HTTP Transport]
        SDK_ERRORS[Errors]
        SDK_CONTEXT[ThreadContext]
    end
    
    APP1 --> SDK_MCP
    APP1 --> SDK_REST
    APP2 --> SDK_STATEFUL
    APP2 --> SDK_STATELESS
    
    SDK_MCP --> EXEC_MCP
    SDK_REST --> EXEC_REST
    SDK_STATEFUL --> EXEC_STATEFUL
    SDK_STATELESS --> EXEC_STATELESS
    
    EXEC_MCP --> TRANS_STDIO
    EXEC_MCP --> TRANS_SSE
    EXEC_MCP --> SESSION
    
    EXEC_REST --> HTTP_CLIENT
    EXEC_STATEFUL --> INSTANCE_MGR
    EXEC_STATELESS --> FUNC_WRAPPER
    
    TRANS_SSE --> SDK_HTTP
    
    SDK_BASE --> SDK_TYPES
    SDK_BASE --> SDK_ERRORS
    SDK_MCP --> SDK_TYPES
    SDK_REST --> SDK_TYPES
    SDK_STATEFUL --> SDK_TYPES
    SDK_STATEFUL --> SDK_CONTEXT
    SDK_STATELESS --> SDK_TYPES
    
    EXEC_MCP --> SDK_TYPES
    EXEC_REST --> SDK_TYPES
    EXEC_STATEFUL --> SDK_TYPES
    EXEC_STATEFUL --> SDK_CONTEXT
    EXEC_STATELESS --> SDK_TYPES
    
    style SDK_MCP fill:#e1f5ff
    style EXEC_MCP fill:#fff4e1
    style SDK_HTTP fill:#f0f0f0
```

## 🎯 MCP执行器架构

### MCP传输层设计

```mermaid
graph TB
    subgraph "MCP Executor"
        MCP_EXEC[McpExecutor]
    end
    
    subgraph "Transport Interface"
        TRANS_IFACE[McpTransport Interface]
    end
    
    subgraph "Transport Implementations"
        STDIO[StdioTransport]
        SSE[SseTransport]
    end
    
    subgraph "Session Management"
        SESSION[McpSession]
    end
    
    subgraph "SDK HTTP Layer"
        HTTP[HttpTransport]
        SSE_BASE[SseTransport]
    end
    
    subgraph "MCP Server"
        SERVER1[Server 1<br/>Stdio]
        SERVER2[Server 2<br/>SSE]
    end
    
    MCP_EXEC --> TRANS_IFACE
    TRANS_IFACE --> STDIO
    TRANS_IFACE --> SSE
    
    STDIO --> SESSION
    SSE --> SESSION
    
    STDIO --> SERVER1
    SSE --> SERVER2
    
    SSE --> SSE_BASE
    
    style MCP_EXEC fill:#e1f5ff
    style TRANS_IFACE fill:#fff4e1
    style SSE_BASE fill:#f0f0f0
```

### MCP消息流

```mermaid
sequenceDiagram
    participant App as Application
    participant SDK as SDK Adapter
    participant Exec as McpExecutor
    participant Trans as Transport
    participant Server as MCP Server
    
    App->>SDK: execute(tool, params)
    SDK->>Exec: doExecute(tool, params)
    Exec->>Trans: execute(toolName, params)
    
    alt Stdio Mode
        Trans->>Server: JSON-RPC via stdin
        Server-->>Trans: JSON-RPC via stdout
    else SSE Mode
        Trans->>Server: HTTP POST
        Server-->>Trans: HTTP Response
    end
    
    Trans-->>Exec: result
    Exec-->>SDK: result
    SDK-->>App: ToolExecutionResult
```

## 🔌 REST执行器架构

### REST执行器设计

```mermaid
graph TB
    subgraph "REST Executor"
        REST_EXEC[RestExecutor]
    end
    
    subgraph "HTTP Client"
        HTTP_CLIENT[HttpClient]
    end
    
    subgraph "SDK HTTP Layer"
        HTTP[HttpTransport]
    end
    
    subgraph "External APIs"
        API1[API 1]
        API2[API 2]
    end
    
    REST_EXEC --> HTTP_CLIENT
    HTTP_CLIENT --> HTTP
    HTTP --> API1
    HTTP --> API2
    
    style REST_EXEC fill:#e1f5ff
    style HTTP_CLIENT fill:#fff4e1
    style HTTP fill:#f0f0f0
```

### REST请求流程

```mermaid
sequenceDiagram
    participant App as Application
    participant SDK as SDK Adapter
    participant Exec as RestExecutor
    participant Client as HttpClient
    participant HTTP as HttpTransport
    participant API as External API
    
    App->>SDK: execute(tool, params)
    SDK->>Exec: doExecute(tool, params)
    Exec->>Client: request(url, options)
    Client->>HTTP: execute(url, options)
    HTTP->>API: HTTP Request
    API-->>HTTP: HTTP Response
    HTTP-->>Client: TransportResponse
    Client-->>Exec: formatted result
    Exec-->>SDK: result
    SDK-->>App: ToolExecutionResult
```

## 🧩 Stateful执行器架构

### Stateful执行器设计

```mermaid
graph TB
    subgraph "Stateful Executor"
        STATEFUL_EXEC[StatefulExecutor]
    end
    
    subgraph "Instance Management"
        INSTANCE_MGR[InstanceManager]
    end
    
    subgraph "SDK Context"
        CONTEXT[ThreadContext]
    end
    
    subgraph "Tool Instances"
        INST1[Tool Instance 1]
        INST2[Tool Instance 2]
    end
    
    STATEFUL_EXEC --> INSTANCE_MGR
    INSTANCE_MGR --> CONTEXT
    CONTEXT --> INST1
    CONTEXT --> INST2
    
    style STATEFUL_EXEC fill:#e1f5ff
    style INSTANCE_MGR fill:#fff4e1
    style CONTEXT fill:#f0f0f0
```

### Stateful执行流程

```mermaid
sequenceDiagram
    participant App as Application
    participant SDK as SDK Adapter
    participant Exec as StatefulExecutor
    participant Mgr as InstanceManager
    participant Context as ThreadContext
    participant Instance as Tool Instance
    
    App->>SDK: execute(tool, params)
    SDK->>Exec: doExecute(tool, params, context)
    Exec->>Mgr: getOrCreateInstance(tool, context)
    Mgr->>Context: registerStatefulTool(tool, factory)
    Context->>Instance: create()
    Instance-->>Context: instance
    Context-->>Mgr: instance
    Mgr->>Instance: execute(params)
    Instance-->>Mgr: result
    Mgr-->>Exec: result
    Exec-->>SDK: result
    SDK-->>App: ToolExecutionResult
```

## 🎯 Stateless执行器架构

### Stateless执行器设计

```mermaid
graph TB
    subgraph "Stateless Executor"
        STATELESS_EXEC[StatelessExecutor]
    end
    
    subgraph "Function Wrapper"
        FUNC_WRAPPER[FunctionWrapper]
    end
    
    subgraph "User Functions"
        FUNC1[Function 1]
        FUNC2[Function 2]
    end
    
    STATELESS_EXEC --> FUNC_WRAPPER
    FUNC_WRAPPER --> FUNC1
    FUNC_WRAPPER --> FUNC2
    
    style STATELESS_EXEC fill:#e1f5ff
    style FUNC_WRAPPER fill:#fff4e1
```

### Stateless执行流程

```mermaid
sequenceDiagram
    participant App as Application
    participant SDK as SDK Adapter
    participant Exec as StatelessExecutor
    participant Wrapper as FunctionWrapper
    participant Func as User Function
    
    App->>SDK: execute(tool, params)
    SDK->>Exec: doExecute(tool, params)
    Exec->>Wrapper: call(execute, params)
    Wrapper->>Func: execute(params)
    Func-->>Wrapper: result
    Wrapper-->>Exec: result
    Exec-->>SDK: result
    SDK-->>App: ToolExecutionResult
```

## 🔄 数据流

### 完整的执行流程

```mermaid
graph TB
    START[Application Request] --> VALIDATE[Validate Parameters]
    VALIDATE --> EXECUTE[Execute Tool]
    
    EXECUTE --> MCP{Tool Type?}
    MCP -->|MCP| MCP_EXEC[MCP Executor]
    MCP -->|REST| REST_EXEC[REST Executor]
    MCP -->|Stateful| STATEFUL_EXEC[Stateful Executor]
    MCP -->|Stateless| STATELESS_EXEC[Stateless Executor]
    
    MCP_EXEC --> MCP_TRANS{Transport?}
    MCP_TRANS -->|Stdio| STDIO[Stdio Transport]
    MCP_TRANS -->|SSE| SSE[SSE Transport]
    
    STDIO --> MCP_SERVER[MCP Server]
    SSE --> MCP_SERVER
    
    REST_EXEC --> HTTP[HTTP Client]
    HTTP --> API[External API]
    
    STATEFUL_EXEC --> CONTEXT[Thread Context]
    CONTEXT --> INSTANCE[Tool Instance]
    
    STATELESS_EXEC --> FUNCTION[User Function]
    
    MCP_SERVER --> RESULT[Process Result]
    API --> RESULT
    INSTANCE --> RESULT
    FUNCTION --> RESULT
    
    RESULT --> RETRY{Retry Needed?}
    RETRY -->|Yes| EXECUTE
    RETRY -->|No| FINAL[Final Result]
    
    FINAL --> RESPONSE[Return to Application]
    
    style MCP_EXEC fill:#e1f5ff
    style REST_EXEC fill:#e1f5ff
    style STATEFUL_EXEC fill:#e1f5ff
    style STATELESS_EXEC fill:#e1f5ff
    style STDIO fill:#fff4e1
    style SSE fill:#fff4e1
    style HTTP fill:#fff4e1
    style CONTEXT fill:#f0f0f0
```

## 📊 错误处理流程

```mermaid
graph TB
    ERROR[Error Occurred] --> CLASSIFY{Error Type?}
    
    CLASSIFY -->|Network| NETWORK[NetworkError]
    CLASSIFY -->|Timeout| TIMEOUT[TimeoutError]
    CLASSIFY -->|Validation| VALIDATION[ValidationError]
    CLASSIFY -->|HTTP| HTTP{HTTP Status?}
    
    HTTP -->|429| RATELIMIT[RateLimitError]
    HTTP -->|5xx| SERVER[ServerError]
    HTTP -->|4xx| CLIENT[ClientError]
    
    NETWORK --> RETRY{Should Retry?}
    TIMEOUT --> RETRY
    RATELIMIT --> RETRY
    SERVER --> RETRY
    
    RETRY -->|Yes| DELAY[Apply Delay]
    RETRY -->|No| FINAL_ERROR[Final Error]
    
    DELAY --> RETRY_COUNT{Retry Count < Max?}
    RETRY_COUNT -->|Yes| RETRY_EXEC[Retry Execution]
    RETRY_COUNT -->|No| FINAL_ERROR
    
    RETRY_EXEC --> SUCCESS{Success?}
    SUCCESS -->|Yes| RESULT[Return Result]
    SUCCESS -->|No| ERROR
    
    VALIDATION --> FINAL_ERROR
    CLIENT --> FINAL_ERROR
    
    FINAL_ERROR --> WRAP[Wrap in ToolError]
    WRAP --> RETURN[Return Error Result]
    
    style NETWORK fill:#ffcccc
    style TIMEOUT fill:#ffcccc
    style VALIDATION fill:#ffcccc
    style RATELIMIT fill:#ffcccc
    style SERVER fill:#ffcccc
    style CLIENT fill:#ffcccc
```

## 🔐 安全考虑

### 1. 输入验证
- 所有参数通过zod schema验证
- 类型安全检查
- 格式验证（URL、email等）

### 2. 错误处理
- 统一的错误类型
- 敏感信息过滤
- 错误日志记录

### 3. 资源管理
- 连接池管理
- 超时控制
- 资源清理

### 4. 权限控制
- ThreadContext隔离
- 实例生命周期管理
- 访问控制

## 🚀 性能优化

### 1. 连接复用
- Transport实例缓存
- 连接池管理
- Keep-alive机制

### 2. 并发控制
- 请求队列
- 并发限制
- 背压处理

### 3. 缓存策略
- 结果缓存
- 配置缓存
- Schema缓存

### 4. 资源优化
- 懒加载
- 按需创建
- 及时释放

## 📈 可扩展性

### 1. 新增执行器
```typescript
// 1. 在packages/tool-executors中创建新执行器
export class NewExecutor extends BaseToolExecutor {
  protected async doExecute(tool, params, context) {
    // 实现逻辑
  }
}

// 2. 在SDK中创建适配器
export class NewToolExecutor extends BaseToolExecutor {
  private executor = new NewExecutor();
  
  protected async doExecute(tool, params, context) {
    return this.executor.doExecute(tool, params, context);
  }
}

// 3. 在ToolType中添加新类型
export enum ToolType {
  // ...existing types
  NEW = 'NEW'
}
```

### 2. 新增传输模式
```typescript
// 1. 实现传输接口
export class NewTransport implements McpTransport {
  async execute(url, options) {
    // 实现逻辑
  }
  
  async disconnect() {
    // 清理逻辑
  }
}

// 2. 在McpExecutor中注册
private async getOrCreateTransport(serverName, config) {
  if (config.transportMode === 'new') {
    return new NewTransport(config);
  }
  // ...existing logic
}
```

## 📝 设计原则

1. **单一职责**：每个模块只负责一个功能
2. **开闭原则**：对扩展开放，对修改关闭
3. **依赖倒置**：依赖抽象而非具体实现
4. **接口隔离**：使用最小接口
5. **里氏替换**：子类可以替换父类

## 🔗 相关文档

- [设计文档](./README.md)
- [迁移指南](./migration-guide.md)
- [API文档](./api.md)
- [最佳实践](./best-practices.md)