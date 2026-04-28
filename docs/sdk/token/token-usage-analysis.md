# Token统计处理逻辑分析

## 一、当前实现分析

### 1.1 核心数据结构

#### TokenUsageStats 接口
位置：`sdk/core/execution/token-usage-tracker.ts:21-30`

```typescript
export interface TokenUsageStats {
  /** 提示 Token 数 */
  promptTokens: number;
  /** 完成 Token 数 */
  completionTokens: number;
  /** 总 Token 数 */
  totalTokens: number;
  /** 原始 API 响应的详细信息 */
  rawUsage?: any;
}
```

#### LLMUsage 接口
位置：`sdk/types/llm.ts:112-125`

```typescript
export interface LLMUsage {
  /** 提示token数 */
  promptTokens: number;
  /** 完成token数 */
  completionTokens: number;
  /** 总token数 */
  totalTokens: number;
  /** 提示token成本（可选） */
  promptTokensCost?: number;
  /** 完成token成本（可选） */
  completionTokensCost?: number;
  /** 总成本（可选） */
  totalCost?: number;
}
```

### 1.2 TokenUsageTracker 类的核心逻辑

位置：`sdk/core/execution/token-usage-tracker.ts:51-354`

#### 三个主要统计字段

1. **cumulativeUsage**: 累积的Token使用统计（会随回退恢复）
   - 记录当前有效状态的累积token消耗
   - 通过检查点恢复时会重置为检查点时的值

2. **currentRequestUsage**: 当前请求的Token使用统计
   - 临时保存当前API调用的token使用情况
   - 调用`finalizeCurrentRequest()`后累加到cumulativeUsage

3. **totalLifetimeUsage**: 生命周期总Token使用统计（无视回退）
   - 记录从线程创建到现在的所有token消耗
   - 即使回退也不会减少，反映真实的总消耗

#### 关键方法分析

##### 1. updateApiUsage(usage: LLMUsage)
位置：`token-usage-tracker.ts:69-77`

```typescript
updateApiUsage(usage: LLMUsage): void {
  // 保存当前请求的 usage
  this.currentRequestUsage = {
    promptTokens: usage.promptTokens,
    completionTokens: usage.completionTokens,
    totalTokens: usage.totalTokens,
    rawUsage: usage
  };
}
```

**功能**：保存当前请求的usage到currentRequestUsage，但不立即累加到总使用量。

##### 2. accumulateStreamUsage(usage: LLMUsage)
位置：`token-usage-tracker.ts:88-116`

```typescript
accumulateStreamUsage(usage: LLMUsage): void {
  if (!this.currentRequestUsage) {
    // 第一次收到 usage，通常是 message_start 事件
    this.currentRequestUsage = {
      promptTokens: usage.promptTokens,
      completionTokens: usage.completionTokens,
      totalTokens: usage.totalTokens,
      rawUsage: usage
    };
  } else {
    // 后续的 usage，通常是 message_delta 事件，进行增量更新
    this.currentRequestUsage.promptTokens = usage.promptTokens;
    this.currentRequestUsage.completionTokens = usage.completionTokens;
    this.currentRequestUsage.totalTokens = usage.totalTokens;
    this.currentRequestUsage.rawUsage = usage;
  }

  // 同步更新累计使用量
  if (!this.cumulativeUsage) {
    this.cumulativeUsage = { ...this.currentRequestUsage };
  } else {
    // 重新计算累计值：累计值 = 之前所有请求的总和 + 当前请求的最新值
    // 这里简化处理，直接使用当前请求的值
    // 更精确的做法是保存历史记录，但会增加复杂度
    this.cumulativeUsage.promptTokens = this.currentRequestUsage.promptTokens;
    this.cumulativeUsage.completionTokens = this.currentRequestUsage.completionTokens;
    this.cumulativeUsage.totalTokens = this.currentRequestUsage.totalTokens;
  }
}
```

**功能**：在流式传输期间持续更新token统计。

**问题**：代码注释中明确指出"这里简化处理，直接使用当前请求的值"，这会导致cumulativeUsage被覆盖而不是累加。

##### 3. finalizeCurrentRequest()
位置：`token-usage-tracker.ts:124-146`

