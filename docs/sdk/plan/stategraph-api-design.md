# StateGraph API 设计文档

## 1. 设计目标

基于现有SDK架构，创建一个与现有API并行的StateGraph实现，提供类似LangGraph的易用性，同时保持与底层API的兼容性。

## 2. 设计原则

1. **与现有API并行**：不修改现有API，提供新的StateGraph API
2. **最小化抽象**：基于现有功能提供便捷封装
3. **类型安全**：充分利用TypeScript的类型系统
4. **向后兼容**：与现有工作流定义和执行机制兼容

## 3. 架构设计

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   StateGraph    │────│  SDK Components  │────│  Core Engine    │
│                 │    │                  │    │                 │
│  • add_node()   │    │  • Workflow API  │    │  • Graph       │
│  • add_edge()   │    │  • Executor API  │    │  • Executor    │
│  • add_cond...  │    │  • Variables API │    │  • Validators  │
│  • compile()    │    │  • Registry API  │    │                 │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

## 4. 核心组件设计

### 4.1 StateGraph 类

```typescript
/**
 * StateGraph - 状态图类
 * 提供类似LangGraph的API，用于构建状态驱动的工作流
 */
export class StateGraph<StateType = any> {
  private nodes: Map<string, NodeFunction<StateType>> = new Map();
  private edges: Map<string, Set<string>> = new Map(); // 普通边
  private conditionalEdges: Map<string, ConditionalEdgeConfig<StateType>> = new Map(); // 条件边
  private entryPoint: string = 'start';
  private workflowId: string;
  private workflowName: string;
  
  constructor(stateSchema: StateSchema<StateType>);
  
  /**
   * 添加节点
   * @param nodeName 节点名称
   * @param nodeFunction 节点函数，接收状态并返回状态更新
   */
  add_node(nodeName: string, nodeFunction: NodeFunction<StateType>): void;
  
  /**
   * 添加条件边
   * @param source 起始节点
   * @param condition 条件函数，从状态中提取路由键
   * @param mapping 条件值到目标节点的映射
   */
  add_conditional_edges(
    source: string, 
    condition: (state: StateType) => string | symbol, 
    mapping: Record<string, string | typeof END>
  ): void;
  
  /**
   * 添加普通边
   * @param source 起始节点
   * @param target 目标节点
   */
  add_edge(source: string, target: string): void;
  
  /**
   * 设置入口点
   * @param nodeName 入口节点名称
   */
  set_entry_point(nodeName: string): void;
  
  /**
   * 编译工作流
   * 将高级API转换为底层的WorkflowDefinition
   */
  compile(): CompiledGraph<StateType>;
}
```

### 4.2 CompiledGraph 类

```typescript
/**
 * 编译后的工作流
 * 提供执行接口
 */
export class CompiledGraph<StateType = any> {
  constructor(private workflowDef: WorkflowDefinition, private sdk: SDK);
  
  /**
   * 执行工作流
   */
  invoke(input: StateType, options?: InvokeOptions): Promise<InvokeResult<StateType>>;
  
  /**
   * 流式执行工作流
   */
  async *stream(input: StateType, options?: StreamOptions): AsyncGenerator<StreamChunk<StateType>, void, unknown>;
  
  /**
   * 获取当前状态
   */
  get_state(threadId: string): Promise<StateType>;
  
  /**
   * 更新状态
   */
  update_state(threadId: string, stateUpdate: Partial<StateType>): Promise<void>;
}
```

### 4.3 便捷构建函数

```typescript
/**
 * 便捷的节点创建函数
 */
export function createNode<StateType>(
  id: string,
  name: string,
  func: NodeFunction<StateType>,
  options?: NodeOptions
): Node;

/**
 * 便捷的路由节点创建函数
 */
export function createRouteNode<StateType>(
  id: string,
  name: string,
  routes: RouteDefinition<StateType>[],
  options?: RouteNodeOptions
): Node;

/**
 * 便捷的边创建函数
 */
export function createEdge(
  sourceNodeId: string,
  targetNodeId: string,
  type?: EdgeType,
  condition?: Condition
): Edge;
```

## 5. 类型定义

```typescript
// 节点函数类型
export type NodeFunction<StateType> = (state: StateType) => Partial<StateType> | Promise<Partial<StateType>>;

// 条件边配置
export interface ConditionalEdgeConfig<StateType> {
  condition: (state: StateType) => string | symbol;
  mapping: Record<string, string | typeof END>;
}

// 状态模式
export interface StateSchema<StateType> {
  // 可以包含状态验证信息等
}

// 调用选项
export interface InvokeOptions {
  maxSteps?: number;
  timeout?: number;
  enableCheckpoints?: boolean;
}

// 流式选项
export interface StreamOptions extends InvokeOptions {
  chunkSize?: number;
}

// 结果类型
export interface InvokeResult<StateType> {
  state: StateType;
  threadId: string;
  executionTime: number;
  success: boolean;
  error?: Error;
}

// 流式块
export interface StreamChunk<StateType> {
  state: StateType;
  step: number;
  nodeId: string;
  timestamp: number;
}
```

