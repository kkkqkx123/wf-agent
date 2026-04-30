# Thread Input/Output 使用案例

## 案例 1: 用户数据处理器

### 场景描述
处理用户提交的数据，验证后返回处理结果。

### Workflow 配置

```toml
# workflow.toml
[variables]
processedData = { type = "object", scope = "thread", default = {} }

[start]
type = "START"

[validate]
type = "LLM"
config = {
  prompt = """验证用户数据：{{input.userData}}
  检查必填字段和格式。
  """
}

[process]
type = "SCRIPT"
config = {
  script = """
  const userData = input.userData;
  const result = {
    id: userData.id,
    processedAt: new Date().toISOString(),
    status: 'processed'
  };
  return result;
  """
}

[output]
type = "END"
config = {
  output = {
    success = true,
    data = "{{processedData}}",
    timestamp = "{{input.timestamp}}"
  }
}
```

### 执行代码

```typescript
// 创建 Thread 并传入 input
const threadEntity = await threadBuilder.build('user-data-processor', {
  input: {
    userData: {
      id: 12345,
      name: 'Alice',
      email: 'alice@example.com',
      age: 28
    },
    timestamp: Date.now(),
    options: {
      validateEmail: true,
      normalizeName: true
    }
  }
});

// 执行
const result = await threadExecutor.executeThread(threadEntity);

// 结果
console.log(result.output);
// {
//   success: true,
//   data: { id: 12345, processedAt: '2026-01-XX...', status: 'processed' },
//   timestamp: 1234567890
// }
```

---

## 案例 2: 批量数据处理

### 场景描述
批量处理多个项目，汇总结果。

### 执行代码

```typescript
// 传入批量数据
const threadEntity = await threadBuilder.build('batch-processor', {
  input: {
    items: [
      { id: 1, type: 'document', content: '...' },
      { id: 2, type: 'image', content: '...' },
      { id: 3, type: 'video', content: '...' }
    ],
    config: {
      batchSize: 2,
      parallel: true
    }
  }
});

// 在循环节点中处理
// 访问当前 item: input.items[loop.index]
// 或: {{input.items[loop.index].type}}

// 最终输出
threadEntity.setOutput({
  totalProcessed: 3,
  successCount: 3,
  failedCount: 0,
  results: [
    { id: 1, status: 'success', processedAt: '...' },
    { id: 2, status: 'success', processedAt: '...' },
    { id: 3, status: 'success', processedAt: '...' }
  ],
  summary: {
    documents: 1,
    images: 1,
    videos: 1
  }
});
```

---

## 案例 3: 条件工作流

### 场景描述
根据输入配置决定执行不同的处理路径。

### 执行代码

```typescript
// 传入不同的配置
const threadEntity = await threadBuilder.build('conditional-workflow', {
  input: {
    mode: 'advanced',  // 或 'basic', 'minimal'
    data: { /* ... */ },
    options: {
      enableLogging: true,
      enableCache: false,
      retryCount: 3
    }
  }
});

// 在 Route 节点中根据 input.mode 分支
const mode = accessor.get('input.mode') as string;

if (mode === 'advanced') {
  // 执行完整流程
} else if (mode === 'basic') {
  // 执行简化流程
} else {
  // 执行最小流程
}

// 输出包含执行模式信息
threadEntity.setOutput({
  mode: mode,
  stepsExecuted: 15,
  features: ['logging', 'validation', 'transformation'],
  data: result
});
```

---

## 案例 4: 多步骤工作流

### 场景描述
复杂的多步骤工作流，每一步都依赖前一步的结果。

### 执行代码

```typescript
// 初始输入
const threadEntity = await threadBuilder.build('multi-step-workflow', {
  input: {
    taskId: 'task-001',
    requestData: {
      url: 'https://api.example.com/data',
      method: 'GET',
      headers: { 'Authorization': 'Bearer token' }
    },
    processingOptions: {
      transform: true,
      validate: true,
      cache: false
    }
  }
});

// 步骤 1: 获取数据
const fetchDataResult = await fetch(accessor.get('input.requestData.url'));
const rawData = await fetchDataResult.json();

// 步骤 2: 转换数据
const transformedData = transformData(rawData);

// 步骤 3: 验证数据
const validationResult = validateData(transformedData);

// 设置最终输出
threadEntity.setOutput({
  taskId: accessor.get('input.taskId'),
  status: validationResult.valid ? 'success' : 'failed',
  originalData: rawData,
  transformedData: transformedData,
  validation: validationResult,
  metadata: {
    fetchTime: fetchTime,
    transformTime: transformTime,
    validateTime: validateTime,
    totalProcessingTime: Date.now() - startTime
  }
});
```

---

## 案例 5: 错误处理工作流

### 场景描述
处理可能失败的操作，提供友好的错误信息。

### 执行代码

```typescript
try {
  // 尝试执行操作
  const inputData = accessor.get('input.data');
  const result = processInput(inputData);
  
  threadEntity.setOutput({
    success: true,
    data: result,
    message: 'Processing completed successfully'
  });
  
} catch (error) {
  // 捕获错误并设置错误输出
  threadEntity.setOutput({
    success: false,
    error: {
      code: 'PROCESSING_ERROR',
      message: error.message,
      details: error.details,
      inputValidation: validateInput(accessor.get('input.data'))
    },
    suggestions: [
      '检查输入数据格式',
      '确认网络连接',
      '联系技术支持'
    ]
  });
}
```

