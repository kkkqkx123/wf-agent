# Metrics 系统设计方案

## 概述

本文档描述如何为 Workflow、Node 模板和 Agent 静态定义补充完整的 Metrics 实现,用于收集调用次数、执行时长等关键指标,帮助识别高频使用的工作流和节点。

## 现有基础

### 已有的 Metrics 基础设施

1. **Universal Metrics System** (`sdk/core/metrics/`)
   - `BaseMetricCollector`: 基础指标收集器
   - `EventMetricsCollector`: 事件指标收集器
   - 支持 Counter、Gauge、Histogram、Summary 四种指标类型
   - 支持基于 Label 的多维度过滤和聚合

2. **现有的统计能力**
   - `WorkflowExecutionStats`: 工作流执行统计(按状态、类型、工作流分组)
   - `TaskStats`: 任务统计(成功率、超时率、执行时长)
   - `AgentLoopStatistics`: Agent Loop 统计信息

3. **事件系统**
   - 完整的工作流生命周期事件: `WORKFLOW_EXECUTION_STARTED`, `COMPLETED`, `FAILED`, `PAUSED`, `RESUMED`
   - Node 执行事件: `NODE_STARTED`, `NODE_COMPLETED`, `NODE_FAILED`
   - Agent 事件: `AGENT_STARTED`, `AGENT_COMPLETED`, `AGENT_ITERATION_STARTED/COMPLETED`

### 当前不足

1. **缺少静态定义的调用追踪**
   - 无法直接查询某个 Workflow 定义被调用了多少次
   - 无法统计某个 Node Template 的使用频率
   - 无法了解 Agent 配置的调用情况

2. **指标分散在不同地方**
   - 部分统计在 Storage 层(`WorkflowExecutionStorage`)
   - 部分在 Registry API 层(`WorkflowExecutionRegistryAPI`)
   - 缺乏统一的跨执行聚合视图

3. **缺少持久化和长期趋势分析**
   - 当前 metrics 主要在内存中
   - 缺少历史数据的持久化存储
   - 无法进行时间序列分析

## 设计方案

### 设计原则

1. **分层设计**: Static Definition → Runtime Execution → Metrics Collection → Aggregation & Storage
2. **非侵入式**: 通过事件驱动,不修改核心执行逻辑
3. **可扩展**: 支持自定义维度和指标类型
4. **可观测性优先**: 提供丰富的查询接口和导出能力

### 架构层次

```
┌─────────────────────────────────────────────────────────┐
│                  Metrics Query & Export                  │
│  (API Layer: REST/GraphQL, Dashboard, Alerting)         │
└──────────────────────┬──────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────┐
│              Metrics Aggregation & Storage               │
│  (Time-series DB, Periodic Reports, Trend Analysis)     │
└──────────────────────┬──────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────┐
│            Universal Metrics Collectors                  │
│  (WorkflowMetrics, NodeMetrics, AgentMetrics, etc.)     │
└──────────────────────┬──────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────┐
│                 Event-Driven Collection                  │
│  (Listen to lifecycle events, record metrics)           │
└──────────────────────┬──────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────┐
│              Core Execution Engine                       │
│  (WorkflowExecutor, NodeCoordinator, AgentLoopExecutor) │
└─────────────────────────────────────────────────────────┘
```

## 核心组件设计

### 1. Workflow Metrics Collector

**位置**: `sdk/core/metrics/workflow-metrics-collector.ts`

**职责**:
- 追踪 Workflow 定义的调用次数
- 记录执行时长分布(P50, P95, P99)
- 统计成功率和失败原因
- 按版本、执行类型(Main/Fork/Triggered)分组

**关键指标**:

```typescript
// Counter 指标
workflow.execution.count{workflow_id, workflow_version, execution_type}
workflow.execution.success.count{workflow_id}
workflow.execution.failure.count{workflow_id, error_type}

// Histogram 指标  
workflow.execution.duration{workflow_id, workflow_version}
workflow.execution.node_count{workflow_id}

// Gauge 指标
workflow.execution.active.count{workflow_id}
```

