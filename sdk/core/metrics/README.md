# Universal Metrics System

## 概述

通用指标系统为工作流引擎提供统一的指标收集、聚合和查询框架。它支持多种指标类型(计数器、仪表盘、直方图、摘要),并允许按不同维度(工作流、节点、工具等)进行分组和过滤。

## 设计目标

- ✅ **统一接口**:所有指标类型遵循相同的采集和查询模式
- ✅ **可扩展性**:支持自定义指标类型和维度
- ✅ **低开销**:异步批量上报,避免影响主流程性能
- ✅ **多维度**:支持按 execution/workflow/node/tool 等维度聚合
- ✅ **时序支持**:内置时间序列数据管理

## 核心概念

### 1. 指标类型 (Metric Types)

#### Counter (计数器)
单调递增的计数器,用于统计事件发生次数。

```typescript
// 示例:工作流执行次数
collector.incrementCounter("workflow.execution.count", {
  workflow_id: "wf-123",
  status: "completed"
});
```

#### Gauge (仪表盘)
可增可减的数值,用于表示当前状态。

```typescript
// 示例:活跃执行数
collector.setGauge("resource.active.executions", 42);
```

#### Histogram (直方图)
跟踪值分布,用于延迟、响应时间等统计分析。

```typescript
// 示例:节点执行时长分布
collector.observeHistogram("node.execution.duration", 1250, {
  node_type: "LLM",
  success: "true"
});
```

#### Summary (摘要)
提供百分位计算(p95, p99等)。

```typescript
// 示例:P95响应时间
collector.observeSummary("tool.call.duration", 850, {
  tool_id: "search-api"
});
```

### 2. 标签 (Labels)

标签是键值对,用于对指标进行分组和过滤:

```typescript
{
  workflow_id: "wf-123",
  execution_id: "exec-456",
  node_type: "LLM",
  success: "true"
}
```

### 3. 指标收集器 (Metric Collector)

每个组件应有自己的收集器实现:

- `WorkflowMetricsCollector` - 工作流执行指标
- `EventMetricsCollector` - 事件统计指标（替代旧的 MetricsAggregator）
- `NodeMetricsCollector` - 节点执行指标 ✅ 已实现
- `ToolMetricsCollector` - 工具调用指标 ✅ 已实现
- `TokenMetricsCollector` - Token使用指标 ✅ 已实现
- `ErrorMetricsCollector` - 错误异常指标 ✅ 已实现
- `ResourceMetricsCollector` - 资源利用指标 ✅ 已实现

## 快速开始

### 安装

指标系统已集成在 SDK 中,无需额外安装。

### 使用工厂函数创建所有收集器 (推荐)

```typescript
import { createMetricsCollectors } from "@wf-agent/sdk/core/metrics";

// 一次性创建所有标准收集器
const collectors = createMetricsCollectors({
  bufferSize: 100,
  flushInterval: 5000,
  enablePeriodicReporting: true,
  reportingInterval: 10000,
});

// 访问各个收集器
collectors.workflow.recordExecutionStart("wf-123", "exec-456");
collectors.tool.recordToolCallStart("search-api", "exec-456");
collectors.token.recordTokenUsage({ /* ... */ });
collectors.node.recordNodeStart("node-1", "LLM", "wf-123", "exec-456");
collectors.error.recordError("LLM_ERROR", "exec-456");
collectors.resource.recordActiveExecutions(5);
```

### 基本使用

```typescript
import { WorkflowMetricsCollector } from "@wf-agent/sdk/core/metrics";

// 1. 创建收集器
const collector = new WorkflowMetricsCollector({
  bufferSize: 100,        // 缓冲区大小
  flushInterval: 5000,    // 自动刷新间隔(ms)
});

// 2. 记录指标
collector.recordExecutionStart("wf-123", "exec-456");

collector.recordNodeExecution(
  "wf-123",
  "exec-456",
  "node-1",
  "LLM",
  1200,  // duration in ms
  true   // success
);

collector.recordExecutionComplete(
  "wf-123",
  "exec-456",
  2500,  // total duration
  5,     // node count
  true   // success
);

// 3. 查询指标
const stats = collector.getWorkflowStats("wf-123");
console.log("Total metrics:", stats.totalCount);

// 4. 清理
collector.dispose();
```

### 事件统计 (EventMetricsCollector)

用于跨执行的事件统计，替代旧的 `MetricsAggregator`：

