# SDK-Kit Result 模式迁移指南

## 概述

SDK-Kit 已重构为使用 Result 模式处理错误，完全替代了异常抛出的方式。这提供了更好的错误处理能力和类型安全性。

### 核心改进

- ✅ **无异常**：正常操作不抛异常，错误作为值返回
- ✅ **错误链**：利用 Result 的 `.andThen()` / `.orElse()` 进行函数式错误处理
- ✅ **类型安全**：完整的 TypeScript 类型推导
- ✅ **多错误收集**：在构建和验证阶段收集所有错误
- ✅ **SDK 兼容**：直接使用 SDK 的错误类型，无重复实现

## 迁移示例

### WorkflowBuilder（工作流定义）

#### 旧方式（异常）

```typescript
try {
  const template = kit.workflow()
    .create('my-wf')
    .node('n1', { type: 'START' })
    .node('n2', { type: 'LLM' })
    .edge('n1', 'n2')
    .build();
  
  console.log('Success', template.id);
} catch (error) {
  if (error instanceof KitError) {
    console.error(`[${error.code}] ${error.message}`);
  }
}
```

#### 新方式（Result - 推荐）

```typescript
// 方法 1: 链式调用
const result = kit.workflow()
  .create('my-wf')
  .andThen(builder => builder.node('n1', { type: 'START' }))
  .andThen(builder => builder.node('n2', { type: 'LLM' }))
  .andThen(builder => builder.edge('n1', 'n2'))
  .andThen(builder => builder.build());

if (result.isOk()) {
  const template = result.unwrap();
  console.log('Success', template.id);
} else {
  const errors = result.unwrapOrElse(e => Array.isArray(e) ? e : [e]);
  for (const error of errors) {
    console.error(`[${error.code}] ${error.message}`);
  }
}

// 方法 2: 显式检查
if (result.isErr()) {
  const error = result.error;
  console.error(`Error: ${error.message}`);
} else {
  const template = result.value;
  console.log('Built workflow:', template.id);
}

// 方法 3: map/andThen 组合
result
  .andThen(template => kit.resource().workflows().create(template))
  .andThen(id => kit.resource().workflows().read(id))
  .andThen(wf => {
    console.log('Workflow ready:', wf.id);
    return ok(wf);
  })
  .orElse(error => {
    console.error('Operation failed:', error.message);
    return err(error);
  });
```

### ResourceManager（资源操作）

#### 旧方式（异常）

```typescript
try {
  const id = await kit.resource()
    .workflows()
    .create(template);
  
  const workflow = await kit.resource()
    .workflows()
    .read(id);
  
  console.log('Workflow:', workflow.name);
} catch (error) {
  console.error('Failed:', error.message);
}
```

#### 新方式（Result）

```typescript
const createResult = await kit.resource()
  .workflows()
  .create(template);

if (createResult.isOk()) {
  const id = createResult.unwrap();
  
  const readResult = await kit.resource()
    .workflows()
    .read(id);
  
  if (readResult.isOk()) {
    const workflow = readResult.unwrap();
    console.log('Workflow:', workflow.name);
  } else {
    console.error('Read failed:', readResult.error.message);
  }
} else {
  console.error('Create failed:', createResult.error.message);
}

// 或使用 async/await + unwrapOrElse
const id = (await kit.resource().workflows().create(template))
  .unwrapOrElse(error => {
    console.error('Create failed:', error.message);
    throw error;  // 如果需要停止
  });

const workflow = (await kit.resource().workflows().read(id))
  .unwrapOrElse(error => {
    console.error('Read failed:', error.message);
    throw error;
  });
```

### ExecutionAPI（工作流执行）

#### 旧方式（异常）

```typescript
try {
  const result = await kit.execution()
    .workflow('my-wf')
    .input({ data: 'test' })
    .execute();
  
  if (result.status === 'completed') {
    console.log('Success:', result.output);
  }
} catch (error) {
  console.error('Execution failed:', error.message);
}
```

#### 新方式（Result）

```typescript
const execResult = await kit.execution()
  .workflow('my-wf')
  .input({ data: 'test' })
  .execute();

if (execResult.isOk()) {
  const result = execResult.unwrap();
  if (result.status === 'completed') {
    console.log('Success:', result.output);
  }
} else {
  const error = execResult.error;
  console.error(`[${error.code}] ${error.message}`);
  
  // 可以检查特定错误
  if (error.isCode('TIMEOUT')) {
    console.error('Execution timed out');
  }
}
```

## 错误类型层级

SDK-Kit 现在使用 SDK 的错误类型层级：

```
CommandError (SDK base)
  ├── CommandValidationError
  ├── CommandExecutionError
  ├── CommandNotFoundError
  ├── CommandTimeoutError
  ├── PermissionError
  ├── StateError
  ├── DependencyError
  └── CancelledError

KitError (extends CommandError)
  └── Kit 特定的操作错误
```

### 使用错误类型检查