**实现示例**:

```typescript
export class WorkflowMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record workflow execution start
   */
  recordExecutionStart(workflowId: string, executionId: string, options: {
    version?: string;
    executionType?: 'MAIN' | 'FORK_JOIN' | 'TRIGGERED_SUBWORKFLOW';
  }): void {
    // Increment execution count
    this.incrementCounter('workflow.execution.count', {
      workflow_id: workflowId,
      workflow_version: options.version || 'unknown',
      execution_type: options.executionType || 'MAIN',
      execution_id: executionId,
    });

    // Track active executions
    this.incrementCounter('workflow.execution.active.count', {
      workflow_id: workflowId,
    });
  }

  /**
   * Record workflow execution completion
   */
  recordExecutionComplete(workflowId: string, executionId: string, result: {
    success: boolean;
    duration: number;
    nodeCount: number;
    errorType?: string;
  }): void {
    // Decrement active executions
    this.incrementCounter('workflow.execution.active.count', {
      workflow_id: workflowId,
    }, -1);

    // Record duration
    this.observeHistogram('workflow.execution.duration', result.duration, {
      workflow_id: workflowId,
    });

    // Record node count
    this.observeHistogram('workflow.execution.node_count', result.nodeCount, {
      workflow_id: workflowId,
    });

    // Record success/failure
    if (result.success) {
      this.incrementCounter('workflow.execution.success.count', {
        workflow_id: workflowId,
      });
    } else {
      this.incrementCounter('workflow.execution.failure.count', {
        workflow_id: workflowId,
        error_type: result.errorType || 'unknown',
      });
    }
  }

  /**
   * Get workflow usage statistics
   */
  getWorkflowUsageStats(workflowId?: string): {
    totalExecutions: number;
    successRate: number;
    avgDuration: number;
    p95Duration: number;
    p99Duration: number;
    byVersion: Record<string, number>;
  } {
    // Implementation using query() method
  }
}
```

### 2. Node Template Metrics Collector

**位置**: `sdk/core/metrics/node-metrics-collector.ts`

**职责**:
- 追踪 Node Template 的实例化次数
- 统计各类 Node 的执行频率
- 记录 Node 执行时长和成功率
- 按 Node Type 和 Category 分组

**关键指标**:

```typescript
// Counter 指标
node.template.instantiation.count{template_name, node_type, category}
node.execution.count{node_type, node_id, workflow_id}
node.execution.success.count{node_type}
node.execution.failure.count{node_type, error_type}

// Histogram 指标
node.execution.duration{node_type, node_id}
node.execution.token_usage{node_type}  // For LLM nodes
```

**实现示例**:

```typescript
export class NodeMetricsCollector extends BaseMetricCollector {
  /**
   * Record node template instantiation
   */
  recordTemplateInstantiation(templateName: string, nodeType: string, metadata?: {
    category?: string;
    tags?: string[];
  }): void {
    this.incrementCounter('node.template.instantiation.count', {
      template_name: templateName,
      node_type: nodeType,
      category: metadata?.category || 'uncategorized',
    });
  }

  /**
   * Record node execution
   */
  recordNodeExecution(nodeId: string, nodeType: string, workflowId: string, result: {
    success: boolean;
    duration: number;
    tokenUsage?: number;
    errorType?: string;
  }): void {
    this.incrementCounter('node.execution.count', {
      node_type: nodeType,
      node_id: nodeId,
      workflow_id: workflowId,
    });

    this.observeHistogram('node.execution.duration', result.duration, {
      node_type: nodeType,
      node_id: nodeId,
    });

    if (result.success) {
      this.incrementCounter('node.execution.success.count', {
        node_type: nodeType,
      });
    } else {
      this.incrementCounter('node.execution.failure.count', {
        node_type: nodeType,
        error_type: result.errorType || 'unknown',
      });
    }

    // For LLM nodes, track token usage
    if (result.tokenUsage !== undefined && nodeType === 'LLM') {
      this.observeHistogram('node.execution.token_usage', result.tokenUsage, {
        node_type: nodeType,
      });
    }
  }

  /**
   * Get most frequently used node templates
   */
  getTopNodeTemplates(limit: number = 10): Array<{
    templateName: string;
    nodeType: string;
    instantiationCount: number;
  }> {
    // Implementation
  }
}
```

