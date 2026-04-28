# Thread Input/Output 数据流分析

## 概述

本文档详细分析了 Graph Agent 框架中 `Thread`（线程/工作流执行实例）的 `input` 和 `output` 机制，包括其定义、作用、生命周期、访问方式以及设计原则。

---

## 1. Input（输入数据）

### 1.1 定义与位置

- **定义文件**: `packages/types/src/thread/definition.ts`
- **类型**: `Record<string, unknown>`
- **字段位置**: `Thread` 接口第 61 行

```typescript
export interface Thread {
  // ... 其他字段
  
  /**
   * Input data (as a special variable, accessible via path)
   * 
   * Description: Stores the input data for the workflow
   * - Initialized when the START node is executed
   * - Can be accessed through expression parsing (using input.)
   * - Remains unchanged throughout the workflow execution
   * - Used to pass external inputs into the workflow
   */
  input: Record<string, unknown>;
  
  // ... 其他字段
}
```

### 1.2 核心作用

| 特性 | 说明 |
|------|------|
| **初始输入源** | 存储工作流执行时从外部传入的输入参数 |
| **只读数据** | 在整个工作流执行过程中保持不变 |
| **路径访问** | 支持通过 `input.` 前缀在表达式中访问嵌套属性 |
| **数据传递** | 用于将外部数据注入到工作流内部 |

### 1.3 生命周期

1. **初始化阶段**
   - 在 `ThreadBuilder.build()` 方法中初始化
   - 从 `ThreadOptions.input` 参数获取
   - 默认值为空对象 `{}`
   
   ```typescript
   // sdk/graph/execution/factories/thread-builder.ts (第 167 行)
   const thread: Thread = {
     // ...
     input: options.input || {},
     output: {},
     // ...
   };
   ```

2. **执行阶段**
   - 全程保持不变（只读）
   - 所有节点均可通过 `input.` 前缀访问

3. **复制与继承**
   - **Fork 操作**: 子线程会深拷贝父线程的 input
   - **子工作流**: 触发子工作流时 input 会被复制传递
   
   ```typescript
   // sdk/graph/execution/factories/thread-builder.ts (第 334 行)
   input: { ...parentThread.input },
   ```

### 1.4 访问方式

#### 通过 VariableAccessor 访问

```typescript
import { VariableAccessor } from 'sdk/graph/execution/utils/variable-accessor.js';

const accessor = new VariableAccessor(threadEntity);

// 访问顶层属性
const userName = accessor.get('input.userName');

// 访问嵌套属性
const timeout = accessor.get('input.config.timeout');

// 访问数组元素
const firstItem = accessor.get('input.items[0].name');

// 检查是否存在
const exists = accessor.has('input.userName');
```

#### 在模板表达式中使用

```handlebars
<!-- 在 Prompt 模板或条件表达式中 -->
Hello {{input.userName}}, your timeout is {{input.config.timeout}}
```

#### 直接获取

```typescript
// 通过 ThreadEntity 获取完整 input 对象
const input = threadEntity.getInput();

// 通过底层 Thread 对象获取
const thread = threadEntity.getThread();
const input = thread.input;
```

### 1.5 与变量的区别

| 特性 | input | variableScopes.thread |
|------|-------|----------------------|
| **用途** | 工作流初始输入 | 工作流执行过程中的变量 |
| **可变性** | 只读 | 可读写 |
| **初始化** | 创建时从外部传入 | 可定义默认值或运行时设置 |
| **生命周期** | 全程不变 | 可随执行流程变化 |
| **访问前缀** | `input.` | 直接访问或 `thread.` |

---

## 2. Output（输出数据）

### 2.1 定义与位置

- **定义文件**: `packages/types/src/thread/definition.ts`
- **类型**: `Record<string, unknown>`
- **字段位置**: `Thread` 接口第 88 行

```typescript
export interface Thread {
  // ... 其他字段
  
  /**
   * Output data (as a special variable, accessible via path)
   * 
   * Description: Stores the final output data of the workflow
   * - Set when the END node is executed
   * - Can be accessed through expression parsing (using output.)
   * - Defaults to an empty object, populated by the END node or the last node
   * - Used to return the execution result of the workflow
   */
  output: Record<string, unknown>;
  
  // ... 其他字段
}
```

### 2.2 核心作用

| 特性 | 说明 |
|------|------|
| **最终结果** | 存储工作流执行完毕后的输出数据 |
| **结果返回** | 作为 `ThreadResult` 的一部分返回给调用者 |
| **节点设置** | 通常由 END 节点或最后一个执行节点设置 |
| **数据聚合** | 可聚合多个节点的执行结果 |

