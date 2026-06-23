# 工作流内部数据结构分析

## 1. 三层数据架构

根据 `boundary-data-flow-architecture.md`，工作流执行中有三个独立的数据层：

| 层次 | 存储位置 | 类型 | 访问方式 | 生命周期 |
|------|---------|------|---------|---------|
| **Execution Data** | `WorkflowExecution.input / output` | JSON-serializable | `getInput() / setOutput()` | 整个执行周期 |
| **Variables** | `VariableStateManager` | 结构化运行时变量 | `variableStateManager.getVariable/setVariable()` | 工作流执行内 |
| **Messages** | `MessageContextRegistry` | LLMMessage[] 数组 | `messageContextRegistry.get/register()` | 跨节点共享 |

---

## 2. 数据流转机制

### 2.1 Execution 级数据流

```
父执行.output (调用方返回值)
     │
     │ 子执行的边界配置 (dataInputs)
     ▼
当前执行.input (执行参数)
     │
     │ START 节点处理 (dataInputs)
     ▼
VariableStateManager (运行时变量)
     │
     │ (在工作流内处理)
     ▼
VariableStateManager (更新后)
     │
     │ END 节点处理 (dataOutputs)
     ▼
当前执行.output (返回给调用方)
```

### 2.2 关键观察

**DataInputs 的本质**：
```typescript
// 从 boundary-data-flow-architecture.md 5.1 节
1. 读取 workflowExecutionEntity.getInput()
2. 遍历 dataInputs 数组
3. 按 parentField 在 input 中查找值
4. 设置到 variableStateManager.setVariable(internalName, value)
```

**DataOutputs 的本质**：
```typescript
// 从 boundary-data-flow-architecture.md 5.2 节
1. 读取 workflowExecutionEntity.getOutput() || {}
2. 遍历 dataOutputs 数组
3. 从 variableStateManager.getVariable(internalName) 读取值
4. 设置到 output[outputKey] = value
```

**结论**：
- `input/output` 是执行的**入口和出口**
- `VariableStateManager` 是执行的**内部工作区**
- 所有节点在工作流内的数据处理都是基于 **Variable**

---

## 3. 多分支数据映射

### 3.1 FORK/JOIN 的参数传递

根据 `boundary-data-flow-architecture.md` 第 4.2 节：

```
FORK (创建分支)
  │
  ├─ 分支 1: 深拷贝父变量和 input
  │   └─ 在分支内修改变量（独立副本）
  │
  ├─ 分支 2: 深拷贝父变量和 input
  │   └─ 在分支内修改变量（独立副本）
  │
  └─ 分支 N: 深拷贝父变量和 input

JOIN
  └─ 通过 variableOutputs / dataOutputs 映射分支变量回父工作流
```

### 3.2 分支输出映射的命名

**关键点**：参数传递后都转换为新的名称（标识符）

```
例子场景：多分支 LLM 分析

FORK
  ├─ Branch A: LLM 分析
  │   output: result (在分支内的变量名)
  │
  ├─ Branch B: DB 查询
  │   output: result (在分支内的变量名)
  │
  └─ Branch C: API 调用
      output: result (在分支内的变量名)

JOIN 节点配置:
  variableOutputs:
    - internalName: "result"    # 分支 A 的变量
      externalName: "result_a"  # 映射到父工作流的新名称
    - internalName: "result"    # 分支 B 的变量
      externalName: "result_b"  # 映射到父工作流的新名称
    - internalName: "result"    # 分支 C 的变量
      externalName: "result_c"  # 映射到父工作流的新名称

执行结果：
  父工作流 VariableStateManager:
    result_a = {...}  (来自分支 A)
    result_b = {...}  (来自分支 B)
    result_c = {...}  (来自分支 C)
```

---

## 4. 当前系统的数据聚合问题

### 4.1 问题描述

多分支的输出通过 `variableOutputs` 或 `dataOutputs` 映射回父工作流后，每个分支的结果是**独立的变量**。

当需要聚合这些结果时（例如放在一个数组中），没有现成的机制：

```
❌ 当前状态：
   result_a = {...}
   result_b = {...}
   result_c = {...}
   
   需要：
   all_results = [result_a, result_b, result_c]
   
   只能依赖 SCRIPT 节点或复杂的表达式
```

### 4.2 为什么 SCRIPT 不适合

- **语义不符**：SCRIPT 设计用于外部 I/O（API 调用、文件操作），不是内部数据处理
- **性能开销**：运行 JavaScript 沙箱有额外成本
- **代码复杂度**：用户需要写代码处理数据转换
- **可读性差**：工作流中嵌入大量脚本代码，难以维护

---

## 5. DATA_PROCESSOR 的数据操作对象

### 5.1 应支持的操作范围

基于上面的分析，DATA_PROCESSOR 应该操作：

#### A. Message 数据（现有）
- 对象：`MessageContextRegistry` 中的 `LLMMessage[]`
- 操作：truncate、insert、replace、clear、filter
- 例子：管理对话历史的长度、删除某些消息

#### B. Variable 数据（新增）
- 对象：`VariableStateManager` 中的运行时变量
- 操作：
  - **aggregate**：聚合多个变量为一个
  - **transform**：转换变量的值
  - **batch-update**：批量更新多个变量
  - **filter**：过滤变量值（如数组或对象）

#### C. Execution Data（可选）
- 对象：`WorkflowExecution.input / output`
- 操作：写入输出数据
- 这些通常通过 END 节点的 dataOutputs 处理，但可以由 DATA_PROCESSOR 作为补充

### 5.2 不需要的概念

**不需要"多分支聚合"这个特殊概念**，因为：

