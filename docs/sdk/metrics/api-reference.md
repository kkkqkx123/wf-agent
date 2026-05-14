# Metrics Collectors API 参考

## 快速索引

- [WorkflowMetricsCollector](#workflowmetricscollector)
- [NodeMetricsCollector](#nodemetricscollector)
- [AgentMetricsCollector](#agentmetricscollector)
- [UnifiedMetricsManager](#unifiedmetricsmanager)

---

## WorkflowMetricsCollector

**文件**: `sdk/core/metrics/workflow-metrics-collector.ts`

### 构造函数

```typescript
constructor(config?: MetricCollectorConfig)
```

### 方法

#### recordExecutionStart

记录工作流执行开始。

```typescript
recordExecutionStart(
  workflowId: string,
  executionId: string,
  labels?: {
    version?: string;
    executionType?: 'MAIN' | 'FORK_JOIN' | 'TRIGGERED_SUBWORKFLOW';
  }
): void
```

**参数**:
- `workflowId`: 工作流 ID
- `executionId`: 执行 ID
- `labels.version`: 工作流版本(可选)
- `labels.executionType`: 执行类型(可选,默认 'MAIN')

**示例**:
```typescript
collector.recordExecutionStart('code-review', 'exec-123', {
  version: '2.0',
  executionType: 'MAIN'
});
```

#### recordExecutionComplete

记录工作流执行完成。

```typescript
recordExecutionComplete(
  workflowId: string,
  executionId: string,
  result: {
    success: boolean;
    duration: number;
    nodeCount: number;
    errorType?: string;
  }
): void
```

**参数**:
- `workflowId`: 工作流 ID
- `executionId`: 执行 ID
- `result.success`: 是否成功
- `result.duration`: 执行时长(毫秒)
- `result.nodeCount`: 节点数量
- `result.errorType`: 错误类型(失败时)

**示例**:
```typescript
collector.recordExecutionComplete('code-review', 'exec-123', {
  success: true,
  duration: 3200,
  nodeCount: 5
});
```

#### getWorkflowUsageStats

获取工作流使用统计。

```typescript
getWorkflowUsageStats(workflowId?: string): {
  totalExecutions: number;
  successRate: number;
  avgDuration: number;
  p95Duration: number;
  p99Duration: number;
  byVersion: Record<string, number>;
}
```

**参数**:
- `workflowId`: 工作流 ID(可选,不提供则统计所有)

**返回**:
- `totalExecutions`: 总执行次数
- `successRate`: 成功率(0-1)
- `avgDuration`: 平均时长(毫秒)
- `p95Duration`: P95 时长(毫秒)
- `p99Duration`: P99 时长(毫秒)
- `byVersion`: 按版本分组的执行次数

**示例**:
```typescript
const stats = collector.getWorkflowUsageStats('code-review');
console.log(`Success rate: ${(stats.successRate * 100).toFixed(1)}%`);
console.log(`P95 duration: ${stats.p95Duration}ms`);
```

#### getTopWorkflows

获取最常用的工作流排名。

```typescript
getTopWorkflows(limit?: number): Array<{
  workflowId: string;
  executionCount: number;
  successRate: number;
}>
```

**参数**:
- `limit`: 返回数量(默认 10)

**返回**: 按执行次数排序的工作流列表

**示例**:
```typescript
const topWorkflows = collector.getTopWorkflows(5);
topWorkflows.forEach((wf, i) => {
  console.log(`${i + 1}. ${wf.workflowId}: ${wf.executionCount} executions`);
});
```

---

## NodeMetricsCollector

**文件**: `sdk/core/metrics/node-metrics-collector.ts`

### 构造函数

```typescript
constructor(config?: MetricCollectorConfig)
```

### 方法

#### recordTemplateInstantiation

记录节点模板实例化。

```typescript
recordTemplateInstantiation(
  templateName: string,
  nodeType: string,
  metadata?: {
    category?: string;
    tags?: string[];
  }
): void
```

**参数**:
- `templateName`: 模板名称
- `nodeType`: 节点类型(START, END, LLM, SCRIPT, etc.)
- `metadata.category`: 分类(可选)
- `metadata.tags`: 标签数组(可选)

**示例**:
```typescript
collector.recordTemplateInstantiation('code-analyzer', 'LLM', {
  category: 'analysis',
  tags: ['code', 'review']
});
```

#### recordNodeExecutionStart

记录节点执行开始。

```typescript
recordNodeExecutionStart(
  nodeId: string,
  nodeType: string,
  workflowId: string
): void
```

**示例**:
```typescript
collector.recordNodeExecutionStart('node-1', 'LLM', 'workflow-123');
```

#### recordNodeExecution

记录节点执行完成。

```typescript
recordNodeExecution(
  nodeId: string,
  nodeType: string,
  workflowId: string,
  result: {
    success: boolean;
    duration: number;
    tokenUsage?: number;
    errorType?: string;
  }
): void
```

**参数**:
- `nodeId`: 节点 ID
- `nodeType`: 节点类型
- `workflowId`: 工作流 ID
- `result.success`: 是否成功
- `result.duration`: 执行时长(毫秒)
- `result.tokenUsage`: Token 使用量(LLM 节点)
- `result.errorType`: 错误类型(失败时)

**示例**:
```typescript
collector.recordNodeExecution('node-1', 'LLM', 'wf-123', {
  success: true,
  duration: 2500,
  tokenUsage: 1500
});
```

#### getTopNodeTemplates

获取最常用的节点模板排名。

```typescript
getTopNodeTemplates(limit?: number): Array<{
  templateName: string;
  nodeType: string;
  instantiationCount: number;
}>
```

**示例**:
```typescript
const topTemplates = collector.getTopNodeTemplates(10);
topTemplates.forEach(nt => {
  console.log(`${nt.templateName} (${nt.nodeType}): ${nt.instantiationCount}`);
});
```

#### getNodeExecutionStatsByType

按节点类型统计执行情况。

```typescript
getNodeExecutionStatsByType(): Record<string, {
  totalCount: number;
  successRate: number;
  avgDuration: number;
}>
```

**返回**: 按节点类型分组的统计数据

**示例**:
```typescript
const stats = collector.getNodeExecutionStatsByType();
Object.entries(stats).forEach(([type, data]) => {
  console.log(`${type}: ${data.totalCount} executions, ${(data.successRate * 100).toFixed(1)}% success`);
});
```

---

## AgentMetricsCollector

**文件**: `sdk/core/metrics/agent-metrics-collector.ts`

### 构造函数

```typescript
constructor(config?: MetricCollectorConfig)
```

### 方法

#### recordExecutionStart

记录 Agent Loop 执行开始。

```typescript
recordExecutionStart(
  profileId: string,
  agentConfigId: string,
  executionId: string
): void
```

**参数**:
- `profileId`: Profile ID
- `agentConfigId`: Agent 配置 ID
- `executionId`: 执行 ID

**示例**:
```typescript
collector.recordExecutionStart('senior-dev', 'config-1', 'agent-exec-123');
```

#### recordExecutionComplete

记录 Agent Loop 执行完成。

```typescript
recordExecutionComplete(
  profileId: string,
  result: {
    iterations: number;
    toolCallCount: number;
    duration: number;
    tokenUsage?: number;
    success: boolean;
  }
): void
```

**参数**:
- `profileId`: Profile ID
- `result.iterations`: 迭代次数
- `result.toolCallCount`: Tool 调用次数
- `result.duration`: 执行时长(毫秒)
- `result.tokenUsage`: Token 使用总量
- `result.success`: 是否成功

**示例**:
```typescript
collector.recordExecutionComplete('senior-dev', {
  iterations: 5,
  toolCallCount: 12,
  duration: 15000,
  tokenUsage: 8000,
  success: true
});
```

#### recordIteration

记录迭代。

```typescript
recordIteration(
  profileId: string,
  iteration: number
): void
```

**示例**:
```typescript
collector.recordIteration('profile-1', 3);
```

#### recordToolCall

记录 Tool 调用。

```typescript
recordToolCall(
  toolName: string,
  profileId: string,
  result: {
    success: boolean;
    duration: number;
  }
): void
```

**示例**:
```typescript
collector.recordToolCall('search_code', 'profile-1', {
  success: true,
  duration: 500
});
```

#### getAgentStats

获取 Agent 统计信息。

```typescript
getAgentStats(profileId?: string): {
  totalExecutions: number;
  avgIterations: number;
  avgToolCalls: number;
  byProfile: Record<string, number>;
}
```

**参数**:
- `profileId`: Profile ID(可选,不提供则统计所有)

**返回**:
- `totalExecutions`: 总执行次数
- `avgIterations`: 平均迭代次数
- `avgToolCalls`: 平均 Tool 调用次数(待实现)
- `byProfile`: 按 Profile 分组的执行次数

**示例**:
```typescript
const stats = collector.getAgentStats();
console.log(`Total executions: ${stats.totalExecutions}`);
console.log(`Average iterations: ${stats.avgIterations.toFixed(1)}`);

Object.entries(stats.byProfile).forEach(([profile, count]) => {
  console.log(`  ${profile}: ${count} executions`);
});
```

---

## UnifiedMetricsManager

**文件**: `sdk/core/metrics/unified-metrics-manager.ts`

### 构造函数

```typescript
constructor(
  globalContext: GlobalContext,
  config?: MetricsManagerConfig
)
```

**MetricsManagerConfig**:
```typescript
interface MetricsManagerConfig {
  workflowMetrics?: MetricCollectorConfig;
  nodeMetrics?: MetricCollectorConfig;
  agentMetrics?: MetricCollectorConfig;
  enablePeriodicReporting?: boolean;
  reportingInterval?: number; // milliseconds
}
```

### 方法

#### getCollectors

获取所有 collectors。

```typescript
getCollectors(): {
  workflow: WorkflowMetricsCollector;
  node: NodeMetricsCollector;
  agent: AgentMetricsCollector;
  event: EventMetricsCollector | null;
}
```

**示例**:
```typescript
const collectors = manager.getCollectors();
collectors.workflow.recordExecutionStart(...);
collectors.node.recordTemplateInstantiation(...);
collectors.agent.recordExecutionStart(...);
```

#### generateReport

生成综合报告。

```typescript
async generateReport(options?: {
  timeRange?: { from: number; to: number };
  includeTrends?: boolean;
}): Promise<MetricReport>
```

**示例**:
```typescript
const report = await manager.generateReport();
console.log(`Total workflow executions: ${report.summary.byCategory.workflow}`);
console.log(`Top metrics:`, report.topMetrics);
```

#### onReport

订阅定期报告。

```typescript
onReport(
  callback: (report: MetricReport) => void | Promise<void>
): () => void  // unsubscribe function
```

**示例**:
```typescript
const unsubscribe = manager.onReport((report) => {
  console.log('Periodic report:', report);
  // Send to monitoring system
});

// Later, unsubscribe
unsubscribe();
```

#### flushAll

刷新所有 metrics。

```typescript
async flushAll(): Promise<void>
```

**示例**:
```typescript
await manager.flushAll();
```

#### dispose

释放资源。

```typescript
dispose(): void
```

**示例**:
```typescript
manager.dispose();
```

---

## 通用类型

### MetricCollectorConfig

```typescript
interface MetricCollectorConfig {
  bufferSize?: number;        // Default: 100
  flushInterval?: number;     // Default: 5000 (ms)
  enablePeriodicReporting?: boolean;  // Default: false
  reportingInterval?: number; // Default: 10000 (ms)
  maxAge?: number;            // Default: 3600000 (1 hour)
}
```

### MetricReport

```typescript
interface MetricReport {
  timestamp: number;
  summary: {
    totalMetrics: number;
    byType: {
      counter: number;
      gauge: number;
      histogram: number;
      summary: number;
    };
    byCategory: Record<string, number>;
  };
  topMetrics: Array<{
    metricName: string;
    value: number;
    labels: Record<string, string>;
  }>;
  anomalies?: Array<{
    metricName: string;
    description: string;
    severity: 'low' | 'medium' | 'high';
  }>;
}
```

---

## 完整示例

### 初始化和使用

```typescript
import { UnifiedMetricsManager } from "@wf-agent/sdk";

// 创建 Manager
const manager = new UnifiedMetricsManager(globalContext, {
  enablePeriodicReporting: true,
  reportingInterval: 60000  // 每分钟报告
});

// 获取 collectors
const { workflow, node, agent } = manager.getCollectors();

// 记录 Workflow 指标
workflow.recordExecutionStart('code-review', 'exec-1', {
  version: '2.0',
  executionType: 'MAIN'
});

// ... 执行工作流 ...

workflow.recordExecutionComplete('code-review', 'exec-1', {
  success: true,
  duration: 3200,
  nodeCount: 5
});

// 记录 Node 指标
node.recordTemplateInstantiation('code-analyzer', 'LLM', {
  category: 'analysis'
});

node.recordNodeExecution('node-1', 'LLM', 'wf-1', {
  success: true,
  duration: 2500,
  tokenUsage: 1500
});

// 记录 Agent 指标
agent.recordExecutionStart('senior-dev', 'config-1', 'agent-1');

// ... 执行 Agent Loop ...

agent.recordExecutionComplete('senior-dev', {
  iterations: 5,
  toolCallCount: 12,
  duration: 15000,
  tokenUsage: 8000,
  success: true
});

// 查询统计
const wfStats = workflow.getWorkflowUsageStats('code-review');
console.log(`Workflow success rate: ${(wfStats.successRate * 100).toFixed(1)}%`);

const topNodes = node.getTopNodeTemplates(5);
console.log('Top node templates:', topNodes);

const agentStats = agent.getAgentStats();
console.log(`Agent avg iterations: ${agentStats.avgIterations.toFixed(1)}`);

// 生成报告
const report = await manager.generateReport();

// 清理
manager.dispose();
```

---

## 注意事项

1. **异步操作**: `flush()` 和 `generateReport()` 是异步的,需要 await
2. **内存管理**: 定期调用 `flush()` 或配置自动刷新
3. **错误处理**: 所有方法都有内部错误处理,不会抛出异常
4. **线程安全**: Collector 不是线程安全的,避免并发访问
5. **持久化**: 当前版本只在内存中,重启后数据丢失

---

**最后更新**: 2026-05-14  
**版本**: Phase 1 Implementation
