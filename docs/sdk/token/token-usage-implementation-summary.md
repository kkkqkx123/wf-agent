# Token统计功能实现总结

## 一、修改概述

根据分析文档 [`docs/token-usage-analysis.md`](docs/token-usage-analysis.md) 的建议，成功实现了基于数组+总量的token统计方式，支持历史记录、精确回退和统计分析功能。

## 二、修改内容

### 2.1 Types层修改

#### 文件：`sdk/types/llm.ts`

**新增接口**：

1. **TokenUsageHistory** - Token使用历史记录
```typescript
export interface TokenUsageHistory {
  requestId: string;           // 请求ID
  timestamp: number;           // 时间戳
  promptTokens: number;        // 提示token数
  completionTokens: number;    // 完成token数
  totalTokens: number;         // 总token数
  cost?: number;               // 成本（可选）
  model?: string;              // 模型名称（可选）
  rawUsage?: LLMUsage;         // 原始usage数据
}
```

2. **TokenUsageStatistics** - Token使用统计信息
```typescript
export interface TokenUsageStatistics {
  totalRequests: number;           // 总请求数
  averageTokens: number;           // 平均token数
  maxTokens: number;               // 最大token数
  minTokens: number;               // 最小token数
  totalCost: number;               // 总成本
  totalPromptTokens: number;       // 总提示token数
  totalCompletionTokens: number;   // 总完成token数
}
```

### 2.2 Core层修改

#### 文件：`sdk/core/execution/token-usage-tracker.ts`

**新增字段**：
- `usageHistory: TokenUsageHistory[]` - 历史记录数组
- `enableHistory: boolean` - 是否启用历史记录
- `maxHistorySize: number` - 最大历史记录数量

**修改的方法**：

1. **构造函数** - 添加历史记录配置选项
```typescript
constructor(options: TokenUsageTrackerOptions = {}) {
  this.tokenLimit = options.tokenLimit || 4000;
  this.enableHistory = options.enableHistory ?? true;
  this.maxHistorySize = options.maxHistorySize || 1000;
}
```

2. **accumulateStreamUsage()** - 修复流式处理统计问题
   - **问题**：原实现直接覆盖cumulativeUsage，导致统计错误
   - **修复**：不再在流式传输期间更新cumulativeUsage，只在finalizeCurrentRequest()中统一累加

3. **finalizeCurrentRequest()** - 添加历史记录功能
   - 在累加到cumulativeUsage和totalLifetimeUsage后
   - 调用addToHistory()将当前请求添加到历史记录

4. **reset()** 和 **fullReset()** - 清空历史记录
   - reset()：清空cumulativeUsage、currentRequestUsage和usageHistory
   - fullReset()：额外清空totalLifetimeUsage

5. **clone()** - 克隆历史记录
   - 克隆时包含usageHistory数组

**新增的方法**：

1. **addToHistory(usage: TokenUsageStats)** - 添加到历史记录（私有方法）
   - 创建TokenUsageHistory对象
   - 添加到usageHistory数组
   - 限制历史记录数量（超过maxHistorySize时删除最旧的记录）

2. **getUsageHistory()** - 获取历史记录
   - 返回usageHistory数组的副本

3. **getRecentHistory(n: number)** - 获取最近N条历史记录
   - 返回usageHistory的最后n条记录

4. **getStatistics()** - 获取统计信息
   - 计算总请求数、平均token数、最大/最小token数、总成本等

5. **rollbackToRequest(requestIndex: number)** - 回退到指定请求之前
   - 重新计算cumulativeUsage
   - 删除回退后的历史记录

6. **rollbackToRequestId(requestId: string)** - 回退到指定请求ID之前
   - 查找请求ID对应的索引
   - 调用rollbackToRequest()

7. **rollbackToTimestamp(timestamp: number)** - 回退到指定时间戳之前
   - 查找时间戳对应的索引
   - 调用rollbackToRequest()

8. **clearHistory()** - 清空历史记录
   - 清空usageHistory数组

#### 文件：`sdk/core/execution/conversation.ts`