```typescript
finalizeCurrentRequest(): void {
  if (this.currentRequestUsage) {
    if (!this.cumulativeUsage) {
      this.cumulativeUsage = { ...this.currentRequestUsage };
    } else {
      // 累加到总使用量
      this.cumulativeUsage.promptTokens += this.currentRequestUsage.promptTokens;
      this.cumulativeUsage.completionTokens += this.currentRequestUsage.completionTokens;
      this.cumulativeUsage.totalTokens += this.currentRequestUsage.totalTokens;
    }

    // 同时累加到生命周期总使用量（无视回退）
    if (!this.totalLifetimeUsage) {
      this.totalLifetimeUsage = { ...this.currentRequestUsage };
    } else {
      this.totalLifetimeUsage.promptTokens += this.currentRequestUsage.promptTokens;
      this.totalLifetimeUsage.completionTokens += this.currentRequestUsage.completionTokens;
      this.totalLifetimeUsage.totalTokens += this.currentRequestUsage.totalTokens;
    }

    this.currentRequestUsage = null;
  }
}
```

**功能**：将当前请求的usage累加到cumulativeUsage和totalLifetimeUsage。

##### 4. getCumulativeUsage()
位置：`token-usage-tracker.ts:152-154`

```typescript
getCumulativeUsage(): TokenUsageStats | null {
  return this.cumulativeUsage ? { ...this.cumulativeUsage } : null;
}
```

**功能**：获取累计的Token使用统计（会随回退恢复）。

##### 5. getTotalLifetimeUsage()
位置：`token-usage-tracker.ts:160-162`

```typescript
getTotalLifetimeUsage(): TokenUsageStats | null {
  return this.totalLifetimeUsage ? { ...this.totalLifetimeUsage } : null;
}
```

**功能**：获取生命周期总Token使用统计（无视回退，反映真实的总token消耗）。

##### 6. setState()
位置：`token-usage-tracker.ts:298-316`

```typescript
setState(
  cumulativeUsage: TokenUsageStats | null,
  currentRequestUsage?: TokenUsageStats | null
): void {
  if (cumulativeUsage) {
    this.cumulativeUsage = { ...cumulativeUsage };
  } else {
    this.cumulativeUsage = null;
  }

  if (currentRequestUsage !== undefined) {
    if (currentRequestUsage) {
      this.currentRequestUsage = { ...currentRequestUsage };
    } else {
      this.currentRequestUsage = null;
    }
  }
  // 不恢复 totalLifetimeUsage，保持生命周期统计
}
```

**功能**：从检查点恢复状态，但不恢复totalLifetimeUsage。

### 1.3 检查点恢复机制

位置：`sdk/core/execution/managers/checkpoint-manager.ts:216-219`

```typescript
// 恢复Token统计
conversationManager.getTokenUsageTracker().setState(
  checkpoint.threadState.conversationState.tokenUsage,
  checkpoint.threadState.conversationState.currentRequestUsage
);
```

**恢复流程**：
1. 从检查点读取tokenUsage和currentRequestUsage
2. 调用setState恢复cumulativeUsage和currentRequestUsage
3. **不恢复totalLifetimeUsage**，保持生命周期统计不变

### 1.4 ConversationState 接口

位置：`sdk/core/execution/managers/conversation-state-manager.ts:24-31`

```typescript
export interface ConversationState {
  /** 消息历史 */
  messages: LLMMessage[];
  /** 累积的 Token 使用统计 */
  tokenUsage: TokenUsageStats | null;
  /** 当前请求的 Token 使用统计 */
  currentRequestUsage: TokenUsageStats | null;
}
```

**注意**：ConversationState中只保存了tokenUsage和currentRequestUsage，没有保存totalLifetimeUsage。

## 二、当前实现的问题

### 2.1 没有历史记录

**问题描述**：
- 当前只保存了累积总量，没有记录每次API调用的详细信息
- 无法查看每次API调用的token消耗情况
- 无法分析token消耗的历史趋势

**影响**：
- 无法进行详细的成本分析
- 无法优化prompt以减少token消耗
- 无法追踪异常的token消耗

### 2.2 无法精确回退

**问题描述**：
- 虽然可以通过检查点恢复，但检查点是粗粒度的（基于节点执行）
- 无法精确回退到某次API调用之前的状态
- 回退只能基于检查点，不能基于API调用

**影响**：
- 在调试时无法精确控制回退粒度
- 无法撤销某次特定的API调用
- 回退操作不够灵活

### 2.3 流式处理的问题

**问题描述**：
在`accumulateStreamUsage`方法中（token-usage-tracker.ts:112-115）：