### 2.3 生命周期

1. **初始化阶段**
   - 创建 Thread 时初始化为空对象 `{}`
   
   ```typescript
   // sdk/graph/execution/factories/thread-builder.ts (第 168 行)
   output: {},
   ```

2. **执行阶段**
   - 初始为空，逐步由节点填充
   - 通常在 END 节点执行时最终设置

3. **返回阶段**
   - 作为 `ThreadResult.output` 返回
   
   ```typescript
   // sdk/graph/execution/coordinators/thread-execution-coordinator.ts (第 124 行)
   return {
     threadId,
     output: this.threadEntity.getOutput(),
     executionTime,
     nodeResults: this.threadEntity.getNodeResults(),
     metadata: { ... }
   };
   ```

### 2.4 访问与设置

#### 设置输出

```typescript
// 通过 ThreadEntity 设置输出
threadEntity.setOutput({
  result: 'Task completed',
  status: 'success',
  data: { 
    count: 10,
    processedItems: ['item1', 'item2']
  }
});

// 在 END 节点中设置
// 通常在节点处理器中完成
```

#### 读取输出

```typescript
// 通过 VariableAccessor 访问
const accessor = new VariableAccessor(threadEntity);
const result = accessor.get('output.result');
const data = accessor.get('output.data.count');

// 通过 ThreadEntity 获取完整输出
const output = threadEntity.getOutput();

// 在表达式中使用
// {{output.result}} // 'Task completed'
// {{output.data.count}} // 10
```

#### 从执行结果获取

```typescript
// 执行完成后从 ThreadResult 获取
const result = await threadExecutor.executeThread(threadEntity);
const output = result.output;
const executionTime = result.executionTime;
```

### 2.5 子工作流中的输出

在子工作流场景中，输出具有继承和隔离特性：

```typescript
// sdk/graph/execution/factories/thread-builder.ts (第 247 行)
// 创建子线程时，output 被清空
output: {},
```

---

## 3. 统一访问机制：VariableAccessor

### 3.1 设计目标

提供统一的变量访问接口，支持：
- 嵌套路径解析
- 命名空间访问
- 数组索引访问
- 作用域优先级查找

### 3.2 支持的命名空间

```typescript
// sdk/graph/execution/utils/variable-accessor.ts (第 36-42 行)
export type VariableNamespace =
  | "input"    // 输入数据
  | "output"   // 输出数据
  | "global"   // 全局作用域
  | "thread"   // 线程作用域
  | "local"    // 局部作用域
  | "loop";    // 循环作用域
```

### 3.3 访问示例

```typescript
const accessor = new VariableAccessor(threadEntity);

// 输入数据
accessor.get('input.userName')           // 获取用户名
accessor.get('input.config.timeout')     // 获取嵌套配置

// 输出数据
accessor.get('output.result')            // 获取执行结果
accessor.get('output.data.items[0]')     // 获取数组元素

// 普通变量（按作用域优先级）
accessor.get('userName')                 // 查找变量
accessor.get('global.config')            // 全局变量
accessor.get('thread.state')             // 线程变量
accessor.get('loop.item')                // 循环变量
```

### 3.4 实现原理

```typescript
// sdk/graph/execution/utils/variable-accessor.ts (第 77-110 行)
get(path: string): unknown {
  if (!path) return undefined;
  
  // 解析命名空间
  const parts = path.split('.');
  const namespace = parts[0];
  const remainingPath = parts.slice(1).join('.');
  
  // 根据命名空间路由到不同的获取方法
  switch (namespace) {
    case 'input':
      return this.getFromInput(remainingPath);
    case 'output':
      return this.getFromOutput(remainingPath);
    case 'global':
      return this.getFromScope(remainingPath || path, 'global');
    // ... 其他命名空间
    default:
      return this.getFromScopedVariables(path);
  }
}
```

---

## 4. 数据流架构

### 4.1 整体流程

```
┌─────────────────┐
│  外部调用者      │
│  (Caller)       │
└────────┬────────┘
         │
         │ 1. 传入 input 数据
         ▼
┌─────────────────┐
│  ThreadBuilder  │
│  (创建 Thread)   │
└────────┬────────┘
         │
         │ 2. 初始化 input/output
         ▼
┌─────────────────┐
│   ThreadEntity  │
│   (执行实例)     │
│   input: {...}  │
│   output: {}    │
└────────┬────────┘
         │
         │ 3. 执行工作流节点
         │    - 读取 input
         │    - 处理业务逻辑
         │    - 设置 output
         ▼
┌─────────────────┐
│ ThreadExecution │
│ Coordinator     │
└────────┬────────┘
         │
         │ 4. 返回 ThreadResult
         │    { output: {...} }
         ▼
┌─────────────────┐
│  外部调用者      │
│  (接收结果)      │
└─────────────────┘
```

