# DATA_PROCESSOR 节点使用指南

## 概述

DATA_PROCESSOR 节点是工作流中的统一数据处理节点，用于处理三大数据域的操作：

1. **Message 操作** - LLM 对话历史管理（现有功能）
2. **Variable 操作** - 工作流运行时变量处理（新增）

> **新功能**：Variable 操作解决了多分支数据聚合、变量转换等场景，替代了对 SCRIPT 节点的不当使用。

---

## Variable 操作

### 1. 聚合操作 (Aggregate)

将多个变量合并为一个，支持三种模式。

#### 使用场景

**场景：多分支 LLM 分析聚合**

```yaml
节点配置:
  type: DATA_PROCESSOR
  config:
    variableOperation:
      operation: aggregate
      sourceVariables:
        - result_a      # 分支 A 的分析结果
        - result_b      # 分支 B 的数据库查询结果
        - result_c      # 分支 C 的 API 调用结果
      targetVariable: all_results
      aggregateMode: array
```

**执行结果**：

```javascript
all_results = [result_a, result_b, result_c]
```

#### Array 模式

将源变量值组织为数组。

```yaml
variableOperation:
  operation: aggregate
  sourceVariables: [var1, var2, var3]
  targetVariable: combined
  aggregateMode: array
```

结果：`combined = [var1_value, var2_value, var3_value]`

#### Object 模式

将源变量映射为对象的键值对。

```yaml
variableOperation:
  operation: aggregate
  sourceVariables: 
    - sentiment_analysis
    - intent_detection
    - entity_extraction
  targetVariable: analysis_results
  aggregateMode: object
  keyMapping:
    sentiment_analysis: sentiment
    intent_detection: intent
    entity_extraction: entities
```

结果：

```javascript
analysis_results = {
  sentiment: <sentiment_analysis_value>,
  intent: <intent_detection_value>,
  entities: <entity_extraction_value>
}
```

#### Merge 模式

深度合并多个对象。

```yaml
variableOperation:
  operation: aggregate
  sourceVariables: 
    - config_a
    - config_b
    - config_c
  targetVariable: merged_config
  aggregateMode: merge
  mergeStrategy: deep  # 或 shallow
```

结果：`merged_config = {...config_a, ...config_b, ...config_c}` （深合并）

#### 条件聚合

可选：聚合前过滤数组元素。

```yaml
variableOperation:
  operation: aggregate
  sourceVariables: [all_results]
  targetVariable: successful_results
  aggregateMode: array
  filterExpression:
    expression: "item.status === 'success'"
```

---

### 2. 转换操作 (Transform)

使用表达式转换单个变量的值。

#### 使用场景

```yaml
# 提取数组中的特定字段
variableOperation:
  operation: transform
  sourceVariable: user_list
  targetVariable: user_ids
  transformExpression: "user_list.map(u => u.id)"
  outputType: array

# 计算派生值
variableOperation:
  operation: transform
  sourceVariable: raw_score
  targetVariable: normalized_score
  transformExpression: "raw_score / 100 * 10"
  outputType: number

# 字符串格式化
variableOperation:
  operation: transform
  sourceVariable: data
  targetVariable: formatted
  transformExpression: "'Result: ' + data.toString()"
  outputType: string
```

#### 支持的表达式

转换表达式使用现有的 **AST-based evaluator**，支持：

- **数组方法**：`map`, `filter`, `reduce`, `find`, `includes`
- **字符串方法**：`substring`, `toUpperCase`, `split`, 等
- **算术运算**：`+`, `-`, `*`, `/`, `%`
- **比较运算**：`>`, `<`, `===`, `!==`
- **逻辑运算**：`&&`, `||`, `!`
- **三元运算**：`condition ? true_val : false_val`

#### 表达式上下文

表达式可以访问所有当前变量：

```yaml
transformExpression: "count * factor + bonus"
# 有权访问：count, factor, bonus 变量
```

---

### 3. 批量更新操作 (Batch Update)

原子性地更新多个变量，表达式可互相引用。

#### 使用场景