1. 多分支输出已经被映射为独立命名的变量（`result_a`, `result_b`, `result_c`）
2. 聚合就是"基于标识符的操作"：
   ```
   # 聚合变量 result_a, result_b, result_c 为一个数组
   aggregated = [
     getVariable("result_a"),
     getVariable("result_b"),
     getVariable("result_c")
   ]
   ```
3. 这等同于一般的"变量聚合"，不需要特殊的分支感知

---

## 6. DATA_PROCESSOR 的设计范围

### 6.1 操作类型定义

```typescript
// Variable 聚合操作
interface VariableAggregateOperation {
  operation: 'aggregate';
  sourceVariables: string[];        // 变量名列表
  targetVariable: string;           // 目标变量名
  aggregateMode: 'array' | 'object' | 'merge';
  
  // object 模式：指定输出的键名映射
  keyMapping?: Record<string, string>;  // {sourceVar: outputKey}
  
  // 可选：过滤条件（如果 sourceVariables 中的值是数组，可以过滤数组元素）
  filterExpression?: string;
}

// Variable 转换操作
interface VariableTransformOperation {
  operation: 'transform';
  sourceVariable: string;
  targetVariable: string;
  // 内置的转换函数（使用现有的表达式求值器）
  transformExpression: string;  // 如 "item.map(x => x.id)"
}

// Variable 批量更新
interface VariableBatchUpdateOperation {
  operation: 'batch-update';
  updates: Array<{
    name: string;
    expression: string;  // 表达式，使用现有 expressionEvaluator
    type?: string;
  }>;
}
```

### 6.2 操作执行流程

```
DATA_PROCESSOR 节点执行：

1. 验证配置
   └─ 检查变量存在性
   └─ 检查表达式有效性

2. 执行操作
   ├─ 对于 aggregate：
   │   └─ 读取 sourceVariables
   │   └─ 按 mode 聚合
   │   └─ 设置 targetVariable
   │
   ├─ 对于 transform：
   │   └─ 读取 sourceVariable
   │   └─ 执行 transformExpression
   │   └─ 设置 targetVariable
   │
   └─ 对于 batch-update：
       └─ 对每个 update
           └─ 求值 expression
           └─ 设置变量

3. 返回结果
   └─ 操作统计信息
```

---

## 7. 与现有系统的关系

### 7.1 与 VARIABLE 节点的区别

| 特性 | VARIABLE 节点 | DATA_PROCESSOR (Variable 模式) |
|------|--------------|------------------------------|
| 用途 | 单个变量赋值 | 多个变量的聚合和批处理 |
| 输入 | 一个表达式 | 多个变量名 |
| 操作 | 表达式求值 → 赋值 | 聚合、转换、批量更新 |
| 适用场景 | 计算某个字段的值 | 聚合多分支结果、数据转换 |

**关键点**：VARIABLE 节点不被替代，DATA_PROCESSOR 是补充

### 7.2 与 CONTEXT_PROCESSOR 的关系

```
┌─────────────────────────────────────┐
│      CONTEXT_PROCESSOR (重命名)     │
│  (或 DATA_PROCESSOR 的更好名称)     │
├─────────────────────────────────────┤
│                                     │
│  Message 操作                       │
│  ├─ truncate, insert, clear...     │
│  └─ 操作对象：MessageContextRegistry│
│                                     │
│  Variable 操作 (新增)               │
│  ├─ aggregate, transform...         │
│  └─ 操作对象：VariableStateManager  │
│                                     │
└─────────────────────────────────────┘
```

### 7.3 与 SCRIPT 节点的竞争关系

**SCRIPT 的位置**（外部 I/O）：
```
SCRIPT 节点：
  调用 API
  读写文件
  执行系统命令
  运行自定义业务逻辑
  
  ❌ 不应该用于内部数据聚合
```

**DATA_PROCESSOR 的位置**（内部数据处理）：
```
DATA_PROCESSOR 节点：
  聚合变量
  转换数据格式
  过滤数据
  批量更新变量
  
  ✅ 专门用于内部数据处理
```

---

## 8. 实现优先级

### 优先级 1：Variable Aggregate 操作
- 最常见的场景（多分支聚合）
- 实现相对简单
- 直接解决用户最大的痛点

### 优先级 2：Variable Transform 操作
- 支持数据格式转换
- 配合 aggregate 使用效果更好

### 优先级 3：Batch-Update 操作
- 优化批量变量更新的性能
- 减少节点执行数

---

## 9. 关键设计决策

### 决策 1：不单独处理 "Data 聚合"
**理由**：
- `input/output` 通过 `dataInputs/dataOutputs` 映射为 Variable
- 所有操作本质上都是 Variable 操作
- 无需建立单独的 data 处理机制

### 决策 2：复用现有的 expressionEvaluator
**理由**：
- 系统已有成熟的表达式求值器（AST-based，安全）
- VARIABLE 节点已在使用
- 无需引入新的脚本运行机制

### 决策 3：不需要"显式的多分支感知"
**理由**：
- JOIN 已经负责命名映射（`result_a`, `result_b`, `result_c`）
- DATA_PROCESSOR 只需基于这些命名进行操作
- 设计更简洁，不增加复杂性

---

## 总结

DATA_PROCESSOR 的核心价值：
1. ✅ 聚合多个变量（解决多分支数据聚合）
2. ✅ 转换数据格式（支持灵活的数据处理）
3. ✅ 批量更新变量（提高效率）
4. ✅ 保持消息处理能力（现有 CONTEXT_PROCESSOR 的功能）
5. ✅ 明确的语义（区分 I/O vs 内部数据处理）