### 4.2 节点执行中的数据流

```
START Node
    │
    ├─> 初始化 input (从 ThreadOptions)
    │
    ▼
Processing Nodes
    │
    ├─> 读取 input.* (只读)
    ├─> 读写变量 (variableScopes)
    └─> 处理业务逻辑
    │
    ▼
END Node
    │
    ├─> 聚合执行结果
    └─> 设置 output.*
    │
    ▼
Return ThreadResult.output
```

---

## 5. 设计原则与最佳实践

### 5.1 设计原则

1. **输入输出分离**
   - `input` 只读，`output` 可写
   - 明确数据流向，便于调试

2. **命名空间隔离**
   - 通过 `input.` 和 `output.` 前缀明确数据来源
   - 避免与普通变量命名冲突

3. **路径访问统一**
   - 支持嵌套对象和数组索引
   - 统一的访问接口简化代码

4. **生命周期清晰**
   - input 全程不变
   - output 在执行结束时确定

### 5.2 最佳实践

#### 输入数据设计
```typescript
// ✅ 推荐：结构化输入
const input = {
  user: { id: 123, name: 'Alice' },
  config: { timeout: 5000, retry: 3 },
  items: ['item1', 'item2', 'item3']
};

// ❌ 避免：扁平化或过于复杂的输入
const input = {
  userId: 123,
  userName: 'Alice',
  userTimeout: 5000,
  userRetry: 3,
  // ...
};
```

#### 输出数据设计
```typescript
// ✅ 推荐：结构清晰的输出
threadEntity.setOutput({
  success: true,
  message: 'Task completed successfully',
  data: {
    processedCount: 10,
    results: [...]
  },
  metadata: {
    executionTime: 1234,
    timestamp: Date.now()
  }
});

// ✅ 在模板中使用
// {{output.success}} ? '成功' : '失败'
// {{output.data.processedCount}} 条记录已处理
```

#### 变量访问
```typescript
// ✅ 推荐：明确命名空间
const userName = accessor.get('input.userName');
const result = accessor.get('output.result');

// ❌ 避免：混淆输入输出与变量
// 不要将 input 数据赋值给普通变量后忘记来源
```

### 5.3 常见误区

| 误区 | 正确做法 |
|------|---------|
| 尝试修改 `input` | `input` 是只读的，应使用变量存储中间状态 |
| 在 START 节点设置 `output` | `output` 应在 END 节点或最后设置 |
| 混淆 `input` 和普通变量 | 明确区分：`input` 是输入，变量是中间状态 |
| 过度嵌套 `input` | 保持输入结构合理，避免过深的嵌套 |

---

## 6. 相关代码位置

### 6.1 类型定义
- `packages/types/src/thread/definition.ts` - Thread 接口定义
- `packages/types/src/thread/execution.ts` - ThreadOptions 和 ThreadResult

### 6.2 核心实现
- `sdk/graph/entities/thread-entity.ts` - ThreadEntity 类
- `sdk/graph/execution/factories/thread-builder.ts` - Thread 构建逻辑
- `sdk/graph/execution/coordinators/thread-execution-coordinator.ts` - 执行协调器
- `sdk/graph/execution/utils/variable-accessor.ts` - 变量访问器
- `sdk/graph/execution/coordinators/variable-coordinator.ts` - 变量协调器

### 6.3 状态管理
- `sdk/graph/state-managers/variable-state.ts` - 变量状态管理
- `sdk/graph/execution/thread-execution-context.ts` - 执行上下文

---

## 7. 总结

Thread 的 `input` 和 `output` 机制是 Graph Agent 框架数据流的核心：

1. **input** 是工作流的**只读输入源**，在创建时初始化，全程不变
2. **output** 是工作流的**最终输出结果**，在执行结束时设置并返回
3. 通过 **VariableAccessor** 提供统一的命名空间访问机制
4. 设计遵循**输入 - 处理 - 输出**的经典模式，数据流向清晰
5. 支持嵌套路径、数组索引等复杂访问场景

这种设计使得工作流的数据流易于理解、调试和测试，是构建可靠自动化工作流的基础。

---

**文档版本**: 1.0  
**最后更新**: 2026-01-XX  
**维护者**: Graph Agent Team