```typescript
import { EventMetricsCollector } from "@wf-agent/sdk/core/metrics";

// 1. 创建事件收集器
const eventCollector = new EventMetricsCollector({
  bufferSize: 100,
  enablePeriodicReporting: true,
  reportingInterval: 5000,
});

// 2. 记录事件
eventCollector.recordEvent('NODE_COMPLETED', 'exec-123', {
  workflow_id: 'wf-456',
  node_id: 'node-1',
  node_type: 'LLM',
});

eventCollector.recordEvent('TOOL_EXECUTED', 'exec-123', {
  workflow_id: 'wf-456',
  tool_name: 'search-api',
});

// 3. 查询统计
const stats = eventCollector.getStatistics('NODE_COMPLETED');
console.log(`Total NODE_COMPLETED events: ${stats?.count}`);

// 4. 按执行清理
eventCollector.cleanupExecution('exec-123');

// 5. 订阅周期性报告
const unsubscribe = eventCollector.onReport((report) => {
  console.log('Total events:', report.summary.totalMetrics);
});

// 6. 清理
unsubscribe();
eventCollector.dispose();
```

### 工具调用指标 (ToolMetricsCollector)

用于监控工具调用的性能和成功率：

```typescript
import { ToolMetricsCollector } from "@wf-agent/sdk/core/metrics";

const toolCollector = new ToolMetricsCollector();

// 记录工具调用开始
toolCollector.recordToolCallStart('search-api', 'exec-123');

// 记录工具调用完成
toolCollector.recordToolCallComplete(
  'search-api',
  'exec-123',
  850,  // duration in ms
  true, // success
  256,  // parameter size in bytes
  1024  // result size in bytes
);

// 查询工具性能
const stats = toolCollector.getToolStats('search-api');
const summary = toolCollector.getToolPerformanceSummary();
```

### Token使用指标 (TokenMetricsCollector)

用于跟踪LLM的Token使用和成本：

```typescript
import { TokenMetricsCollector } from "@wf-agent/sdk/core/metrics";

const tokenCollector = new TokenMetricsCollector();

// 记录Token使用
tokenCollector.recordTokenUsage({
  profileId: 'gpt-4',
  executionId: 'exec-123',
  nodeId: 'node-1',
  totalTokens: 1500,
  promptTokens: 1000,
  completionTokens: 500,
  cost: 0.045, // USD
});

// 获取Token使用摘要
const summary = tokenCollector.getTokenUsageSummary();
console.log('Total tokens:', summary.totalTokens);
console.log('Total cost:', summary.totalCost);

// 获取每个profile的平均Token使用
const averages = tokenCollector.getAverageTokensPerRequest();
```

### 节点执行指标 (NodeMetricsCollector)

用于监控工作流节点的执行情况：

```typescript
import { NodeMetricsCollector } from "@wf-agent/sdk/core/metrics";

const nodeCollector = new NodeMetricsCollector();

// 记录节点开始
nodeCollector.recordNodeStart('node-1', 'LLM', 'wf-123', 'exec-456');

// 记录节点完成
nodeCollector.recordNodeComplete(
  'node-1',
  'LLM',
  'wf-123',
  'exec-456',
  1200,  // duration in ms
  true,  // success
  512,   // input size
  1024   // output size
);

// 记录节点重试
nodeCollector.recordNodeRetry('node-1', 'LLM', 'wf-123', 1);

// 获取节点性能摘要
const performance = nodeCollector.getNodePerformanceByType();
```

### 错误指标 (ErrorMetricsCollector)

用于跟踪和分析错误模式：

```typescript
import { ErrorMetricsCollector } from "@wf-agent/sdk/core/metrics";

const errorCollector = new ErrorMetricsCollector();

// 记录错误
errorCollector.recordError(
  'LLM_ERROR',
  'exec-123',
  'node-1',
  'Rate limit exceeded'
);

// 记录错误恢复
errorCollector.recordErrorRecovery('LLM_ERROR', 'exec-123');

// 获取错误摘要
const summary = errorCollector.getErrorSummary();
console.log('Total errors:', summary.totalErrors);
console.log('Top errors:', summary.topErrors);
```

### 资源指标 (ResourceMetricsCollector)

用于监控系统资源使用情况：

```typescript
import { ResourceMetricsCollector } from "@wf-agent/sdk/core/metrics";

const resourceCollector = new ResourceMetricsCollector();

// 记录资源快照
resourceCollector.recordResourceSnapshot({
  memoryUsageMB: 256.5,
  activeExecutions: 5,
  queuedTasks: 12,
  eventQueueLength: 3,
});

// 或者单独记录
resourceCollector.recordMemoryUsage(256.5, 'executor');
resourceCollector.recordActiveExecutions(5);
resourceCollector.recordQueuedTasks(12, 'workflow');
resourceCollector.recordEventQueueLength(3);

// 获取资源摘要
const summary = resourceCollector.getResourceSummary();
```

