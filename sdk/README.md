# Graph Agent SDK

Graph Agent SDK 是一个用于工作流执行的核心SDK，提供类型定义和执行引擎。

## 目录结构

```
sdk/
├── types/              # 类型定义层
│   ├── common.ts       # 基础类型（ID、Timestamp、Version、Metadata）
│   ├── workflow.ts     # 工作流类型
│   ├── node.ts         # 节点类型
│   ├── edge.ts         # 边类型
│   ├── thread.ts       # 线程类型（执行实例）
│   ├── tool.ts         # 工具类型
│   ├── llm.ts          # LLM类型
│   ├── execution.ts    # 执行类型
│   ├── events.ts       # 事件类型
│   ├── errors.ts       # 错误类型
│   ├── checkpoint.ts   # 检查点类型
│   └── index.ts        # 统一导出
├── core/               # 核心执行层（待实现）
│   ├── execution/      # 执行引擎
│   ├── state/          # 状态管理
│   ├── llm/            # LLM集成
│   ├── tools/          # 工具执行
│   └── validation/     # 验证
├── api/                # 对外API（待实现）
│   ├── sdk.ts          # SDK主类
│   ├── options.ts      # API选项
│   └── result.ts       # API结果
├── utils/              # 工具函数（待实现）
│   ├── id-generator.ts # ID生成
│   └── error-handler.ts # 错误处理
├── tsconfig.json       # TypeScript配置
└── README.md           # 本文件
```

## 核心概念

### 1. Workflow（工作流）

- 纯静态定义，包含nodes、edges等结构信息
- 不包含任何执行状态
- 可序列化和反序列化

### 2. Thread（线程）

- Workflow的执行实例
- 包含执行状态、变量、历史等动态信息
- 支持Fork/Join操作
- 可序列化，支持执行恢复

### 3. Node（节点）

- 15种节点类型：START、END、VARIABLE、FORK、JOIN、SUBGRAPH、CODE、LLM、TOOL、USER_INTERACTION、ROUTE、CONTEXT_PROCESSOR、LOOP_START、LOOP_END
- 只存储edgeId，不持有Edge对象引用
- 支持动态属性和验证规则

### 4. Edge（边）

- 定义节点之间的连接关系
- 只存储nodeId，不持有Node对象引用
- 支持条件路由和优先级

### 5. LLM Profile

- LLM配置文件，支持独立配置和复用
- 包含provider、model、parameters、headers等
- LLM Node通过profileId引用

### 6. Tool

- 只提供工具引用，不包含实现细节
- 包含名称、描述、参数schema
- 用于LLM调用时提供工具定义

## 设计原则

### 1. 避免循环依赖

- Node和Edge只存储ID
- 通过Workflow对象进行关联查询
- 边数组支持排序和过滤

### 2. 职责分离

- Workflow：静态定义
- Thread：执行实例
- Checkpoint：状态快照
- 应用层：持久化、管理

### 3. 配置复用

- LLM使用Profile概念
- Tool只提供引用
- 避免重复配置

### 4. 不提供持久化接口

- SDK专注于执行
- 持久化由应用层负责
- Checkpoint只包含创建和恢复

### 5. 事件驱动

- 所有事件关联到threadId
- 支持Fork/Join事件
- 支持异步事件处理

## 类型系统

### 基础类型

- `ID`: 字符串类型的ID，提供`IDUtils`工具函数
- `Timestamp`: 数字类型的时间戳，提供`TimestampUtils`工具函数
- `Version`: 字符串类型的版本号，提供`VersionUtils`工具函数
- `Metadata`: 键值对类型的元数据，提供`MetadataUtils`工具函数

### 错误类型

- `SDKError`: 基础错误类
- `ValidationError`: 验证错误
- `ExecutionError`: 执行错误
- `ConfigurationError`: 配置错误
- `TimeoutError`: 超时错误
- `NotFoundError`: 资源未找到错误
- `NetworkError`: 网络错误
- `LLMError`: LLM调用错误
- `ToolError`: 工具调用错误

## 使用示例

### 导入类型

```typescript
import {
  WorkflowDefinition,
  Node,
  NodeType,
  Edge,
  EdgeType,
  Thread,
  ThreadStatus,
  LLMProfile,
  LLMProvider,
  Tool,
  ToolType,
  IDUtils,
  TimestampUtils,
} from "@sdk/types";
```

### 创建工作流

```typescript
const workflow: WorkflowDefinition = {
  id: IDUtils.generate(),
  name: "示例工作流",
  version: "1.0.0",
  createdAt: TimestampUtils.now(),
  updatedAt: TimestampUtils.now(),
  nodes: [
    {
      id: "start",
      type: NodeType.START,
      name: "开始",
      config: {},
      outgoingEdgeIds: ["edge-1"],
      incomingEdgeIds: [],
    },
    {
      id: "llm",
      type: NodeType.LLM,
      name: "LLM调用",
      config: {
        profileId: "openai-gpt4",
        prompt: [{ role: "user", content: "Hello" }],
      },
      outgoingEdgeIds: ["edge-2"],
      incomingEdgeIds: ["edge-1"],
    },
    {
      id: "end",
      type: NodeType.END,
      name: "结束",
      config: {},
      outgoingEdgeIds: [],
      incomingEdgeIds: ["edge-2"],
    },
  ],
  edges: [
    {
      id: "edge-1",
      sourceNodeId: "start",
      targetNodeId: "llm",
      type: EdgeType.DEFAULT,
    },
    {
      id: "edge-2",
      sourceNodeId: "llm",
      targetNodeId: "end",
      type: EdgeType.DEFAULT,
    },
  ],
};
```

### 创建LLM Profile

```typescript
const llmProfile: LLMProfile = {
  id: "openai-gpt4",
  name: "OpenAI GPT-4",
  provider: LLMProvider.OPENAI,
  model: "gpt-4",
  apiKey: "sk-xxx",
  parameters: {
    temperature: 0.7,
    maxTokens: 2000,
  },
  headers: {
    "X-Custom-Header": "value",
  },
  timeout: 30000,
  maxRetries: 3,
};
```

### 创建工具定义

```typescript
const tool: Tool = {
  id: "calculator",
  name: "calculator",
  type: ToolType.BUILTIN,
  description: "执行数学计算",
  parameters: {
    properties: {
      expression: {
        type: "string",
        description: "数学表达式",
      },
    },
    required: ["expression"],
  },
};
```

## 开发状态

### ✅ 已完成

- [x] Types层所有类型定义
- [x] TypeScript配置
- [x] 类型检查通过

### 🚧 待实现

- [ ] Core层 - 执行引擎
- [ ] Core层 - 状态管理
- [ ] Core层 - LLM集成
- [ ] Core层 - 工具框架
- [ ] API层 - SDK主类
- [ ] Utils层 - 工具函数
- [ ] 单元测试
- [ ] 集成测试

## 构建和测试

### 类型检查

```bash
cd sdk
tsc --noEmit
```

### 构建

```bash
cd sdk
tsc
```

## 注意事项

1. **类型安全**：充分利用TypeScript类型系统
2. **避免循环依赖**：使用ID引用，不持有对象
3. **职责分离**：SDK专注执行，应用层负责持久化
4. **配置复用**：使用Profile概念避免重复配置
5. **事件驱动**：通过事件提供扩展点

## 许可证

MIT