**修改**：
- 导入TokenUsageHistory类型
- 新增getUsageHistory()方法 - 获取Token使用历史记录

#### 文件：`sdk/core/execution/managers/conversation-state-manager.ts`

**修改**：
- 导入TokenUsageHistory和TokenUsageStatistics类型
- ConversationState接口新增usageHistory字段
- getState()方法返回usageHistory
- createSnapshot()方法包含usageHistory

**新增方法**：
1. **getUsageHistory()** - 获取Token使用历史记录
2. **getRecentUsageHistory(n: number)** - 获取最近N条历史记录
3. **getTokenUsageStatistics()** - 获取统计信息
4. **rollbackToRequest(requestIndex: number)** - 回退到指定请求之前
5. **rollbackToRequestId(requestId: string)** - 回退到指定请求ID之前
6. **rollbackToTimestamp(timestamp: number)** - 回退到指定时间戳之前

## 三、关键改进

### 3.1 修复流式处理统计问题

**原问题**：
```typescript
// 原实现 - 直接覆盖cumulativeUsage
this.cumulativeUsage.promptTokens = this.currentRequestUsage.promptTokens;
```

**修复后**：
```typescript
// 不再在流式传输期间更新cumulativeUsage
// 只在finalizeCurrentRequest()中统一累加
```

### 3.2 支持历史记录

**优势**：
- 记录每次API调用的详细信息
- 支持历史查询和分析
- 支持精确回退

**配置选项**：
```typescript
const tracker = new TokenUsageTracker({
  tokenLimit: 4000,
  enableHistory: true,        // 启用历史记录
  maxHistorySize: 1000        // 最多保存1000条记录
});
```

### 3.3 支持精确回退

**回退方式**：
1. 按索引回退：`rollbackToRequest(5)`
2. 按请求ID回退：`rollbackToRequestId('req-123')`
3. 按时间戳回退：`rollbackToTimestamp(Date.now() - 3600000)`

### 3.4 支持统计分析

**统计信息**：
- 总请求数
- 平均token数
- 最大/最小token数
- 总成本
- 总提示/完成token数

## 四、向后兼容性

### 4.1 保留原有API

所有原有的方法保持不变：
- `getCumulativeUsage()` - 获取累积总量
- `getTotalLifetimeUsage()` - 获取生命周期总量
- `getCurrentRequestUsage()` - 获取当前请求统计
- `getTokenUsage(messages)` - 获取token使用情况
- `isTokenLimitExceeded(messages)` - 检查是否超过限制

### 4.2 新增可选功能

新增的功能都是可选的，不影响现有代码：
- 历史记录功能默认启用，但可以通过配置关闭
- 回退功能是新增的，不影响现有逻辑
- 统计功能是新增的，不影响现有逻辑

### 4.3 检查点兼容

ConversationState新增了usageHistory字段，但它是可选的：
```typescript
export interface ConversationState {
  messages: LLMMessage[];
  tokenUsage: TokenUsageStats | null;
  currentRequestUsage: TokenUsageStats | null;
  usageHistory?: TokenUsageHistory[];  // 可选字段
}
```

## 五、使用示例

### 5.1 基本使用

```typescript
// 创建tracker
const tracker = new TokenUsageTracker({
  tokenLimit: 4000,
  enableHistory: true,
  maxHistorySize: 1000
});

// 更新API使用统计
tracker.updateApiUsage({
  promptTokens: 100,
  completionTokens: 50,
  totalTokens: 150
});

// 完成当前请求
tracker.finalizeCurrentRequest();

// 获取累积总量
const cumulative = tracker.getCumulativeUsage();
console.log(cumulative?.totalTokens); // 150

// 获取历史记录
const history = tracker.getUsageHistory();
console.log(history.length); // 1
```

### 5.2 历史查询

```typescript
// 获取最近10条历史记录
const recentHistory = tracker.getRecentHistory(10);

// 获取统计信息
const stats = tracker.getStatistics();
console.log(stats.totalRequests);      // 总请求数
console.log(stats.averageTokens);      // 平均token数
console.log(stats.totalCost);          // 总成本
```