```typescript
// 重新计算累计值：累计值 = 之前所有请求的总和 + 当前请求的最新值
// 这里简化处理，直接使用当前请求的值
// 更精确的做法是保存历史记录，但会增加复杂度
this.cumulativeUsage.promptTokens = this.currentRequestUsage.promptTokens;
this.cumulativeUsage.completionTokens = this.currentRequestUsage.completionTokens;
this.cumulativeUsage.totalTokens = this.currentRequestUsage.totalTokens;
```

**问题**：
- 直接覆盖cumulativeUsage而不是累加
- 代码注释中明确指出这是简化处理
- 会导致统计不准确

**影响**：
- 流式响应期间的token统计不准确
- 多次流式调用时统计会出错

### 2.4 无法分析历史

**问题描述**：
- 没有保存每次调用的详细信息
- 无法计算平均值、最大值、最小值等统计指标
- 无法识别token消耗的模式

**影响**：
- 无法进行性能优化
- 无法进行成本预测
- 无法进行异常检测

## 三、是否应该采用数组+总量的统计方式？

### 3.1 优势分析

#### 1. 支持精确回退

**场景**：
- 可以回退到任意一次API调用之前的状态
- 支持基于requestId、时间戳、索引的回退
- 提供更细粒度的状态管理

**示例**：
```typescript
// 回退到第5次API调用之前
tracker.rollbackToRequest(5);

// 回退到特定请求ID之前
tracker.rollbackToRequestId('req-123');

// 回退到特定时间戳之前
tracker.rollbackToTimestamp(Date.now() - 3600000);
```

#### 2. 历史分析

**场景**：
- 可以分析每次API调用的token消耗趋势
- 可以计算平均值、最大值、最小值
- 可以识别token消耗的模式

**示例**：
```typescript
// 获取历史记录
const history = tracker.getUsageHistory();

// 计算平均token消耗
const avgTokens = history.reduce((sum, h) => sum + h.totalTokens, 0) / history.length;

// 找出最大消耗
const maxUsage = history.reduce((max, h) => h.totalTokens > max.totalTokens ? h : max);
```

#### 3. 成本追踪

**场景**：
- 可以追踪每次调用的成本
- 可以按模型、按时间段统计成本
- 可以生成成本报告

**示例**：
```typescript
// 按模型统计成本
const costByModel = tracker.getCostByModel();

// 按时间段统计成本
const costByTime = tracker.getCostByTimeRange(startTime, endTime);
```

#### 4. 调试友好

**场景**：
- 可以查看详细的调用历史
- 可以追踪token消耗的来源
- 可以快速定位问题

**示例**：
```typescript
// 查看最近的调用历史
const recentHistory = tracker.getRecentHistory(10);

// 查找异常消耗
const abnormalUsage = history.filter(h => h.totalTokens > threshold);
```

#### 5. 支持更复杂的统计

**场景**：
- 可以计算百分位数
- 可以进行趋势分析
- 可以进行预测

**示例**：
```typescript
// 计算P95
const p95 = tracker.getPercentile(95);

// 趋势分析
const trend = tracker.analyzeTrend();
```

### 3.2 劣势分析

#### 1. 内存占用

**问题**：
- 需要存储每次调用的详细信息
- 长时间运行的线程会积累大量历史记录
- 可能导致内存压力

**解决方案**：
- 提供配置选项控制历史记录的最大数量
- 提供自动清理机制
- 提供持久化选项

#### 2. 性能开销

**问题**：
- 每次调用都需要记录历史
- 需要维护历史记录的数据结构
- 可能影响性能

**解决方案**：
- 使用高效的数据结构（如环形缓冲区）
- 提供异步记录选项
- 提供采样选项

#### 3. 复杂度增加

**问题**：
- 需要管理历史记录的清理、查询等
- 需要处理并发访问
- 需要处理序列化/反序列化

**解决方案**：
- 提供清晰的API设计
- 提供完善的文档
- 提供单元测试

### 3.3 建议方案

#### 方案1：完全采用数组+总量（推荐）

