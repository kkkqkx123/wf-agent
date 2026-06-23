# SDK 中 input/output 字段使用情况分析

## 概述

本文档分析了 SDK 中所有与 input/output 相关的字段的使用情况，评估它们是否冗余，以及是否应该删除以降低系统复杂度。

---

## 一、已删除的字段分析

### 1. Node.inputs 和 Node.outputs

**定义位置**: `sdk/types/node.ts:343-345`（已删除）

**字段定义**:
```typescript
export interface Node {
  // ... 其他字段
  /** 输入定义 */
  inputs?: NodeInput[];
  /** 输出定义 */
  outputs?: NodeOutput[];
}
```

**实际使用情况**:
- ✅ 在整个 SDK 代码库中**没有任何实际使用**
- ✅ 测试文件中也没有使用
- ✅ 应用层代码中也没有使用
- ⚠️ 仅在文档 `docs/architecture/workflow/workflow-example-design-summary.md` 中作为示例出现，但这是另一个示例项目的设计

**结论**: 这些字段是**完全冗余的**，删除是正确的决定，可以降低系统复杂度。

---

## 二、应保留的字段分析

### 1. Thread.input 和 Thread.output

**定义位置**: `sdk/types/thread.ts:74-77`

**字段定义**:
```typescript
export interface NodeExecutionResult {
  /** 节点ID */
  nodeId: ID;
  /** 节点类型 */
  nodeType: string;
  /** 执行状态 */
  status: 'PENDING' | 'RUNNING' | 'COMPLETED' | 'FAILED' | 'SKIPPED' | 'CANCELLED';
  /** 执行步骤序号 */
  step: number;
  /** 输入数据 */
  input?: any;
  /** 输出数据 */
  output?: any;
  // ... 其他字段
}
```

**实际使用情况**:
- ✅ `sdk/core/execution/thread-builder.ts:97,191` - 创建 Thread 时初始化
- ✅ `sdk/core/execution/context/thread-context.ts:105-122` - 提供 getter/setter 方法
- ✅ `sdk/core/execution/router.ts:97-98` - 路由决策时使用
- ✅ `sdk/core/execution/managers/checkpoint-manager.ts:79-80` - 检查点保存/恢复
- ✅ `sdk/core/execution/executors/node/start-node-executor.ts:85-101` - START 节点初始化输入
- ✅ `sdk/core/execution/executors/node/end-node-executor.ts:68-85` - END 节点设置输出
- ✅ `sdk/utils/expression-parser.ts:299-306` - 表达式解析时访问 `input.` 和 `output.` 路径

**结论**: **必须保留**。这是 Thread 的核心数据容器，用于存储工作流的输入和输出数据，在整个执行流程中被广泛使用。

---

### 2. SubgraphNodeConfig.inputMapping 和 outputMapping

**定义位置**: `sdk/types/node.ts:257-260`

**字段定义**:
```typescript
export interface SubgraphNodeConfig {
  /** 子工作流ID */
  subgraphId: ID;
  /** 输入参数映射（父工作流变量到子工作流输入的映射） */
  inputMapping: Record<string, string>;
  /** 输出参数映射（子工作流输出到父工作流变量的映射） */
  outputMapping: Record<string, string>;
  /** 是否异步执行 */
  async: boolean;
}
```

**实际使用情况**:
- ✅ `sdk/core/execution/executors/node/subgraph-node-executor.ts:79,91` - 准备子工作流输入和处理输出
- ✅ `sdk/core/validation/node-validator.ts:288-298` - 验证 SUBGRAPH 节点配置
- ✅ `sdk/core/graph/graph-validator.ts:360-381` - 验证子工作流映射关系
- ✅ `sdk/core/graph/graph-builder.ts:198-199` - 构建子工作流图时使用

**结论**: **必须保留**。这是 SUBGRAPH 节点的核心配置，用于定义父工作流和子工作流之间的变量映射关系。

---

### 3. SubgraphMergeLog.inputMapping 和 outputMapping

**定义位置**: `sdk/types/workflow.ts:96-99`

**字段定义**:
```typescript
export interface SubgraphMergeLog {
  /** 子工作流ID */
  subworkflowId: ID;
  /** 子工作流名称 */
  subworkflowName: string;
  /** SUBGRAPH节点ID */
  subgraphNodeId: ID;
  /** 合并的节点ID映射（原始ID -> 新ID） */
  nodeIdMapping: Map<ID, ID>;
  /** 合并的边ID映射（原始ID -> 新ID） */
  edgeIdMapping: Map<ID, ID>;
  /** 输入映射关系 */
  inputMapping: Map<string, ID>;
  /** 输出映射关系 */
  outputMapping: Map<string, ID>;
  /** 合并时间戳 */
  mergedAt: Timestamp;
}
```