### 5.3 精确回退

```typescript
// 回退到第5次请求之前
tracker.rollbackToRequest(5);

// 回退到指定请求ID之前
tracker.rollbackToRequestId('req-123');

// 回退到1小时之前
tracker.rollbackToTimestamp(Date.now() - 3600000);
```

### 5.4 流式处理

```typescript
// 流式传输期间持续更新
tracker.accumulateStreamUsage({
  promptTokens: 100,
  completionTokens: 10,
  totalTokens: 110
});

// 完成流式传输
tracker.finalizeCurrentRequest();
```

## 六、测试建议

### 6.1 单元测试

需要为以下功能添加单元测试：

1. **历史记录功能**
   - 测试历史记录的添加
   - 测试历史记录的数量限制
   - 测试历史记录的查询

2. **回退功能**
   - 测试按索引回退
   - 测试按请求ID回退
   - 测试按时间戳回退
   - 测试回退后的cumulativeUsage计算

3. **统计功能**
   - 测试统计信息的计算
   - 测试空历史记录的统计

4. **流式处理**
   - 测试流式传输的token统计
   - 测试多次流式调用的累加

### 6.2 集成测试

需要测试以下场景：

1. **检查点恢复**
   - 测试包含历史记录的检查点保存和恢复
   - 测试回退后的检查点保存和恢复

2. **多轮对话**
   - 测试多轮对话的token统计
   - 测试历史记录的累积

3. **并发访问**
   - 测试并发访问历史记录的安全性

## 七、性能考虑

### 7.1 内存占用

**历史记录的内存占用**：
- 每条历史记录约200-300字节
- 1000条记录约200-300KB
- 可通过maxHistorySize控制

**优化建议**：
- 对于长时间运行的线程，可以设置较小的maxHistorySize
- 可以定期清理旧的历史记录
- 可以考虑持久化历史记录到外部存储

### 7.2 性能开销

**历史记录的添加**：
- 时间复杂度：O(1)
- 空间复杂度：O(1)

**历史记录的查询**：
- getUsageHistory()：O(n)
- getRecentHistory(n)：O(n)
- getStatistics()：O(n)

**回退操作**：
- rollbackToRequest()：O(n)
- rollbackToRequestId()：O(n)
- rollbackToTimestamp()：O(n)

**优化建议**：
- 对于频繁的查询操作，可以考虑缓存统计信息
- 对于大规模历史记录，可以考虑使用更高效的数据结构

## 八、未来扩展

### 8.1 持久化支持

可以添加历史记录的持久化功能：
- 保存到数据库
- 支持历史记录的导入导出
- 支持历史记录的压缩

### 8.2 高级统计

可以添加更高级的统计功能：
- 百分位数计算（P50, P90, P95, P99）
- 趋势分析
- 异常检测
- 成本预测

### 8.3 可视化

可以添加可视化功能：
- Token消耗趋势图
- 成本分布图
- 模型使用统计图

## 九、总结

### 9.1 实现的功能

✅ Token使用历史记录
✅ 精确回退功能
✅ 统计分析功能
✅ 修复流式处理统计问题
✅ 向后兼容
✅ 类型安全

### 9.2 修改的文件

1. `sdk/types/llm.ts` - 新增类型定义
2. `sdk/core/execution/token-usage-tracker.ts` - 核心实现
3. `sdk/core/execution/conversation.ts` - 添加历史记录方法
4. `sdk/core/execution/managers/conversation-state-manager.ts` - 添加历史记录和回退方法

### 9.3 类型检查

✅ 所有修改通过TypeScript类型检查

### 9.4 下一步

1. 添加单元测试
2. 添加集成测试
3. 更新文档
4. 性能测试和优化

## 十、参考资料

- 分析文档：[`docs/token-usage-analysis.md`](docs/token-usage-analysis.md)
- TokenUsageTracker实现：[`sdk/core/execution/token-usage-tracker.ts`](sdk/core/execution/token-usage-tracker.ts)
- 类型定义：[`sdk/types/llm.ts`](sdk/types/llm.ts)