---

## 案例 6: Fork-Join 场景

### 场景描述
并行处理多个任务，然后汇总结果。

### 执行代码

```typescript
// 父线程
const parentThread = await threadBuilder.build('parent-workflow', {
  input: {
    items: [1, 2, 3, 4, 5],
    config: { timeout: 5000 }
  }
});

// Fork 创建子线程
const forkThreads = [];
for (const item of parentThread.getInput().items) {
  const forkThread = await threadBuilder.createFork(parentThread, {
    forkId: 'parallel-processing',
    startNodeId: 'process-item'
  });
  
  // 子线程继承 input，但可以在节点中修改自己的变量
  forkThread.setVariable('currentItem', item);
  forkThreads.push(forkThread);
}

// 并行执行所有子线程
const results = await Promise.all(
  forkThreads.map(thread => threadExecutor.executeThread(thread))
);

// Join: 汇总结果
const aggregatedOutput = {
  totalItems: results.length,
  successful: results.filter(r => r.output.success).length,
  failed: results.filter(r => !r.output.success).length,
  results: results.map(r => r.output)
};

parentThread.setOutput(aggregatedOutput);
```

---

## 案例 7: 子工作流触发

### 场景描述
主工作流触发子工作流，传递输入并接收输出。

### 执行代码

```typescript
// 主工作流
const mainThread = await threadBuilder.build('main-workflow', {
  input: {
    orderId: 'ORD-12345',
    customer: { id: 100, name: 'Alice' },
    items: [
      { productId: 'P001', quantity: 2 },
      { productId: 'P002', quantity: 1 }
    ]
  }
});

// 触发子工作流：订单验证
const validationThread = await threadBuilder.build('order-validation', {
  input: {
    orderId: mainThread.getInput().orderId,
    items: mainThread.getInput().items
  }
});

const validationResult = await threadExecutor.executeThread(validationThread);

// 触发子工作流：库存检查
const inventoryThread = await threadBuilder.build('inventory-check', {
  input: {
    items: mainThread.getInput().items,
    warehouse: 'WH-001'
  }
});

const inventoryResult = await threadExecutor.executeThread(inventoryThread);

// 汇总所有子工作流结果
mainThread.setOutput({
  orderId: mainThread.getInput().orderId,
  customer: mainThread.getInput().customer,
  validation: validationResult.output,
  inventory: inventoryResult.output,
  canProceed: validationResult.output.valid && inventoryResult.output.available,
  timestamp: Date.now()
});
```

---

## 案例 8: 模板表达式使用

### 场景描述
在 Prompt 模板和条件表达式中使用 input/output。

### Workflow 配置

```toml
[llm-node]
type = "LLM"
config = {
  prompt = """
  用户信息:
  - 姓名：{{input.userName}}
  - 年龄：{{input.userAge}}
  - 会员等级：{{input.membership.level}}
  
  处理请求:
  {{input.request.description}}
  
  配置:
  - 优先级：{{input.config.priority}}
  - 超时：{{input.config.timeout}}ms
  
  请根据以上信息生成响应。
  """
}

[condition-node]
type = "ROUTE"
config = {
  conditions = [
    { condition = "{{input.userAge}} >= 18", next = "adult-flow" },
    { condition = "{{input.userAge}} < 18", next = "minor-flow" }
  ]
}
```

### 执行代码

```typescript
const threadEntity = await threadBuilder.build('template-workflow', {
  input: {
    userName: 'Alice',
    userAge: 25,
    membership: {
      level: 'gold',
      points: 1500
    },
    request: {
      description: '需要查询订单历史',
      urgency: 'high'
    },
    config: {
      priority: 1,
      timeout: 10000
    }
  }
});

// 在模板中，{{input.userName}} 会被替换为 'Alice'
// {{input.userAge}} 会被替换为 25
// {{input.membership.level}} 会被替换为 'gold'
```

---

## 最佳实践总结

### 1. 输入数据设计
- ✅ 使用结构化数据（嵌套对象）
- ✅ 保持合理的层级深度（不超过 4 层）
- ✅ 为复杂数据提供清晰的字段名
- ❌ 避免扁平化的大量字段
- ❌ 避免过深的嵌套

### 2. 输出数据设计
- ✅ 包含成功/失败状态
- ✅ 提供详细的错误信息（如果失败）
- ✅ 包含执行元数据（时间、步骤数等）
- ✅ 保持输出结构一致
- ❌ 避免只返回原始数据
- ❌ 避免输出结构频繁变化

### 3. 访问模式
- ✅ 始终使用命名空间前缀（input./output.）
- ✅ 在表达式中使用模板语法
- ✅ 对访问结果进行空值检查
- ❌ 假设 input/output 中一定存在某些字段
- ❌ 混合使用不同的访问方式

### 4. 错误处理
- ✅ 在输出中包含错误详情
- ✅ 提供可操作的错误建议
- ✅ 记录输入验证信息
- ❌ 只返回简单的错误消息
- ❌ 不记录导致错误的输入数据

---

**相关文档**:
- [详细分析](./analysis.md)
- [快速指南](./quick-start.md)