**设计**：
```typescript
interface TokenUsageHistory {
  /** 请求ID */
  requestId: string;
  /** 时间戳 */
  timestamp: number;
  /** 提示token数 */
  promptTokens: number;
  /** 完成token数 */
  completionTokens: number;
  /** 总token数 */
  totalTokens: number;
  /** 成本（可选） */
  cost?: number;
  /** 模型名称（可选） */
  model?: string;
  /** 原始usage数据 */
  rawUsage?: LLMUsage;
}

interface TokenUsageTrackerOptions {
  /** Token限制阈值 */
  tokenLimit?: number;
  /** 是否启用历史记录 */
  enableHistory?: boolean;
  /** 最大历史记录数量 */
  maxHistorySize?: number;
  /** 是否持久化历史记录 */
  persistHistory?: boolean;
}

class TokenUsageTracker {
  private cumulativeUsage: TokenUsageStats | null = null;
  private currentRequestUsage: TokenUsageStats | null = null;
  private totalLifetimeUsage: TokenUsageStats | null = null;
  private usageHistory: TokenUsageHistory[] = [];
  private options: TokenUsageTrackerOptions;

  constructor(options: TokenUsageTrackerOptions = {}) {
    this.options = {
      tokenLimit: options.tokenLimit || 4000,
      enableHistory: options.enableHistory ?? true,
      maxHistorySize: options.maxHistorySize || 1000,
      persistHistory: options.persistHistory ?? false
    };
  }

  // 更新API使用统计
  updateApiUsage(usage: LLMUsage, requestId: string, model?: string): void {
    this.currentRequestUsage = {
      promptTokens: usage.promptTokens,
      completionTokens: usage.completionTokens,
      totalTokens: usage.totalTokens,
      rawUsage: usage
    };
  }

  // 完成当前请求
  finalizeCurrentRequest(): void {
    if (this.currentRequestUsage) {
      // 累加到cumulativeUsage
      if (!this.cumulativeUsage) {
        this.cumulativeUsage = { ...this.currentRequestUsage };
      } else {
        this.cumulativeUsage.promptTokens += this.currentRequestUsage.promptTokens;
        this.cumulativeUsage.completionTokens += this.currentRequestUsage.completionTokens;
        this.cumulativeUsage.totalTokens += this.currentRequestUsage.totalTokens;
      }

      // 累加到totalLifetimeUsage
      if (!this.totalLifetimeUsage) {
        this.totalLifetimeUsage = { ...this.currentRequestUsage };
      } else {
        this.totalLifetimeUsage.promptTokens += this.currentRequestUsage.promptTokens;
        this.totalLifetimeUsage.completionTokens += this.currentRequestUsage.completionTokens;
        this.totalLifetimeUsage.totalTokens += this.currentRequestUsage.totalTokens;
      }

      // 添加到历史记录
      if (this.options.enableHistory) {
        this.addToHistory(this.currentRequestUsage);
      }

      this.currentRequestUsage = null;
    }
  }

  // 添加到历史记录
  private addToHistory(usage: TokenUsageStats): void {
    const historyItem: TokenUsageHistory = {
      requestId: generateId(),
      timestamp: Date.now(),
      promptTokens: usage.promptTokens,
      completionTokens: usage.completionTokens,
      totalTokens: usage.totalTokens,
      cost: usage.rawUsage?.totalCost,
      model: usage.rawUsage?.model,
      rawUsage: usage.rawUsage
    };

    this.usageHistory.push(historyItem);

    // 限制历史记录数量
    if (this.usageHistory.length > this.options.maxHistorySize) {
      this.usageHistory.shift();
    }
  }

  // 回退到指定请求之前
  rollbackToRequest(requestIndex: number): void {
    if (requestIndex < 0 || requestIndex >= this.usageHistory.length) {
      throw new Error('Invalid request index');
    }

    // 重新计算cumulativeUsage
    this.cumulativeUsage = {
      promptTokens: 0,
      completionTokens: 0,
      totalTokens: 0
    };

    for (let i = 0; i < requestIndex; i++) {
      const item = this.usageHistory[i];
      this.cumulativeUsage.promptTokens += item.promptTokens;
      this.cumulativeUsage.completionTokens += item.completionTokens;
      this.cumulativeUsage.totalTokens += item.totalTokens;
    }

    // 删除回退后的历史记录
    this.usageHistory = this.usageHistory.slice(0, requestIndex);
  }

  // 获取历史记录
  getUsageHistory(): TokenUsageHistory[] {
    return [...this.usageHistory];
  }

  // 获取最近N条历史记录
  getRecentHistory(n: number): TokenUsageHistory[] {
    return this.usageHistory.slice(-n);
  }

  // 获取统计信息
  getStatistics(): {
    totalRequests: number;
    averageTokens: number;
    maxTokens: number;
    minTokens: number;
    totalCost: number;
  } {
    if (this.usageHistory.length === 0) {
      return {
        totalRequests: 0,
        averageTokens: 0,
        maxTokens: 0,
        minTokens: 0,
        totalCost: 0
      };
    }

    const totalTokens = this.usageHistory.reduce((sum, h) => sum + h.totalTokens, 0);
    const maxTokens = Math.max(...this.usageHistory.map(h => h.totalTokens));
    const minTokens = Math.min(...this.usageHistory.map(h => h.totalTokens));
    const totalCost = this.usageHistory.reduce((sum, h) => sum + (h.cost || 0), 0);

    return {
      totalRequests: this.usageHistory.length,
      averageTokens: totalTokens / this.usageHistory.length,
      maxTokens,
      minTokens,
      totalCost
    };
  }
}
```