## 6. 实现策略

### 6.1 工作流定义转换

StateGraph内部将高级API调用转换为标准的WorkflowDefinition：

1. **节点转换**：将函数节点转换为CODE或VARIABLE类型的Node
2. **条件边转换**：将条件边转换为ROUTER节点和对应的条件Edge
3. **普通边转换**：直接创建DEFAULT类型的Edge

### 6.2 状态管理

利用现有的VariableManagerAPI进行状态管理：

1. 将状态作为变量存储
2. 在节点执行时更新状态
3. 提供便捷的状态访问和更新方法

### 6.3 执行流程

保持与现有ThreadExecutor的兼容：

1. 编译后的WorkflowDefinition注册到WorkflowRegistry
2. 通过ThreadExecutor执行
3. 状态通过Thread的变量系统传递

## 7. API使用示例

```typescript
// 定义状态类型
interface AgentWorldState {
  task: string;
  messages: [string, string][];
  nextAgent: 'Researcher' | 'Writer' | 'FINISH';
}

// 定义节点函数
const researcherAgentNode = (state: AgentWorldState): Partial<AgentWorldState> => {
  console.log(`--- [Agent: 研究员] 开始工作，任务: ${state.task} ---`);
  const researchResult = `这是关于'${state.task}'的研究成果。`;
  return { 
    messages: [...state.messages, ['Researcher', researchResult]],
    nextAgent: 'Writer' as const
  };
};

const writerAgentNode = (state: AgentWorldState): Partial<AgentWorldState> => {
  console.log(`--- [Agent: 作家] 开始工作 ---`);
  const lastMessage = state.messages[state.messages.length - 1][1];
  const writingResult = `基于以下研究成果：\n${lastMessage}\n\n我完成了最终报告。`;
  return { 
    messages: [...state.messages, ['Writer', writingResult]],
    nextAgent: 'FINISH' as const
  };
};

const dispatcherNode = (state: AgentWorldState): Partial<AgentWorldState> => {
  const lastMessageSender = state.messages.length > 0 ? 
    state.messages[state.messages.length - 1][0] : 'START';
  
  if (lastMessageSender === 'Researcher') {
    return { nextAgent: 'Writer' };
  } else if (lastMessageSender === 'Writer') {
    return { nextAgent: 'FINISH' };
  } else {
    return { nextAgent: 'Researcher' };
  }
};

// 使用StateGraph API
const workflow = new StateGraph<AgentWorldState>({} as AgentWorldState);

workflow.add_node("researcher", researcherAgentNode);
workflow.add_node("writer", writerAgentNode);
workflow.add_node("dispatcher", dispatcherNode);

// 设置入口点
workflow.set_entry_point("dispatcher");

// 添加条件边
workflow.add_conditional_edges(
  "dispatcher",
  (state: AgentWorldState) => state.nextAgent,
  {
    "Researcher": "researcher",
    "Writer": "writer",
    "FINISH": END  // END表示结束
  }
);

// 添加普通边
workflow.add_edge("researcher", "dispatcher");
workflow.add_edge("writer", "dispatcher");

// 编译和执行
const app = workflow.compile();

const inputs = {
  task: "AI在软件开发中的作用",
  messages: [],
  nextAgent: "Researcher" as const
};

// 调用
const result = await app.invoke(inputs);
console.log(result);

// 或流式执行
for await (const chunk of app.stream(inputs)) {
  console.log(chunk);
}
```

## 8. 与现有API的关系

### 8.1 依赖关系
- StateGraph依赖于现有的SDK类
- 不修改现有API
- 通过现有API进行工作流注册和执行

### 8.2 扩展性
- 保持现有API的全部功能
- 为特定用例提供更便捷的接口
- 可以混合使用新旧API

## 9. 实现计划

### 阶段1：核心类实现
- 实现StateGraph类
- 实现CompiledGraph类
- 实现类型定义

### 阶段2：转换逻辑
- 实现高级API到WorkflowDefinition的转换
- 实现条件边到路由节点的转换

### 阶段3：执行集成
- 集成ThreadExecutor执行
- 实现流式执行支持

### 阶段4：测试和文档
- 编写单元测试
- 创建使用示例
- 编写API文档