**实际使用情况**:
- ✅ `sdk/core/registry/workflow-registry.ts:651-652` - 记录子工作流合并过程中的映射关系

**结论**: **应该保留**。这是运行时审计信息，用于追踪子工作流合并过程，便于调试和问题排查。

---

### 4. NodeExecutionResult.input 和 output

**定义位置**: `sdk/types/thread.ts:74-77`

**字段定义**:
```typescript
export interface NodeExecutionResult {
  /** 节点ID */
  nodeId: ID;
  /** 节点类型 */
  nodeType: string;
  /** 执行状态 */
  status: 'PENDING' | 'RUNNING' | 'COMPLETED' | 'FAILED' | 'SKIPPED' | 'CANCELLED';
  /** 执行步骤序号 */
  step: number;
  /** 输入数据 */
  input?: any;
  /** 输出数据 */
  output?: any;
  // ... 其他字段
}
```

**实际使用情况**:
- ✅ `sdk/core/execution/executors/node/base-node-executor.ts:66` - 构建节点执行结果
- ✅ `sdk/core/execution/executors/node/join-node-executor.ts:213-249` - JOIN 节点聚合多个子线程的输出
- ✅ `sdk/core/execution/executors/node/end-node-executor.ts:75` - END 节点获取最后一个节点的输出

**结论**: **必须保留**。这是节点执行结果的核心字段，用于记录节点的输入输出数据，支持执行追踪和调试。

---

### 5. GraphNode.inputMapping 和 outputMapping

**定义位置**: `sdk/types/graph.ts:232-235`

**字段定义**:
```typescript
export interface GraphNode {
  /** 节点ID */
  id: ID;
  /** 节点类型 */
  type: string;
  /** 节点名称 */
  name: string;
  /** 输入变量映射 */
  inputMapping?: Map<string, string>;
  /** 输出变量映射 */
  outputMapping?: Map<string, string>;
  // ... 其他字段
}
```

**实际使用情况**:
- ✅ 在图构建和验证过程中使用，用于处理子工作流的变量映射

**结论**: **应该保留**。这是图处理的关键字段，支持子工作流的变量映射。

---

## 三、inputMapping/outputMapping 与 variables 的关系

### 1. 两者不是重复的，而是互补关系

#### variables 的作用

**定义位置**:
- `sdk/types/workflow.ts:64-79` - WorkflowVariable（工作流定义）
- `sdk/types/thread.ts:31-44` - ThreadVariable（执行实例）
- `sdk/types/thread.ts:130` - variableValues（变量值映射）

**用途**:
- 存储工作流执行过程中的**变量数据**
- `variables`: 变量的元数据（名称、类型、作用域、是否只读等）
- `variableValues`: 变量的实际值（`Record<string, any>`）
- 通过 VARIABLE 节点修改
- 在表达式求值时使用（`{{variableName}}`）

**示例**:
```typescript
// 工作流定义
workflow.variables = [
  { name: 'userName', type: 'string', defaultValue: 'Alice' },
  { name: 'userAge', type: 'number', defaultValue: 25 }
]

// Thread 执行时
thread.variableValues = {
  userName: 'Alice',
  userAge: 25
}
```

#### inputMapping/outputMapping 的作用

**定义位置**:
- `sdk/types/node.ts:257-260` - SubgraphNodeConfig

**用途**:
- 定义**父工作流和子工作流之间的数据传递规则**
- `inputMapping`: 父工作流变量 → 子工作流输入
- `outputMapping`: 子工作流输出 → 父工作流变量
- 仅在 SUBGRAPH 节点中使用

**示例**:
```typescript
// SUBGRAPH 节点配置
{
  subgraphId: 'child-workflow',
  inputMapping: {
    'childInput': 'parentVar1',      // 父工作流的 parentVar1 → 子工作流的 childInput
    'childConfig': 'parentVar2'
  },
  outputMapping: {
    'parentResult': 'childOutput',   // 子工作流的 childOutput → 父工作流的 parentResult
    'parentStatus': 'childStatus'
  }
}
```

### 2. 实际使用流程分析

从 `subgraph-node-executor.ts` 可以看到完整流程：