### 周期性报告

```typescript
const collector = new WorkflowMetricsCollector({
  enablePeriodicReporting: true,
  reportingInterval: 10000, // 每10秒生成报告
});

// 订阅报告
const unsubscribe = collector.onReport((report) => {
  console.log("📊 Report:", report.summary);
  console.log("Top metrics:", report.topMetrics);
});

// ... 执行业务逻辑 ...

// 取消订阅
unsubscribe();
```

## 预定义指标

### 工作流指标 (WORKFLOW_METRICS)

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `workflow.execution.duration` | Histogram | 工作流执行时长 |
| `workflow.execution.count` | Counter | 工作流执行次数 |
| `workflow.node.count` | Gauge | 节点数量 |
| `workflow.error.count` | Counter | 错误次数 |
| `workflow.success.rate` | Gauge | 成功率 |

### 节点指标 (NODE_METRICS)

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `node.execution.duration` | Histogram | 节点执行时长 |
| `node.execution.count` | Counter | 节点执行次数 |
| `node.retry.count` | Counter | 重试次数 |
| `node.error.count` | Counter | 错误次数 |
| `node.input.size` | Gauge | 输入大小 |
| `node.output.size` | Gauge | 输出大小 |

### 工具指标 (TOOL_METRICS)

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `tool.call.duration` | Histogram | 工具调用时长 |
| `tool.call.count` | Counter | 工具调用次数 |
| `tool.error.count` | Counter | 工具错误次数 |
| `tool.parameter.size` | Gauge | 参数大小 |
| `tool.result.size` | Gauge | 结果大小 |

### Token指标 (TOKEN_METRICS)

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `token.usage.total` | Counter | 总Token数 |
| `token.usage.prompt` | Counter | Prompt Token数 |
| `token.usage.completion` | Counter | Completion Token数 |
| `token.cost.total` | Counter | 总成本 |
| `token.request.count` | Counter | 请求次数 |

### 错误指标 (ERROR_METRICS)

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `error.occurrence.count` | Counter | 错误发生次数 |
| `error.recovery.rate` | Gauge | 恢复率 |
| `error.affected.executions` | Gauge | 受影响的执行数 |

### 资源指标 (RESOURCE_METRICS)

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `resource.memory.usage` | Gauge | 内存使用 |
| `resource.active.executions` | Gauge | 活跃执行数 |
| `resource.queued.tasks` | Gauge | 排队任务数 |
| `resource.event.queue.length` | Gauge | 事件队列长度 |

## 高级用法

### 自定义指标收集器

```typescript
import { BaseMetricCollector } from "@wf-agent/sdk/core/metrics";

class CustomMetricsCollector extends BaseMetricCollector {
  // 实现自定义方法
  recordCustomMetric(value: number, labels?: Record<string, string>) {
    this.observeHistogram("custom.metric", value, labels);
  }

  // 实现持久化逻辑
  async flush(): Promise<void> {
    const metrics = this.metricsBuffer;
    
    // TODO: 写入数据库或发送到监控服务
    await sendToMonitoringService(metrics);
    
    this.metricsBuffer = [];
  }
}
```

### 集成到现有代码

```typescript
class WorkflowExecutorWithMetrics {
  private metrics: WorkflowMetricsCollector;

  constructor() {
    this.metrics = new WorkflowMetricsCollector();
  }

  async execute(workflowId: string, executionId: string) {
    const startTime = Date.now();

    try {
      this.metrics.recordExecutionStart(workflowId, executionId);
      
      // ... 执行工作流 ...
      
      const duration = Date.now() - startTime;
      this.metrics.recordExecutionComplete(
        workflowId, executionId, duration, nodeCount, true
      );
    } catch (error) {
      const duration = Date.now() - startTime;
      this.metrics.recordExecutionComplete(
        workflowId, executionId, duration, 0, false
      );
      this.metrics.recordError(workflowId, executionId, error.name);
      throw error;
    }
  }
}
```

### 查询和过滤

```typescript
// 查询特定工作流的指标
const result = collector.query({
  labels: { workflow_id: "wf-123" },
  timeRange: {
    from: Date.now() - 3600000, // 最近1小时
    to: Date.now()
  },
  limit: 100
});

console.log("Total metrics:", result.totalCount);
console.log("Aggregated data:", result.metrics);
```

## 与旧 MetricsAggregator 的关系

