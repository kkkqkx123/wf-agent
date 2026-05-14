# Universal Metrics System

## 概述

通用指标系统为工作流引擎提供统一的指标收集、聚合和查询框架。它支持多种指标类型（计数器、仪表盘、直方图、摘要），并允许按不同维度（工作流、Agent Loop、节点、工具、模板、配置等）进行分组和过滤。

## 设计目标

- ✅ **统一接口**：所有指标类型遵循相同的采集和查询模式
- ✅ **可扩展性**：支持自定义指标类型和维度
- ✅ **低开销**：异步批量上报，避免影响主流程性能
- ✅ **多维度**：支持按 execution/workflow/agent_loop/node/tool/template/config 等维度聚合
- ✅ **时序支持**：内置时间序列数据管理

## 核心概念

### 1. 指标类型 (Metric Types)

#### Counter (计数器)
单调递增的计数器，用于统计事件发生次数。

```typescript
// 示例：工作流执行次数
collector.incrementCounter("workflow.execution.count", {
  workflow_id: "wf-123",
  status: "completed"
});
```

#### Gauge (仪表盘)
可增可减的数值，用于表示当前状态。

```typescript
// 示例：活跃执行数
collector.setGauge("resource.active.executions", 42);
```

#### Histogram (直方图)
跟踪值分布，用于延迟、响应时间等统计分析。

```typescript
// 示例：节点执行时长分布
collector.observeHistogram("node.execution.duration", 1250, {
  node_type: "LLM",
  success: "true"
});
```

#### Summary (摘要)
提供百分位计算（p95, p99等）。

```typescript
// 示例：P95响应时间
collector.observeSummary("tool.call.duration", 850, {
  tool_id: "search-api"
});
```

### 2. 标签 (Labels)

标签是键值对，用于对指标进行分组和过滤：

```typescript
{
  workflow_id: "wf-123",
  agent_loop_id: "agent-456",
  execution_id: "exec-789",
  node_type: "LLM",
  success: "true"
}
```

### 3. 指标收集器 (Metric Collectors)

系统提供以下专用收集器：

- `WorkflowMetricsCollector` - 工作流执行指标
- `AgentLoopMetricsCollector` - Agent Loop 执行指标
- `NodeMetricsCollector` - 节点执行指标
- `ToolMetricsCollector` - 工具调用指标
- `TokenMetricsCollector` - Token 使用指标
- `TemplateMetricsCollector` - 模板渲染和缓存统计
- `ConfigMetricsCollector` - 配置加载和访问统计
- `ErrorMetricsCollector` - 错误异常指标
- `ResourceMetricsCollector` - 资源利用指标
- `EventMetricsCollector` - 事件统计指标

## 快速开始

### 使用工厂函数创建所有收集器（推荐）

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
collectors.agentLoop.recordExecutionStart("agent-789", "exec-789");
collectors.tool.recordToolCallStart("search-api", "exec-456");
collectors.token.recordTokenUsage({ /* ... */ });
collectors.node.recordNodeStart("node-1", "LLM", "wf-123", "exec-456");
collectors.template.recordUsage("system-prompt", { workflow_id: "wf-123" });
collectors.config.recordAccess("llms.provider.openai", "config");
collectors.error.recordError("LLM_ERROR", "exec-456");
collectors.resource.recordActiveExecutions(5);
```

### 基本使用示例

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

## 各收集器使用指南

### Workflow 指标

监控工作流的执行生命周期和性能：

```typescript
import { WorkflowMetricsCollector } from "@wf-agent/sdk/core/metrics";

const workflowCollector = new WorkflowMetricsCollector();

// 记录执行开始
workflowCollector.recordExecutionStart("wf-123", "exec-456");

// 记录执行完成
workflowCollector.recordExecutionComplete(
  "wf-123",
  "exec-456",
  2500,     // duration in ms
  5,        // node count
  true      // success
);

// 记录节点执行
workflowCollector.recordNodeExecution(
  "wf-123",
  "exec-456",
  "node-1",
  "LLM",
  1200,     // duration
  true      // success
);

// 获取统计信息
const stats = workflowCollector.getWorkflowStats("wf-123");
```

### Agent Loop 指标

监控 Agent Loop 的执行生命周期、迭代和工具调用：

```typescript
import { AgentLoopMetricsCollector } from "@wf-agent/sdk/core/metrics";

const agentLoopCollector = new AgentLoopMetricsCollector();

// 记录执行开始
agentLoopCollector.recordExecutionStart("agent-123", "exec-456");

// 记录执行完成
agentLoopCollector.recordExecutionComplete(
  "agent-123",
  "exec-456",
  5000,     // duration in ms
  10,       // iterations
  25,       // tool calls
  true      // success
);

// 记录迭代
agentLoopCollector.recordIterationStart("agent-123", 1);
agentLoopCollector.recordIterationComplete("agent-123", 1, 500, 3);

// 记录暂停/恢复
agentLoopCollector.recordPause("agent-123");
agentLoopCollector.recordResume("agent-123", 30000); // 30s pause