#### 步骤1: 准备子工作流输入
```typescript
private prepareSubgraphInput(inputMapping, thread) {
  if (!inputMapping) {
    // 没有映射时，传递所有变量
    return { ...thread.variableValues };
  }
  
  // 根据 inputMapping 从父工作流的 variableValues 中提取值
  for (const [subgraphVar, parentVar] of Object.entries(inputMapping)) {
    const value = this.getVariableValue(parentVar, thread); // 从 thread.variableValues 读取
    subgraphInput[subgraphVar] = value;
  }
}
```

#### 步骤2: 执行子工作流
```typescript
// 子工作流接收输入，执行后返回结果
const subgraphResult = await this.executeSubgraph(subgraphId, input, ...);
// subgraphResult.output 包含子工作流的输出
```

#### 步骤3: 处理子工作流输出
```typescript
private processSubgraphOutput(outputMapping, subgraphResult, thread) {
  if (!outputMapping) {
    return subgraphResult.output || {};
  }
  
  // 根据 outputMapping 将子工作流输出映射到父工作流的 variableValues
  for (const [parentVar, subgraphVar] of Object.entries(outputMapping)) {
    const value = this.getNestedValue(subgraphResult.output, subgraphVar);
    mappedOutput[parentVar] = value;
    
    // 更新父工作流的 variableValues
    thread.variableValues[parentVar] = value;  // 写入父工作流的变量
  }
}
```

### 3. 为什么两者都需要？

#### 不同的职责
- **variables**: 数据存储容器，保存工作流的变量值
- **inputMapping/outputMapping**: 数据传递规则，定义跨工作流的数据映射

#### 不同的使用场景
- **variables**: 
  - 工作流内部的数据共享
  - 节点之间的数据传递
  - 表达式求值
  
- **inputMapping/outputMapping**:
  - 仅用于 SUBGRAPH 节点
  - 父子工作流之间的数据隔离和传递
  - 支持变量重命名和选择性传递

#### 灵活性优势
```typescript
// 场景1: 父子工作流变量名不同
inputMapping: {
  'childUserName': 'parent.user.name',  // 支持嵌套路径
  'childAge': 'parent.user.age'
}

// 场景2: 选择性传递（只传递需要的变量）
inputMapping: {
  'childInput': 'parentVar1'  // 只传递 parentVar1，不传递 parentVar2
}

// 场景3: 无映射时传递所有变量
inputMapping: undefined  // 传递所有 variableValues
```

---

## 四、总结与建议

### 应该删除的字段
1. **Node.inputs 和 Node.outputs** - ✅ 已删除，完全冗余，无任何实际使用

### 应该保留的字段
1. **Thread.input 和 Thread.output** - 核心数据容器，广泛使用
2. **SubgraphNodeConfig.inputMapping 和 outputMapping** - 子工作流变量映射的关键配置
3. **SubgraphMergeLog.inputMapping 和 outputMapping** - 运行时审计信息
4. **NodeExecutionResult.input 和 output** - 执行追踪和调试
5. **GraphNode.inputMapping 和 outputMapping** - 图处理的关键字段

### 系统复杂度分析
- 删除 Node.inputs 和 Node.outputs 是正确的，因为它们没有被实际使用
- 其他 input/output 相关字段都有明确的用途和广泛的使用场景
- 这些字段共同构成了工作流系统的数据流和变量映射机制
- 删除它们会破坏系统的核心功能

### 设计原则
SDK 的设计遵循以下原则：
1. **数据流通过 Thread 变量系统** - 使用 `thread.variableValues` 存储工作流变量
2. **节点配置通过 Node.config** - 每种节点类型有特定的配置接口
3. **执行结果通过 NodeExecutionResult** - 记录节点的输入输出和状态
4. **子工作流通过映射机制** - 使用 inputMapping/outputMapping 实现变量传递

这种设计避免了在 Node 类型中定义通用的 inputs/outputs 字段，而是通过具体的配置和执行上下文来处理数据流，更加灵活和类型安全。

### inputMapping/outputMapping 与 variables 的关系
**inputMapping/outputMapping 与 variables 不重复，而是互补关系**：

1. **variables** 是数据存储，保存工作流的变量值
2. **inputMapping/outputMapping** 是映射规则，定义如何在不同工作流之间传递数据
3. 两者配合使用，实现了：
   - 工作流内部的数据共享（通过 variables）
   - 工作流之间的数据隔离和传递（通过 inputMapping/outputMapping）
   - 灵活的变量重命名和选择性传递

**建议**: 保留这两个机制，它们共同构成了完整的数据流系统。