### 迁移完成

**旧的 `MetricsAggregator` 已被完全替换为 `EventMetricsCollector`**

- ✅ `MetricsAggregator` 已删除
- ✅ `EventMetricsCollector` 提供相同功能并扩展了新能力
- ✅ `EventRegistry` 已更新使用新的收集器
- ✅ API 保持向后兼容（`getStatistics()`, `generateSummary()` 等方法）

### 主要改进

| 特性 | 旧 MetricsAggregator | 新 EventMetricsCollector |
|-----|---------------------|-------------------------|
| 基础架构 | 独立实现 | 基于 Universal Metrics 系统 |
| 指标类型 | 仅计数器 | Counter + 支持其他类型扩展 |
| 维度标签 | executionId + eventType | 任意标签组合 (workflow_id, node_type, tool_name等) |
| 查询能力 | 基础统计 | 高级过滤和聚合 |
| 周期性报告 | onSummary | onReport (统一接口) |
| 可扩展性 | 有限 | 高度可扩展，可与其他收集器集成 |

### 迁移指南

如果你之前使用了 `MetricsAggregator`，现在应该使用 `EventMetricsCollector`：

**旧代码:**
```typescript
import { MetricsAggregator } from "@wf-agent/sdk/core/registry";

const aggregator = new MetricsAggregator();
aggregator.record({
  executionId: 'exec-1',
  eventType: 'NODE_COMPLETED',
  timestamp: Date.now(),
});

const stats = aggregator.getStatistics('NODE_COMPLETED');
```

**新代码:**
```typescript
import { EventMetricsCollector } from "@wf-agent/sdk/core/metrics";

const collector = new EventMetricsCollector();
collector.recordEvent('NODE_COMPLETED', 'exec-1', {
  workflow_id: 'wf-123',
  node_type: 'LLM',
});

const stats = collector.getStatistics('NODE_COMPLETED');
```

**从 EventRegistry 获取:**
```typescript
// 旧方式 (已移除)
// const aggregator = eventRegistry.getMetricsAggregator();

// 新方式
const collector = eventRegistry.getMetricsCollector();
const stats = collector.getStatistics('NODE_COMPLETED');
```

## 最佳实践

### 1. 选择合适的指标类型

- **计数事件** → Counter
- **当前状态** → Gauge
- **分布分析** → Histogram
- **百分位统计** → Summary

### 2. 合理使用标签

✅ 好的做法:
```typescript
{ workflow_id: "wf-123", node_type: "LLM" }
```

❌ 避免高基数标签:
```typescript
{ execution_id: "exec-unique-uuid-every-time" } // 会导致内存爆炸
```

### 3. 控制缓冲区大小

- 高频指标:较小的 `bufferSize`(50-100)
- 低频指标:较大的 `bufferSize`(500-1000)

### 4. 定期清理

```typescript
// 应用关闭时
collector.dispose();

// 或定期清理过期数据
setInterval(() => collector.clear(), 3600000);
```

### 5. 错误处理

```typescript
try {
  collector.record(metric);
} catch (error) {
  // 指标记录失败不应影响主业务逻辑
  console.warn("Failed to record metric:", error);
}
```

## 性能考虑

### 内存使用

- 每个指标约占用 200-500 字节
- 默认缓冲区 100 个指标 ≈ 20-50 KB
- 建议根据实际负载调整 `bufferSize`

### CPU 开销

- 指标记录:~0.01ms/次
- 查询聚合:取决于数据量,通常 < 1ms
- 周期性报告:< 5ms

### I/O 开销

- 异步批量刷新,不阻塞主线程
- 建议 `flushInterval` ≥ 5000ms

## 未来扩展

所有核心收集器已实现完成:

- ✅ `NodeMetricsCollector` - 节点级详细指标
- ✅ `ToolMetricsCollector` - 工具调用性能分析
- ✅ `TokenMetricsCollector` - LLM Token使用和成本
- ✅ `ErrorMetricsCollector` - 错误模式和恢复率
- ✅ `ResourceMetricsCollector` - 系统资源监控

下一步可以:
- 集成到更多执行器和协调器中
- 实现持久化层(数据库、监控系统)
- 添加更多高级查询和聚合功能

## 示例代码

查看完整示例:
- [examples.ts](./examples.ts) - 通用指标使用示例集合
- [__examples__/event-collector-example.ts](__examples__/event-collector-example.ts) - EventMetricsCollector 使用示例

运行示例:
```bash
cd sdk
npx tsx core/metrics/examples.ts
npx tsx core/metrics/__examples__/event-collector-example.ts
```
