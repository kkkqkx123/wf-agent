# WorkflowExecution Input/Output 快速指南

## 快速开始

### 1. 创建 WorkflowExecution 时传入 input

```typescript
import { WorkflowExecutionBuilder } from 'sdk/workflow/execution/factories/workflow-execution-builder.js';

const executionBuilder = new WorkflowExecutionBuilder();

// 创建 WorkflowExecution 并传入 input 数据
const workflowExecution = await executionBuilder.build('my-workflow', {
  input: {
    userName: 'Alice',
    userAge: 25,
    config: {
      timeout: 5000,
      retry: 3
    },
    items: ['item1', 'item2', 'item3']
  }
});
```

### 2. 在节点中访问 input

```typescript
import { VariableAccessor } from 'sdk/workflow/execution/utils/variable-accessor.js';

// 创建访问器
const accessor = new VariableAccessor(workflowExecution);

// 访问 input 数据
const userName = accessor.get('input.userName');        // 'Alice'
const timeout = accessor.get('input.config.timeout');   // 5000
const firstItem = accessor.get('input.items[0]');       // 'item1'

// 检查是否存在
if (accessor.has('input.userName')) {
  // 执行某些逻辑
}
```

### 3. 在模板中使用 input

```handlebars
<!-- 在 Prompt 模板中 -->
Hello {{input.userName}}!
Your configuration:
- Timeout: {{input.config.timeout}}ms
- Retry count: {{input.config.retry}}

Items to process:
{{#each input.items}}
- {{this}}
{{/each}}
```

### 4. 设置 output

```typescript
// 在处理逻辑完成后设置输出
workflowExecution.setOutput({
  success: true,
  message: 'Processing completed',
  data: {
    processedCount: 3,
    results: ['result1', 'result2', 'result3']
  },
  metadata: {
    executionTime: Date.now()
  }
});
```

### 5. 访问 output

```typescript
// 通过访问器
const accessor = new VariableAccessor(workflowExecution);
const success = accessor.get('output.success');        // true
const count = accessor.get('output.data.processedCount'); // 3

// 直接获取
const output = workflowExecution.getOutput();
console.log(output.message); // 'Processing completed'
```

### 6. 获取执行结果

```typescript
import { WorkflowExecutor } from 'sdk/workflow/execution/executors/workflow-executor.js';

const executor = new WorkflowExecutor({
  graphRegistry,
  workflowExecutionCoordinatorFactory
});

// 执行 WorkflowExecution
const result = await executor.execute(workflowExecution);

// 获取 output
console.log(result.output);  // 完整的输出对象
console.log(result.executionTime);  // 执行时间
console.log(result.nodeResults);    // 节点执行结果
```

---

## 常见场景示例

### 场景 1: 条件判断

```typescript
// 根据 input 中的配置决定是否执行某操作
const retry = accessor.get('input.config.retry');
if (retry > 0) {
  // 执行重试逻辑
}
```

### 场景 2: 数据转换

```typescript
// 读取 input 数据，处理后设置到 output
const items = accessor.get('input.items') as string[];
const processedItems = items.map(item => item.toUpperCase());

workflowExecution.setOutput({
  originalCount: items.length,
  processedItems: processedItems
});
```

### 场景 3: 错误处理

```typescript
try {
  // 执行某些操作
  workflowExecution.setOutput({
    success: true,
    data: result
  });
} catch (error) {
  workflowExecution.setOutput({
    success: false,
    error: error.message,
    code: 'PROCESSING_FAILED'
  });
}
```

### 场景 4: 累积结果

```typescript
// 在循环中累积结果
let accumulated = accessor.get('output.accumulated') || [];
accumulated = [...accumulated, newItem];

const currentOutput = workflowExecution.getOutput();
workflowExecution.setOutput({
  ...currentOutput,
  accumulated: accumulated
});
```

---

## 注意事项

### ✅ 推荐做法

1. **保持 input 结构清晰**
   ```typescript
   // 好的做法
   input: {
     user: { id: 1, name: 'Alice' },
     config: { timeout: 5000 }
   }
   ```

2. **明确命名空间**
   ```typescript
   // 明确使用 input. 和 output. 前缀
   accessor.get('input.userName')
   accessor.get('output.result')
   ```

3. **在 END 节点设置 output**
   ```typescript
   // 在最后一个节点或 END 节点设置最终输出
   workflowExecution.setOutput(finalResult);
   ```

### ❌ 避免做法

1. **不要修改 input**
   ```typescript
   // ❌ 错误：input 是只读的
   const input = workflowExecution.getInput();
   input.userName = 'Bob'; // 不应该这样做
   ```

2. **不要过早设置 output**
   ```typescript
   // ❌ 错误：在中间节点设置 output 可能被覆盖
   // ✅ 正确：在 END 节点设置最终输出
   ```

3. **不要混淆 input 和变量**
   ```typescript
   // ❌ 混淆：将 input 数据赋值给变量后忘记来源
   const userName = accessor.get('input.userName');
   // 后续使用 userName 时忘记它来自 input
   
   // ✅ 清晰：始终通过命名空间访问
   const userName = accessor.get('input.userName');
   ```

---

## API 参考

### WorkflowExecutionOptions

```typescript
interface WorkflowExecutionOptions {
  input?: Record<string, unknown>;  // 输入数据
  maxSteps?: number;
  timeout?: number;
  // ... 其他选项
}
```

### WorkflowExecutionEntity 方法

```typescript
// 获取 input
getInput(): Record<string, unknown>

// 获取 output
getOutput(): Record<string, unknown>

// 设置 output
setOutput(output: Record<string, unknown>): void
```

### VariableAccessor 方法

```typescript
// 获取变量值
get(path: string): unknown

// 检查变量是否存在
has(path: string): boolean
```

### WorkflowExecutionResult

```typescript
interface WorkflowExecutionResult {
  executionId: string;
  output: Record<string, unknown>;  // 输出数据
  executionTime: number;
  nodeResults: NodeExecutionResult[];
  metadata: WorkflowExecutionResultMetadata;
}
```

---

**相关文档**: 
- [详细分析](./analysis.md)
- [VariableAccessor 实现](../../workflow/execution/utils/variable-accessor.ts)
- [WorkflowExecution 类型定义](../../../../packages/types/src/workflow-execution/definition.ts)