### 3. Agent Loop Metrics Collector

**位置**: `sdk/core/metrics/agent-metrics-collector.ts`

**职责**:
- 追踪 Agent Loop 配置的调用次数
- 统计迭代次数分布
- 记录 Tool 调用频率
- 按 Profile ID 和配置分组

**关键指标**:

```typescript
// Counter 指标
agent.loop.execution.count{profile_id, agent_config_id}
agent.loop.iteration.count{profile_id}
agent.tool.call.count{tool_name, profile_id}

// Histogram 指标
agent.loop.duration{profile_id}
agent.loop.iterations_per_execution{profile_id}
agent.loop.tokens_per_iteration{profile_id}
```

**实现示例**:

```typescript
export class AgentMetricsCollector extends BaseMetricCollector {
  /**
   * Record agent loop execution start
   */
  recordExecutionStart(profileId: string, agentConfigId: string, executionId: string): void {
    this.incrementCounter('agent.loop.execution.count', {
      profile_id: profileId,
      agent_config_id: agentConfigId,
      execution_id: executionId,
    });
  }

  /**
   * Record agent loop completion
   */
  recordExecutionComplete(profileId: string, result: {
    iterations: number;
    toolCallCount: number;
    duration: number;
    tokenUsage?: number;
    success: boolean;
  }): void {
    this.observeHistogram('agent.loop.duration', result.duration, {
      profile_id: profileId,
    });

    this.observeHistogram('agent.loop.iterations_per_execution', result.iterations, {
      profile_id: profileId,
    });

    if (result.tokenUsage !== undefined) {
      this.observeHistogram('agent.loop.tokens_per_iteration', 
        result.tokenUsage / Math.max(result.iterations, 1), {
        profile_id: profileId,
      });
    }
  }

  /**
   * Record iteration
   */
  recordIteration(profileId: string, iteration: number): void {
    this.incrementCounter('agent.loop.iteration.count', {
      profile_id: profileId,
    });
  }

  /**
   * Record tool call
   */
  recordToolCall(toolName: string, profileId: string, result: {
    success: boolean;
    duration: number;
  }): void {
    this.incrementCounter('agent.tool.call.count', {
      tool_name: toolName,
      profile_id: profileId,
    });

    this.observeHistogram('agent.tool.execution.duration', result.duration, {
      tool_name: toolName,
    });
  }
}
```

### 4. Unified Metrics Manager

**位置**: `sdk/core/metrics/unified-metrics-manager.ts`

**职责**:
- 统一管理所有 Metrics Collectors
- 提供全局查询接口
- 协调定期刷新和报告
- 集成到 GlobalContext

**实现**:

```typescript
export interface MetricsManagerConfig {
  workflowMetrics?: MetricCollectorConfig;
  nodeMetrics?: MetricCollectorConfig;
  agentMetrics?: MetricCollectorConfig;
  enablePeriodicReporting?: boolean;
  reportingInterval?: number;
}

export class UnifiedMetricsManager {
  private workflowMetrics: WorkflowMetricsCollector;
  private nodeMetrics: NodeMetricsCollector;
  private agentMetrics: AgentMetricsCollector;
  private eventCollector: EventMetricsCollector;

  constructor(globalContext: GlobalContext, config?: MetricsManagerConfig) {
    this.workflowMetrics = new WorkflowMetricsCollector(config?.workflowMetrics);
    this.nodeMetrics = new NodeMetricsCollector(config?.nodeMetrics);
    this.agentMetrics = new AgentMetricsCollector(config?.agentMetrics);
    this.eventCollector = globalContext.eventRegistry.getMetricsCollector();

    // Setup periodic reporting
    if (config?.enablePeriodicReporting) {
      this.setupPeriodicReporting(config.reportingInterval);
    }
  }

  /**
   * Get all collectors
   */
  getCollectors(): {
    workflow: WorkflowMetricsCollector;
    node: NodeMetricsCollector;
    agent: AgentMetricsCollector;
    event: EventMetricsCollector;
  } {
    return {
      workflow: this.workflowMetrics,
      node: this.nodeMetrics,
      agent: this.agentMetrics,
      event: this.eventCollector,
    };
  }

  /**
   * Generate comprehensive report
   */
  async generateReport(options?: {
    timeRange?: { from: number; to: number };
    includeTrends?: boolean;
  }): Promise<MetricReport> {
    // Aggregate from all collectors
  }

  /**
   * Flush all metrics
   */
  async flushAll(): Promise<void> {
    await Promise.all([
      this.workflowMetrics.flush(),
      this.nodeMetrics.flush(),
      this.agentMetrics.flush(),
      this.eventCollector.flush(),
    ]);
  }

  /**
   * Dispose all collectors
   */
  dispose(): void {
    this.workflowMetrics.dispose();
    this.nodeMetrics.dispose();
    this.agentMetrics.dispose();
  }
}
```

## 集成点

### 1. Workflow Lifecycle Integration

**位置**: `sdk/workflow/execution/coordinators/workflow-lifecycle-coordinator.ts`

在关键生命周期事件中注入 metrics 记录:

```typescript
async execute(workflowId: string, options: WorkflowExecutionOptions = {}): Promise<WorkflowExecutionResult> {
  const startTime = now();
  
  // Step 1: Build entity
  const { workflowExecutionEntity } = await this.workflowExecutionBuilder.build(workflowId, options);
  const executionId = workflowExecutionEntity.id;

  // [NEW] Record execution start
  const metricsManager = this.globalContext.container.get(Identifiers.MetricsManager);
  metricsManager.getCollectors().workflow.recordExecutionStart(workflowId, executionId, {
    version: workflowExecutionEntity.getWorkflowVersion(),
    executionType: workflowExecutionEntity.getExecutionType(),
  });

  // Step 2-5: Execute workflow (existing logic)
  this.workflowExecutionRegistry.register(workflowExecutionEntity);
  await this.workflowStateTransitor.startWorkflowExecution(workflowExecutionEntity);
  const result = await this.workflowExecutor.executeWorkflow(workflowExecutionEntity);
  
  // Update status
  const status = result.metadata?.status;
  if (status === "COMPLETED") {
    await this.workflowStateTransitor.completeWorkflowExecution(workflowExecutionEntity, result);
  } else {
    await this.workflowStateTransitor.failWorkflowExecution(workflowExecutionEntity, lastError);
  }

  // [NEW] Record execution complete
  const duration = diffTimestamp(startTime, now());
  metricsManager.getCollectors().workflow.recordExecutionComplete(workflowId, executionId, {
    success: status === "COMPLETED",
    duration,
    nodeCount: result.nodeResults.length,
    errorType: status === "FAILED" ? getLastErrorCode(result.errors) : undefined,
  });

  return result;
}
```

### 2. Node Execution Integration

**位置**: `sdk/workflow/execution/coordinators/node-execution-coordinator.ts`