```yaml
variableOperation:
  operation: batch-update
  updates:
    - name: subtotal
      expression: "price * quantity"
      type: number
    
    - name: tax
      expression: "subtotal * tax_rate"
      type: number
    
    - name: total
      expression: "subtotal + tax"
      type: number
    
    - name: status
      expression: "total > 100 ? 'premium' : 'standard'"
      type: string
```

#### 执行顺序

批量更新**按列表顺序执行**，后续更新可以引用前面已更新的变量：

```yaml
updates:
  - name: a
    expression: "10"    # a = 10
  
  - name: b
    expression: "a * 2" # b = a * 2 = 20 (使用刚更新的 a)
  
  - name: c
    expression: "a + b" # c = a + b = 30 (使用已更新的 a 和 b)
```

#### 跳过只读变量

系统自动跳过标记为 readonly 的变量：

```typescript
readonly?: boolean  // 定义时标记的只读标志
```

---

## 实际示例

### 示例 1：多分支 LLM 聚合

**工作流结构**：

```
FORK (创建3个并行分支)
  ├─ Branch A: LLM 分析用户情感
  │   output: sentiment_result
  │
  ├─ Branch B: 查询数据库历史
  │   output: history_data
  │
  └─ Branch C: 调用推荐 API
      output: recommendations

JOIN (合并分支)
  variableOutputs:
    - internalName: sentiment_result → externalName: sentiment
    - internalName: history_data → externalName: history
    - internalName: recommendations → externalName: recs

DATA_PROCESSOR (聚合结果)
  operation: aggregate
  sourceVariables: [sentiment, history, recs]
  targetVariable: analysis
  aggregateMode: object
  keyMapping:
    sentiment: sentiment_analysis
    history: historical_context
    recs: recommendations

LLM (使用聚合结果)
  prompt: "Based on this analysis: {analysis}, provide insights..."
```

### 示例 2：条件聚合与转换

**场景**：从多个 API 调用的结果中，只收集成功的响应

```
FORK (调用多个 API)
  └─ 每个分支返回 { status, data, error }

JOIN
  variableOutputs:
    - 每个分支结果映射为 api_result_1, api_result_2, ...

DATA_PROCESSOR (转换：提取数据)
  operation: transform
  sourceVariable: api_result_1
  targetVariable: api_data_1
  transformExpression: "api_result_1.data || {}"

DATA_PROCESSOR (聚合：只收集成功)
  operation: aggregate
  sourceVariables: [api_data_1, api_data_2, api_data_3]
  targetVariable: successful_apis
  aggregateMode: array
  filterExpression:
    expression: "item && typeof item === 'object' && Object.keys(item).length > 0"
```

### 示例 3：计算派生字段

**场景**：购物订单的价格计算

```
VARIABLE (输入)
  items: [
    { name: "product1", price: 100, qty: 2 },
    { name: "product2", price: 50, qty: 1 }
  ]
  tax_rate: 0.1
  discount_rate: 0.05

DATA_PROCESSOR (计算)
  operation: batch-update
  updates:
    - name: subtotal
      expression: "items.reduce((sum, item) => sum + item.price * item.qty, 0)"
      type: number
    
    - name: discount_amount
      expression: "subtotal * discount_rate"
      type: number
    
    - name: subtotal_after_discount
      expression: "subtotal - discount_amount"
      type: number
    
    - name: tax_amount
      expression: "subtotal_after_discount * tax_rate"
      type: number
    
    - name: total
      expression: "subtotal_after_discount + tax_amount"
      type: number
    
    - name: order_summary
      expression: "{
        subtotal: subtotal,
        discount: discount_amount,
        tax: tax_amount,
        total: total,
        item_count: items.length
      }"
      type: object
```

---

## 与其他节点的关系

### vs. VARIABLE 节点

| 特性 | VARIABLE | DATA_PROCESSOR |
|------|----------|---------|
| 用途 | 单变量赋值 | 多变量聚合/转换 |
| 输入 | 一个表达式 | 多个变量或单个变量 |
| 输出 | 一个变量 | 一个或多个变量 |
| 典型场景 | 简单计算 | 复杂聚合、批量更新 |