**优点**：
- 完整的历史记录
- 精确的回退能力
- 丰富的统计分析
- 灵活的配置选项

**缺点**：
- 内存占用较大
- 实现复杂度较高

#### 方案2：混合方案（折中）

**设计**：
- 保留当前的cumulativeUsage和totalLifetimeUsage
- 添加可选的历史记录功能
- 默认关闭历史记录，需要时开启

**优点**：
- 向后兼容
- 灵活配置
- 性能可控

**缺点**：
- 功能不完整
- 需要用户主动配置

#### 方案3：仅保留当前实现（不推荐）

**设计**：
- 保持当前的实现不变
- 修复流式处理的问题
- 不添加历史记录功能

**优点**：
- 实现简单
- 性能最优
- 内存占用最小

**缺点**：
- 功能受限
- 无法精确回退
- 无法分析历史

## 四、推荐方案

### 4.1 推荐采用方案1（完全采用数组+总量）

**理由**：

1. **功能完整性**：提供完整的历史记录和回退能力
2. **灵活性**：通过配置选项控制内存占用和性能
3. **可扩展性**：为未来的功能扩展提供基础
4. **用户体验**：提供更好的调试和分析能力

### 4.2 实施建议

#### 阶段1：基础实现
1. 定义TokenUsageHistory接口
2. 修改TokenUsageTracker类，添加usageHistory数组
3. 实现addToHistory方法
4. 实现基本的查询方法

#### 阶段2：回退功能
1. 实现rollbackToRequest方法
2. 实现rollbackToRequestId方法
3. 实现rollbackToTimestamp方法
4. 添加回退验证

#### 阶段3：统计分析
1. 实现getStatistics方法
2. 实现getCostByModel方法
3. 实现getCostByTimeRange方法
4. 实现getPercentile方法

#### 阶段4：优化和持久化
1. 实现历史记录的自动清理
2. 实现历史记录的持久化
3. 实现异步记录选项
4. 性能优化

### 4.3 配置建议

```typescript
const tracker = new TokenUsageTracker({
  tokenLimit: 4000,
  enableHistory: true,        // 启用历史记录
  maxHistorySize: 1000,       // 最多保存1000条记录
  persistHistory: false       // 不持久化（可选）
});
```

### 4.4 向后兼容

为了保持向后兼容，建议：

1. 保留现有的API接口
2. 新增的方法使用可选参数
3. 提供迁移指南
4. 提供单元测试

## 五、总结

### 5.1 当前实现的问题

1. 没有历史记录，无法分析token消耗趋势
2. 无法精确回退到某次API调用之前
3. 流式处理存在统计不准确的问题
4. 无法进行详细的成本分析

### 5.2 推荐方案

采用**数组+总量**的统计方式，具体包括：

1. **usageHistory数组**：记录每次API调用的详细信息
2. **cumulativeUsage**：当前累积总量（可回退）
3. **totalLifetimeUsage**：生命周期总量（不可回退）
4. **配置选项**：控制历史记录的启用、大小、持久化等

### 5.3 预期收益

1. **精确回退**：可以回退到任意一次API调用之前
2. **历史分析**：可以分析token消耗的趋势和模式
3. **成本追踪**：可以追踪每次调用的成本
4. **调试友好**：可以查看详细的调用历史
5. **扩展性**：为未来的功能扩展提供基础

### 5.4 实施优先级

1. **高优先级**：修复流式处理的统计问题
2. **中优先级**：实现基础的历史记录功能
3. **低优先级**：实现高级的统计分析和持久化功能