```typescript
async executeNode(...): Promise<NodeExecutionResult> {
  const startTime = now();
  const nodeId = node.id;
  const nodeType = node.type;
  const workflowId = workflowExecutionEntity.getWorkflowId();

  // [NEW] Record node execution start
  const metricsManager = this.globalContext.container.get(Identifiers.MetricsManager);
  metricsManager.getCollectors().node.recordNodeExecutionStart(nodeId, nodeType, workflowId);

  try {
    // Execute node handler (existing logic)
    const output = await handler(this.globalContext, workflowExecutionEntity, node, handlerContext);
    
    const duration = diffTimestamp(startTime, now());
    
    // [NEW] Record successful execution
    metricsManager.getCollectors().node.recordNodeExecution(nodeId, nodeType, workflowId, {
      success: true,
      duration,
      tokenUsage: extractTokenUsage(output),
    });

    return { /* existing result */ };
  } catch (error) {
    const duration = diffTimestamp(startTime, now());
    
    // [NEW] Record failed execution
    metricsManager.getCollectors().node.recordNodeExecution(nodeId, nodeType, workflowId, {
      success: false,
      duration,
      errorType: getErrorType(error),
    });

    throw error;
  }
}
```

### 3. Agent Loop Integration

**位置**: `sdk/agent/execution/executors/agent-loop-executor.ts`

```typescript
async execute(entity: AgentLoopEntity): Promise<AgentLoopResult> {
  const startTime = now();
  const profileId = entity.config.profileId || 'DEFAULT';
  const agentConfigId = entity.config.id;

  // [NEW] Record execution start
  const metricsManager = this.globalContext.container.get(Identifiers.MetricsManager);
  metricsManager.getCollectors().agent.recordExecutionStart(profileId, agentConfigId, entity.id);

  // Execute (existing logic)
  const coordinator = this.createCoordinator();
  const result = await coordinator.execute(entity, ...);

  // [NEW] Record execution complete
  const duration = diffTimestamp(startTime, now());
  metricsManager.getCollectors().agent.recordExecutionComplete(profileId, {
    iterations: result.iterations,
    toolCallCount: result.toolCallCount,
    duration,
    tokenUsage: result.tokenUsage,
    success: result.success,
  });

  return result;
}
```

### 4. Node Template Registration Integration

**位置**: `sdk/core/registry/node-template-registry.ts`

```typescript
register(template: NodeTemplate): void {
  // Existing validation and registration logic
  
  // [NEW] Record template instantiation
  const metricsManager = this.globalContext.container.get(Identifiers.MetricsManager);
  metricsManager.getCollectors().node.recordTemplateInstantiation(
    template.name,
    template.type,
    {
      category: template.metadata?.['category'] as string,
      tags: template.metadata?.['tags'] as string[],
    }
  );
  
  this.templates.set(template.name, template);
}
```

## API 层扩展

### 1. Metrics Resource API

**位置**: `sdk/api/shared/resources/metrics/metrics-resource-api.ts`

```typescript
export class MetricsResourceAPI {
  constructor(private deps: APIDependencyManager) {}

  /**
   * Get workflow usage statistics
   */
  async getWorkflowMetrics(options?: {
    workflowId?: string;
    timeRange?: { from: number; to: number };
  }): Promise<{
    totalExecutions: number;
    successRate: number;
    avgDuration: number;
    p95Duration: number;
    byWorkflow: Record<string, {
      count: number;
      successRate: number;
      avgDuration: number;
    }>;
  }> {
    const collector = this.deps.getMetricsManager().getCollectors().workflow;
    return collector.getWorkflowUsageStats(options?.workflowId);
  }

  /**
   * Get top workflows by usage
   */
  async getTopWorkflows(limit: number = 10): Promise<Array<{
    workflowId: string;
    workflowName?: string;
    executionCount: number;
    successRate: number;
  }>> {
    // Implementation
  }

  /**
   * Get node template usage statistics
   */
  async getNodeTemplateMetrics(options?: {
    templateName?: string;
    nodeType?: string;
  }): Promise<{
    totalInstantiations: number;
    totalExecutions: number;
    byTemplate: Record<string, number>;
    byNodeType: Record<string, number>;
  }> {
    const collector = this.deps.getMetricsManager().getCollectors().node;
    return collector.getNodeTemplateStats(options);
  }

  /**
   * Get agent loop metrics
   */
  async getAgentMetrics(options?: {
    profileId?: string;
  }): Promise<{
    totalExecutions: number;
    avgIterations: number;
    avgToolCalls: number;
    byProfile: Record<string, number>;
  }> {
    const collector = this.deps.getMetricsManager().getCollectors().agent;
    return collector.getAgentStats(options?.profileId);
  }

  /**
   * Export metrics in Prometheus format
   */
  async exportPrometheus(): Promise<string> {
    // Convert all metrics to Prometheus exposition format
  }
}
```