**协同使用**：

```
VARIABLE 节点：初始化简单值
  input_count = 10

DATA_PROCESSOR：聚合和转换
  aggregate 操作：合并多个计数
  transform 操作：格式化结果
```

### vs. SCRIPT 节点

| 特性 | SCRIPT | DATA_PROCESSOR |
|------|--------|---------|
| 用途 | I/O 操作 | 数据处理 |
| 执行上下文 | JavaScript 沙箱 | AST-based 表达式 |
| 性能 | 较低（沙箱开销） | 较高（原生实现） |
| 安全性 | 沙箱隔离 | 表达式限制 |

**迁移指南**：

❌ **不要用 SCRIPT 做这些**（改用 DATA_PROCESSOR）：

```javascript
// SCRIPT 中的反面例子
data = {
  sentiment: branch_a_result.sentiment,
  intent: branch_b_result.intent,
  entities: branch_c_result.entities
}

// ✅ DATA_PROCESSOR 替代品
variableOperation:
  operation: aggregate
  sourceVariables: [sentiment, intent, entities]
  targetVariable: data
  aggregateMode: object
  keyMapping: { ... }
```

✅ **SCRIPT 的正确用途**：

```javascript
// 调用外部 API
response = await fetch('https://api.example.com/...')

// 读写文件
fs.writeFileSync(...)

// 执行系统命令
shell('...')
```

---

## 性能考虑

### 优化建议

1. **批量操作优于链式调用**

   ❌ 不推荐：
   ```yaml
   DATA_PROCESSOR (更新 a)
     update: a = 10
   
   DATA_PROCESSOR (更新 b，依赖 a)
     update: b = a * 2
   ```

   ✅ 推荐：
   ```yaml
   DATA_PROCESSOR (批量更新)
     updates:
       - a = 10
       - b = a * 2
   ```

2. **使用合适的数据结构**

   - 频繁访问：使用 object（键值查找 O(1)）
   - 有序处理：使用 array（遍历）
   - 合并多源：使用 merge 模式

3. **避免深层嵌套**

   - Array 聚合：线性复杂度
   - Object merge：深合并可能较慢，大对象时使用 shallow

---

## 故障排除

### 常见错误

1. **"Source variable not found"**

   原因：引用的变量不存在或名称拼写错误

   解决：检查变量名拼写、确保变量已被初始化

2. **"Failed to evaluate expression"**

   原因：表达式语法错误

   解决：验证表达式的有效性（使用支持的操作符）

3. **"Cannot merge non-object value"**

   原因：Merge 模式用于非对象类型

   解决：切换到 array 或 object 模式，或使用 transform 先转换类型

### 调试技巧

- 检查节点的执行历史（NodeResults）
- 验证输入变量的值和类型
- 逐步构建复杂的表达式（从简单开始，逐步添加）
- 使用类型声明（outputType）确保结果类型正确

---

## 最佳实践

1. **清晰的变量命名**

   ```yaml
   # ✅ 好的
   sourceVariables: [user_sentiment, api_intent, database_entities]
   targetVariable: combined_analysis
   
   # ❌ 避免
   sourceVariables: [a, b, c]
   targetVariable: result
   ```

2. **明确的数据流**

   使用 keyMapping 让结构自解释：

   ```yaml
   keyMapping:
     user_sentiment: sentiment_score
     api_intent: detected_intent
     database_entities: referenced_entities
   ```

3. **验证聚合结果**

   在 LLM 或后续节点中验证聚合的数据结构

4. **文档化复杂表达式**

   在 transform 和 batch-update 中使用注释（如果系统支持）

---

## 参考

- [VariableOperationConfig](../../packages/types/src/node/configs/variable-operation-configs.ts)
- [处理器实现](../../sdk/workflow/execution/handlers/node-handlers/variable-operation-handlers.ts)
- [数据结构分析](./data-structure-analysis.md)
- [设计文档](./unified-data-processor-analysis.md)