// 获取统计信息
const stats = agentLoopCollector.getAgentLoopStats("agent-123");
const activeCount = agentLoopCollector.getActiveAgentLoops();
const avgIterations = agentLoopCollector.getAverageIterations();
```

### Node 指标

监控工作流节点的执行情况：

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

### Tool 指标

监控工具调用的性能和成功率：

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

### Token 指标

跟踪 LLM 的 Token 使用和成本：

```typescript
import { TokenMetricsCollector } from "@wf-agent/sdk/core/metrics";

const tokenCollector = new TokenMetricsCollector();

// 记录 Token 使用
tokenCollector.recordTokenUsage({
  profileId: 'gpt-4',
  executionId: 'exec-123',
  nodeId: 'node-1',
  totalTokens: 1500,
  promptTokens: 1000,
  completionTokens: 500,
  cost: 0.045, // USD
});

// 获取 Token 使用摘要
const summary = tokenCollector.getTokenUsageSummary();
console.log('Total tokens:', summary.totalTokens);
console.log('Total cost:', summary.totalCost);

// 获取每个 profile 的平均 Token 使用
const averages = tokenCollector.getAverageTokensPerRequest();
```

### Template 指标

监控模板的渲染和使用情况：

```typescript
import { TemplateMetricsCollector } from "@wf-agent/sdk/core/metrics";

const templateCollector = new TemplateMetricsCollector();

// 记录模板使用
templateCollector.recordUsage("system-prompt", {
  workflow_id: "wf-123",
  agent_loop_id: "agent-456",
});

// 记录渲染完成
templateCollector.recordRenderComplete(
  "system-prompt",
  50,      // render duration in ms
  true,    // success
  { workflow_id: "wf-123" }
);

// 记录缓存命中/未命中
templateCollector.recordCacheHit("system-prompt");
templateCollector.recordCacheMiss("system-prompt");

// 获取统计信息
const stats = templateCollector.getTemplateStats("system-prompt");
const hitRate = templateCollector.getCacheHitRate("system-prompt");
const avgDuration = templateCollector.getAverageRenderDuration("system-prompt");
```

### Config 指标

监控配置的加载和访问情况：

```typescript
import { ConfigMetricsCollector } from "@wf-agent/sdk/core/metrics";

const configCollector = new ConfigMetricsCollector();

// 记录配置访问
configCollector.recordAccess("llms.provider.openai", "config", {
  workflow_id: "wf-123",
});

// 记录加载完成
configCollector.recordLoadComplete(
  "llms.provider.openai",
  100,     // load duration in ms
  true,    // success
  "config",
  { workflow_id: "wf-123" }
);

// 记录缓存命中/未命中
configCollector.recordCacheHit("llms.provider.openai", "config");
configCollector.recordCacheMiss("llms.provider.openai", "config");

// 记录验证错误
configCollector.recordValidationError(
  "workflow.invalid-node",
  "schema_validation_failed",
  "workflow"
);

// 获取统计信息
const stats = configCollector.getConfigStats("llms.provider.openai", "config");
const hitRate = configCollector.getCacheHitRate("llms.provider.openai", "config");
const avgLoadTime = configCollector.getAverageLoadDuration("llms.provider.openai", "config");
```

### Error 指标

跟踪和分析错误模式：

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

### Resource 指标

监控系统资源使用情况：

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

### Event 指标

用于跨执行的事件统计：

```typescript
import { EventMetricsCollector } from "@wf-agent/sdk/core/metrics";

const eventCollector = new EventMetricsCollector({
  bufferSize: 100,
  enablePeriodicReporting: true,
  reportingInterval: 5000,
});

// 记录事件
eventCollector.recordEvent('NODE_COMPLETED', 'exec-123', {
  workflow_id: 'wf-456',
  node_id: 'node-1',
  node_type: 'LLM',
});

// 查询统计
const stats = eventCollector.getStatistics('NODE_COMPLETED');
console.log(`Total NODE_COMPLETED events: ${stats?.count}`);

// 按执行清理
eventCollector.cleanupExecution('exec-123');