### 2. CLI Commands

**位置**: `apps/cli-app/src/commands/metrics/index.ts`

```bash
# View workflow usage
wf-agent metrics workflow [--workflow-id <id>] [--top <n>]

# View node template usage
wf-agent metrics node-templates [--type <type>] [--top <n>]

# View agent loop usage
wf-agent metrics agents [--profile <id>]

# Export metrics
wf-agent metrics export --format prometheus|json --output <file>
```

## 持久化方案

### 阶段 1: 内存 + 定期导出 (当前)

- 使用现有的 `MetricCollector` buffer
- 定期 flush 到文件(JSONL格式)
- 适合开发和小型部署

### 阶段 2: SQLite 集成 (中期)

利用现有的 `@wf-agent/storage` 包:

```typescript
export class SQLiteMetricsStorage implements MetricsStorageAdapter {
  async storeMetrics(metrics: Metric[]): Promise<void> {
    // Batch insert into metrics table
  }

  async queryMetrics(filter: MetricFilter): Promise<Metric[]> {
    // SQL query with proper indexing
  }

  async aggregateMetrics(query: AggregationQuery): Promise<AggregatedMetric[]> {
    // Use SQL aggregation functions
  }
}
```

表结构:

```sql
CREATE TABLE metrics (
  id TEXT PRIMARY KEY,
  metric_name TEXT NOT NULL,
  metric_type TEXT NOT NULL,
  value REAL NOT NULL,
  labels TEXT,  -- JSON encoded
  timestamp INTEGER NOT NULL,
  INDEX idx_metric_name (metric_name),
  INDEX idx_timestamp (timestamp),
  INDEX idx_labels (labels)
);
```

### 阶段 3: 时序数据库 (长期)

可选集成:
- InfluxDB
- TimescaleDB
- Prometheus

通过适配器模式实现:

```typescript
interface MetricsStorageAdapter {
  storeMetrics(metrics: Metric[]): Promise<void>;
  queryMetrics(filter: MetricFilter): Promise<Metric[]>;
  aggregateMetrics(query: AggregationQuery): Promise<AggregatedMetric[]>;
}
```

## 监控和告警

### 关键指标阈值

```typescript
const ALERT_THRESHOLDS = {
  // Workflow
  'workflow.failure.rate': { warning: 0.1, critical: 0.3 },  // >10% failure rate
  'workflow.duration.p95': { warning: 60000, critical: 300000 },  // >60s p95
  
  // Node
  'node.failure.rate': { warning: 0.05, critical: 0.2 },
  
  // Agent
  'agent.iteration.avg': { warning: 8, critical: 15 },  // Too many iterations
};
```

### 定期报告

```typescript
// Daily summary report
{
  period: "2026-05-14",
  workflows: {
    totalExecutions: 1234,
    topWorkflows: [
      { id: "wf-1", name: "Code Review", count: 456 },
      { id: "wf-2", name: "Data Processing", count: 234 },
    ],
    avgSuccessRate: 0.95,
  },
  nodes: {
    totalExecutions: 5678,
    mostUsedTypes: ["LLM", "SCRIPT", "TOOL"],
  },
  agents: {
    totalExecutions: 890,
    avgIterations: 4.5,
  }
}
```

## 实施计划

### Phase 1: 基础实现 (1-2周)

1. ✅ 创建三个 Metrics Collectors
   - `WorkflowMetricsCollector`
   - `NodeMetricsCollector`
   - `AgentMetricsCollector`