```typescript
import { CommandTimeoutError, CommandNotFoundError } from '@wf-agent/sdk/api';
import { KitError } from '@wf-agent/sdk-kit';

const result = await kit.execution().workflow('wf').execute();

if (result.isErr()) {
  const error = result.error;
  
  // 检查 SDK 错误
  if (error instanceof CommandTimeoutError) {
    console.error('Timeout after', error.timeoutMs, 'ms');
  }
  
  // 检查 Kit 错误
  if (error instanceof KitError) {
    console.error('Kit error:', error.kitErrorCode);
  }
  
  // 检查错误码
  if (error.isCode('NOT_FOUND_ERROR')) {
    console.error('Resource not found');
  }
}
```

## Result 模式 API

### 基本方法

```typescript
// 创建 Result
const ok_value = ok(42);           // Ok<number>
const err_value = err(error);      // Err<KitError>

// 检查状态
result.isOk()      // boolean
result.isErr()     // boolean

// 提取值
result.unwrap()           // 返回值或抛异常
result.unwrapOrElse(fn)   // 返回值或调用 fn
result.value              // 类型安全的值访问
result.error              // 类型安全的错误访问
```

### 链式操作

```typescript
// 成功时继续，失败时中止
result
  .andThen(value => processValue(value))
  .andThen(processed => ok(processed * 2))
  .andThen(doubled => ok(doubled + 1))
  .orElse(error => {
    console.error('Chain failed:', error);
    return ok(0);  // 恢复
  });

// 映射值
result
  .andThen(value => ok(value.toUpperCase()))
  .orElse(error => ok('DEFAULT'));
```

## 多错误收集

WorkflowBuilder 在 `build()` 阶段收集所有验证错误：

```typescript
const result = kit.workflow()
  .create('my-wf')
  .andThen(b => b.node('n1', { type: '' }))      // ❌ 缺少 type
  .andThen(b => b.edge('missing', 'n1'))         // ❌ 节点不存在
  .andThen(b => b.build());

if (result.isErr()) {
  const errors = result.unwrapOrElse(e => Array.isArray(e) ? e : [e]);
  
  // ✅ 所有错误同时返回
  console.log(`Found ${errors.length} errors:`);
  errors.forEach(error => {
    console.log(`  - [${error.code}] ${error.message}`);
  });
}
```

## 向后兼容

如果需要使用异常的方式，可以手动转换：

```typescript
// 显式地转换为异常（仅在需要时）
const result = await kit.resource().workflows().create(template);

const id = result.unwrapOrElse(error => {
  throw error;  // 转换为异常
});
```

## 常见模式

### 模式 1: 条件处理

```typescript
const result = await kit.resource().workflows().read(id);

match(result)
  .case(isOk(), (wf) => console.log('Found:', wf.name))
  .case(isErr(), (error) => console.error('Not found:', error.message));
```

### 模式 2: 批量操作

```typescript
// 创建多个工作流
const results = await Promise.all(
  templates.map(t => kit.resource().workflows().create(t))
);

// 收集成功和失败
const successes = results.filter(r => r.isOk()).map(r => r.unwrap());
const failures = results.filter(r => r.isErr()).map(r => r.error);

console.log(`Created ${successes.length}, failed: ${failures.length}`);
```

### 模式 3: 条件链接

```typescript
const result = await kit.execution()
  .workflow(workflowId)
  .input(input)
  .execute();

await result
  .andThen(execResult => {
    if (execResult.status === 'completed') {
      return ok(execResult.output);
    } else {
      return err(new KitError(
        `Execution failed with status: ${execResult.status}`,
        'EXECUTION_FAILED'
      ));
    }
  })
  .andThen(output => {
    // 进一步处理
    return ok({ processed: output });
  });
```

## 迁移检查列表

- [ ] 用 `Result` 类型替换 `try-catch` 块
- [ ] 使用 `.andThen()` / `.orElse()` 进行错误链接
- [ ] 检查 `instanceof KitError` 替换为 `error.isCode()`
- [ ] 更新单元测试以验证 Result 返回值
- [ ] 验证错误处理的完整性（所有路径都被处理）
- [ ] 测试多错误收集场景（特别是 WorkflowBuilder）

## 常见问题

### Q: 如何获取完整的错误链信息？

```typescript
const result = await operation();
if (result.isErr()) {
  const error = result.error;
  console.log('Code:', error.code);
  console.log('Message:', error.message);
  console.log('Context:', error.context);
  console.log('Severity:', error.severity);
  console.log('Cause:', error.cause);  // 原始 SDK 错误
}
```

### Q: 如何在异步操作中使用 Result？

```typescript
const result = await Promise.all([
  kit.resource().workflows().create(wf1),
  kit.resource().workflows().create(wf2),
])
.then(results => {
  const ids = results
    .filter(r => r.isOk())
    .map(r => r.unwrap());
  return ok(ids);
})
.catch(error => err(new KitError('Operation failed', 'INTERNAL_ERROR')));
```

### Q: 如何与现有的 Promise/async-await 代码集成？

```typescript
// 保持异步方式，但处理 Result
async function processWorkflows(ids: string[]) {
  for (const id of ids) {
    const result = await kit.resource().workflows().read(id);
    
    if (result.isOk()) {
      const wf = result.unwrap();
      console.log('Processing:', wf.name);
    } else {
      console.error('Skip:', result.error.message);
    }
  }
}
```