// 订阅周期性报告
const unsubscribe = eventCollector.onReport((report) => {
  console.log('Total events:', report.summary.totalMetrics);
});
```

## 预定义指标

### WORKFLOW_METRICS

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `workflow.execution.duration` | Histogram | 工作流执行时长 |
| `workflow.execution.count` | Counter | 工作流执行次数 |
| `workflow.node.count` | Gauge | 节点数量 |
| `workflow.error.count` | Counter | 错误次数 |
| `workflow.success.rate` | Gauge | 成功率 |

### AGENT_LOOP_METRICS

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `agent_loop.execution.duration` | Histogram | Agent Loop 执行时长 |
| `agent_loop.execution.count` | Counter | Agent Loop 执行次数 |
| `agent_loop.active.count` | Gauge | 活跃 Agent Loop 数量 |
| `agent_loop.iteration.count` | Gauge | 迭代次数 |
| `agent_loop.iteration.duration` | Histogram | 迭代时长分布 |
| `agent_loop.iteration.limit_reached` | Counter | 达到最大迭代次数 |
| `agent_loop.tool_calls.total` | Gauge | 总工具调用数 |
| `agent_loop.tool_calls.per_iteration` | Gauge | 每次迭代的工具调用数 |
| `agent_loop.pause.count` | Counter | 暂停次数 |
| `agent_loop.resume.count` | Counter | 恢复次数 |
| `agent_loop.pause.duration` | Histogram | 暂停时长分布 |
| `agent_loop.success.rate` | Gauge | 成功率 |
| `agent_loop.error.count` | Counter | 错误次数 |

### NODE_METRICS

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `node.execution.duration` | Histogram | 节点执行时长 |
| `node.execution.count` | Counter | 节点执行次数 |
| `node.retry.count` | Counter | 重试次数 |
| `node.error.count` | Counter | 错误次数 |
| `node.input.size` | Gauge | 输入大小 |
| `node.output.size` | Gauge | 输出大小 |

### TOOL_METRICS

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `tool.call.duration` | Histogram | 工具调用时长 |
| `tool.call.count` | Counter | 工具调用次数 |
| `tool.error.count` | Counter | 工具错误次数 |
| `tool.parameter.size` | Gauge | 参数大小 |
| `tool.result.size` | Gauge | 结果大小 |

### TOKEN_METRICS

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `token.usage.total` | Counter | 总 Token 数 |
| `token.usage.prompt` | Counter | Prompt Token 数 |
| `token.usage.completion` | Counter | Completion Token 数 |
| `token.cost.total` | Counter | 总成本 |
| `token.request.count` | Counter | 请求次数 |

### TEMPLATE_METRICS

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `template.usage.count` | Counter | 模板使用次数 |
| `template.render.duration` | Histogram | 渲染时长分布 |
| `template.cache.hit_count` | Counter | 缓存命中次数 |
| `template.cache.miss_count` | Counter | 缓存未命中次数 |
| `template.error.count` | Counter | 错误次数 |

### CONFIG_METRICS

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `config.access.count` | Counter | 配置访问次数 |
| `config.load.duration` | Histogram | 加载时长分布 |
| `config.validation_error.count` | Counter | 验证错误次数 |
| `config.cache.hit_count` | Counter | 缓存命中次数 |
| `config.cache.miss_count` | Counter | 缓存未命中次数 |

### ERROR_METRICS

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `error.occurrence.count` | Counter | 错误发生次数 |
| `error.recovery.rate` | Gauge | 恢复率 |
| `error.affected.executions` | Gauge | 受影响的执行数 |

### RESOURCE_METRICS

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

## 最佳实践

### 1. 选择合适的指标类型

- **计数事件** → Counter
- **当前状态** → Gauge
- **分布分析** → Histogram
- **百分位统计** → Summary

### 2. 合理使用标签

✅ 好的做法：
```typescript
{ workflow_id: "wf-123", node_type: "LLM" }
```

❌ 避免高基数标签：
```typescript
{ execution_id: "exec-unique-uuid-every-time" } // 会导致内存爆炸
```

### 3. 控制缓冲区大小

- 高频指标：较小的 `bufferSize`（50-100）
- 低频指标：较大的 `bufferSize`（500-1000）

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

- 指标记录：~0.01ms/次
- 查询聚合：取决于数据量，通常 < 1ms
- 周期性报告：< 5ms

### I/O 开销

- 异步批量刷新，不阻塞主线程
- 建议 `flushInterval` ≥ 5000ms

## 未来扩展

所有核心收集器已实现完成：

- ✅ `WorkflowMetricsCollector` - 工作流执行指标
- ✅ `AgentLoopMetricsCollector` - Agent Loop 执行生命周期
- ✅ `NodeMetricsCollector` - 节点级详细指标
- ✅ `ToolMetricsCollector` - 工具调用性能分析
- ✅ `TokenMetricsCollector` - LLM Token 使用和成本
- ✅ `TemplateMetricsCollector` - 模板渲染和缓存统计
- ✅ `ConfigMetricsCollector` - 配置加载和访问统计
- ✅ `ErrorMetricsCollector` - 错误模式和恢复率
- ✅ `ResourceMetricsCollector` - 系统资源监控
- ✅ `EventMetricsCollector` - 事件统计指标

下一步可以：
- 在模板引擎和配置加载器中集成指标收集
- 实现持久化层（数据库、监控系统如 Prometheus/Grafana）
- 添加更多高级查询和聚合功能
- 基于指标设置告警规则

## 示例代码

查看完整示例：
- [examples.ts](./examples.ts) - 通用指标使用示例集合
- [__examples__/event-collector-example.ts](__examples__/event-collector-example.ts) - EventMetricsCollector 使用示例

运行示例：
```bash
cd sdk
npx tsx core/metrics/examples.ts
npx tsx core/metrics/__examples__/event-collector-example.ts
```