2. ✅ 创建 `UnifiedMetricsManager`

3. ✅ 集成到关键执行路径
   - Workflow lifecycle
   - Node execution
   - Agent loop execution

4. ✅ 添加基础查询 API

### Phase 2: API 和工具 (1周)

1. ✅ 实现 `MetricsResourceAPI`

2. ✅ 添加 CLI commands

3. ✅ 集成到 SDK dependencies

4. ✅ 编写单元测试

### Phase 3: 持久化和优化 (2-3周)

1. ⏳ 实现 SQLite storage adapter

2. ⏳ 添加定期 flush 机制

3. ⏳ 实现数据清理策略(保留最近N天)

4. ⏳ 性能优化和压力测试

### Phase 4: 高级功能 (按需)

1. 📋 Prometheus 导出

2. 📋 Dashboard 集成

3. 📋 告警系统

4. 📋 趋势分析和预测

## 使用示例

### 查询高频工作流

```typescript
const metricsApi = sdk.metrics;

// Get top 10 workflows by execution count
const topWorkflows = await metricsApi.getTopWorkflows(10);
console.log("Most used workflows:");
topWorkflows.forEach(wf => {
  console.log(`  ${wf.workflowId}: ${wf.executionCount} executions (${(wf.successRate * 100).toFixed(1)}% success)`);
});

// Get detailed stats for a specific workflow
const stats = await metricsApi.getWorkflowMetrics({ workflowId: "code-review-v2" });
console.log(`Average duration: ${stats.avgDuration}ms`);
console.log(`P95 duration: ${stats.p95Duration}ms`);
```

### 查询常用 Node 模板

```typescript
// Get most instantiated node templates
const nodeStats = await metricsApi.getNodeTemplateMetrics();
console.log("Most used node templates:");
Object.entries(nodeStats.byTemplate)
  .sort(([,a], [,b]) => b - a)
  .slice(0, 10)
  .forEach(([name, count]) => {
    console.log(`  ${name}: ${count} instantiations`);
  });
```

### 监控 Agent 使用情况

```typescript
// Get agent loop metrics by profile
const agentStats = await metricsApi.getAgentMetrics();
console.log("Agent usage by profile:");
Object.entries(agentStats.byProfile).forEach(([profileId, count]) => {
  console.log(`  ${profileId}: ${count} executions`);
});
```

## 注意事项

### 性能考虑

1. **异步记录**: Metrics 记录不应阻塞主执行流程
2. **批量处理**: 使用 buffer 批量写入,减少 I/O
3. **采样策略**: 对于高频事件,考虑采样记录(如每10次记录1次)
4. **内存管理**: 定期清理旧数据,设置最大 buffer 大小

### 数据隐私

1. **脱敏**: 不要在 metrics 中记录敏感数据(input/output内容)
2. **仅元数据**: 只记录执行次数、时长等元数据
3. **可配置**: 允许用户禁用 metrics 收集

### 向后兼容

1. **可选功能**: Metrics 应该是可选的,不影响核心功能
2. **默认关闭持久化**: 初期默认只在内存中,避免影响现有部署
3. **渐进式启用**: 通过配置逐步启用各项功能

## 总结

本方案提供了一个完整的 Metrics 系统设计,能够:

✅ **追踪静态定义的调用次数**: Workflow、Node Template、Agent Config
✅ **多维度分析**: 按类型、版本、状态等分组统计
✅ **性能监控**: 执行时长分布、成功率、错误类型
✅ **可扩展架构**: 支持自定义指标和存储后端
✅ **易于集成**: 通过事件驱动,最小化对现有代码的影响

实施后,用户可以轻松回答以下问题:
- 哪个工作流被最频繁使用?
- 哪些 Node 模板最受欢迎?
- Agent 配置的平均迭代次数是多少?
- 工作流的 P95 执行时长是多少?
- 哪些工作流失败率较高?

这将为性能优化、资源规划和产品决策提供重要数据支持。
