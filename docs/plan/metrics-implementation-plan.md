# Metrics 系统实施计划

## 概述

本文档提供 Metrics 系统的具体实施步骤和代码实现指南。

## Phase 1: 核心 Collectors 实现

### Step 1.1: Workflow Metrics Collector

**文件**: `sdk/core/metrics/workflow-metrics-collector.ts`

```typescript
/**
 * Workflow Metrics Collector
 * 
 * Tracks workflow execution metrics including:
 * - Execution count by workflow ID and version
 * - Success/failure rates
 * - Execution duration distributions
 * - Active execution counts
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig } from "./types.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "WorkflowMetricsCollector" });

export interface WorkflowExecutionLabels {
  workflow_id: string;
  workflow_version?: string;
  execution_type?: 'MAIN' | 'FORK_JOIN' | 'TRIGGERED_SUBWORKFLOW';
  execution_id?: string;
  error_type?: string;
}

export class WorkflowMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record workflow execution start
   */
  recordExecutionStart(workflowId: string, executionId: string, labels?: {
    version?: string;
    executionType?: 'MAIN' | 'FORK_JOIN' | 'TRIGGERED_SUBWORKFLOW';
  }): void {
    if (!workflowId || !executionId) {
      logger.warn("recordExecutionStart called with missing parameters", { workflowId, executionId });
      return;
    }

    // Increment execution count
    this.incrementCounter('workflow.execution.count', {
      workflow_id: workflowId,
      workflow_version: labels?.version || 'unknown',
      execution_type: labels?.executionType || 'MAIN',
      execution_id: executionId,
    });

    // Track active executions
    this.incrementCounter('workflow.execution.active.count', {
      workflow_id: workflowId,
    });

    logger.debug("Recorded workflow execution start", { workflowId, executionId });
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
    if (!workflowId || !executionId) {
      logger.warn("recordExecutionComplete called with missing parameters");
      return;
    }

    // Decrement active executions
    this.incrementCounter('workflow.execution.active.count', {
      workflow_id: workflowId,
    }, -1);

    // Record duration histogram
    this.observeHistogram('workflow.execution.duration', result.duration, {
      workflow_id: workflowId,
    });

    // Record node count
    this.observeHistogram('workflow.execution.node_count', result.nodeCount, {
      workflow_id: workflowId,
    });

    // Record success or failure
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

    logger.debug("Recorded workflow execution complete", { 
      workflowId, 
      executionId, 
      success: result.success,
      duration: result.duration 
    });
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
    const filter = workflowId ? {
      labels: { workflow_id: workflowId }
    } : {};

    // Query execution count
    const countResult = this.query({
      metricName: 'workflow.execution.count',
      metricType: 'counter',
      ...filter,
    });

    // Query success/failure counts
    const successResult = this.query({
      metricName: 'workflow.execution.success.count',
      metricType: 'counter',
      ...filter,
    });

    const failureResult = this.query({
      metricName: 'workflow.execution.failure.count',
      metricType: 'counter',
      ...filter,
    });

    // Query duration histogram
    const durationResult = this.query({
      metricName: 'workflow.execution.duration',
      metricType: 'histogram',
      ...filter,
    });

    const totalExecutions = countResult.totalCount;
    const successCount = successResult.totalCount;
    const failureCount = failureResult.totalCount;
    const successRate = totalExecutions > 0 ? successCount / totalExecutions : 0;

    // Calculate duration statistics from histogram
    let avgDuration = 0;
    let p95Duration = 0;
    let p99Duration = 0;

    const durationMetric = durationResult.metrics.get('workflow.execution.duration');
    if (durationMetric && durationMetric.timeSeries && durationMetric.timeSeries.length > 0) {
      const values = durationMetric.timeSeries.map(ts => ts.value).sort((a, b) => a - b);
      avgDuration = values.reduce((sum, v) => sum + v, 0) / values.length;
      p95Duration = values[Math.floor(values.length * 0.95)] || 0;
      p99Duration = values[Math.floor(values.length * 0.99)] || 0;
    }

    // Group by version
    const byVersion: Record<string, number> = {};
    const countMetric = countResult.metrics.get('workflow.execution.count');
    if (countMetric) {
      for (const [labelKey, labelAgg] of countMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.workflow_version) {
            byVersion[labels.workflow_version] = (byVersion[labels.workflow_version] || 0) + labelAgg.value;
          }
        } catch (error) {
          logger.warn("Failed to parse label key", { labelKey, error });
        }
      }
    }

    return {
      totalExecutions,
      successRate,
      avgDuration,
      p95Duration,
      p99Duration,
      byVersion,
    };
  }

  /**
   * Get top workflows by execution count
   */
  getTopWorkflows(limit: number = 10): Array<{
    workflowId: string;
    executionCount: number;
    successRate: number;
  }> {
    const countResult = this.query({
      metricName: 'workflow.execution.count',
      metricType: 'counter',
    });

    const workflows: Map<string, { count: number; success: number; failure: number }> = new Map();

    // Aggregate by workflow_id
    const countMetric = countResult.metrics.get('workflow.execution.count');
    if (countMetric) {
      for (const [labelKey, labelAgg] of countMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.workflow_id) {
            const wfId = labels.workflow_id;
            if (!workflows.has(wfId)) {
              workflows.set(wfId, { count: 0, success: 0, failure: 0 });
            }
            workflows.get(wfId)!.count += labelAgg.value;
          }
        } catch (error) {
          logger.warn("Failed to parse label key", { labelKey, error });
        }
      }
    }

    // Get success counts
    const successMetric = this.query({
      metricName: 'workflow.execution.success.count',
      metricType: 'counter',
    }).metrics.get('workflow.execution.success.count');

    if (successMetric) {
      for (const [labelKey, labelAgg] of successMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.workflow_id && workflows.has(labels.workflow_id)) {
            workflows.get(labels.workflow_id)!.success += labelAgg.value;
          }
        } catch (error) {
          // Ignore parsing errors
        }
      }
    }

    // Sort and return top N
    return Array.from(workflows.entries())
      .map(([workflowId, stats]) => ({
        workflowId,
        executionCount: stats.count,
        successRate: stats.count > 0 ? stats.success / stats.count : 0,
      }))
      .sort((a, b) => b.executionCount - a.executionCount)
      .slice(0, limit);
  }
}
```

### Step 1.2: Node Metrics Collector

**文件**: `sdk/core/metrics/node-metrics-collector.ts`

```typescript
/**
 * Node Metrics Collector
 * 
 * Tracks node template and execution metrics including:
 * - Template instantiation count
 * - Node execution frequency by type
 * - Execution duration and success rate
 * - Token usage for LLM nodes
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig } from "./types.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "NodeMetricsCollector" });

export class NodeMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record node template instantiation
   */
  recordTemplateInstantiation(templateName: string, nodeType: string, metadata?: {
    category?: string;
    tags?: string[];
  }): void {
    if (!templateName || !nodeType) {
      logger.warn("recordTemplateInstantiation called with missing parameters");
      return;
    }

    this.incrementCounter('node.template.instantiation.count', {
      template_name: templateName,
      node_type: nodeType,
      category: metadata?.category || 'uncategorized',
    });

    logger.debug("Recorded node template instantiation", { templateName, nodeType });
  }

  /**
   * Record node execution start
   */
  recordNodeExecutionStart(nodeId: string, nodeType: string, workflowId: string): void {
    if (!nodeId || !nodeType || !workflowId) {
      logger.warn("recordNodeExecutionStart called with missing parameters");
      return;
    }

    this.incrementCounter('node.execution.started.count', {
      node_type: nodeType,
      node_id: nodeId,
      workflow_id: workflowId,
    });
  }

  /**
   * Record node execution completion
   */
  recordNodeExecution(nodeId: string, nodeType: string, workflowId: string, result: {
    success: boolean;
    duration: number;
    tokenUsage?: number;
    errorType?: string;
  }): void {
    if (!nodeId || !nodeType || !workflowId) {
      logger.warn("recordNodeExecution called with missing parameters");
      return;
    }

    // Record execution count
    this.incrementCounter('node.execution.count', {
      node_type: nodeType,
      node_id: nodeId,
      workflow_id: workflowId,
    });

    // Record duration
    this.observeHistogram('node.execution.duration', result.duration, {
      node_type: nodeType,
      node_id: nodeId,
    });

    // Record success/failure
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

    logger.debug("Recorded node execution", { 
      nodeId, 
      nodeType, 
      workflowId,
      success: result.success,
      duration: result.duration 
    });
  }

  /**
   * Get most frequently used node templates
   */
  getTopNodeTemplates(limit: number = 10): Array<{
    templateName: string;
    nodeType: string;
    instantiationCount: number;
  }> {
    const result = this.query({
      metricName: 'node.template.instantiation.count',
      metricType: 'counter',
    });

    const templates: Map<string, { name: string; type: string; count: number }> = new Map();

    const metric = result.metrics.get('node.template.instantiation.count');
    if (metric) {
      for (const [labelKey, labelAgg] of metric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.template_name && labels.node_type) {
            const key = `${labels.template_name}:${labels.node_type}`;
            templates.set(key, {
              name: labels.template_name,
              type: labels.node_type,
              count: labelAgg.value,
            });
          }
        } catch (error) {
          logger.warn("Failed to parse label key", { labelKey, error });
        }
      }
    }

    return Array.from(templates.values())
      .sort((a, b) => b.count - a.count)
      .slice(0, limit)
      .map(t => ({
        templateName: t.name,
        nodeType: t.type,
        instantiationCount: t.count,
      }));
  }

  /**
   * Get node execution statistics by type
   */
  getNodeExecutionStatsByType(): Record<string, {
    totalCount: number;
    successRate: number;
    avgDuration: number;
  }> {
    const result: Record<string, { totalCount: number; successCount: number; totalDuration: number }> = {};

    // Get counts by type
    const countResult = this.query({
      metricName: 'node.execution.count',
      metricType: 'counter',
    });

    const countMetric = countResult.metrics.get('node.execution.count');
    if (countMetric) {
      for (const [labelKey, labelAgg] of countMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.node_type) {
            const nodeType = labels.node_type;
            if (!result[nodeType]) {
              result[nodeType] = { totalCount: 0, successCount: 0, totalDuration: 0 };
            }
            result[nodeType].totalCount += labelAgg.value;
          }
        } catch (error) {
          // Ignore
        }
      }
    }

    // Get success counts
    const successResult = this.query({
      metricName: 'node.execution.success.count',
      metricType: 'counter',
    });

    const successMetric = successResult.metrics.get('node.execution.success.count');
    if (successMetric) {
      for (const [labelKey, labelAgg] of successMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.node_type && result[labels.node_type]) {
            result[labels.node_type].successCount += labelAgg.value;
          }
        } catch (error) {
          // Ignore
        }
      }
    }

    // Calculate final stats
    const stats: Record<string, { totalCount: number; successRate: number; avgDuration: number }> = {};
    for (const [nodeType, data] of Object.entries(result)) {
      stats[nodeType] = {
        totalCount: data.totalCount,
        successRate: data.totalCount > 0 ? data.successCount / data.totalCount : 0,
        avgDuration: 0, // Would need to query duration histogram
      };
    }

    return stats;
  }
}
```

### Step 1.3: Agent Metrics Collector

**文件**: `sdk/core/metrics/agent-metrics-collector.ts`

```typescript
/**
 * Agent Loop Metrics Collector
 * 
 * Tracks agent loop execution metrics including:
 * - Execution count by profile and config
 * - Iteration count distributions
 * - Tool call frequency
 * - Token usage
 */

import { BaseMetricCollector } from "./base-collector.js";
import type { MetricCollectorConfig } from "./types.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "AgentMetricsCollector" });

export class AgentMetricsCollector extends BaseMetricCollector {
  constructor(config?: MetricCollectorConfig) {
    super(config);
  }

  /**
   * Record agent loop execution start
   */
  recordExecutionStart(profileId: string, agentConfigId: string, executionId: string): void {
    if (!profileId || !executionId) {
      logger.warn("recordExecutionStart called with missing parameters");
      return;
    }

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
    if (!profileId) {
      logger.warn("recordExecutionComplete called with missing profileId");
      return;
    }

    // Record duration
    this.observeHistogram('agent.loop.duration', result.duration, {
      profile_id: profileId,
    });

    // Record iteration count
    this.observeHistogram('agent.loop.iterations_per_execution', result.iterations, {
      profile_id: profileId,
    });

    // Record token usage per iteration
    if (result.tokenUsage !== undefined && result.iterations > 0) {
      this.observeHistogram('agent.loop.tokens_per_iteration', 
        result.tokenUsage / result.iterations, {
        profile_id: profileId,
      });
    }

    logger.debug("Recorded agent loop completion", { 
      profileId, 
      iterations: result.iterations,
      duration: result.duration 
    });
  }

  /**
   * Record iteration
   */
  recordIteration(profileId: string, iteration: number): void {
    if (!profileId) {
      logger.warn("recordIteration called with missing profileId");
      return;
    }

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
    if (!toolName || !profileId) {
      logger.warn("recordToolCall called with missing parameters");
      return;
    }

    this.incrementCounter('agent.tool.call.count', {
      tool_name: toolName,
      profile_id: profileId,
    });

    this.observeHistogram('agent.tool.execution.duration', result.duration, {
      tool_name: toolName,
    });
  }

  /**
   * Get agent loop statistics
   */
  getAgentStats(profileId?: string): {
    totalExecutions: number;
    avgIterations: number;
    avgToolCalls: number;
    byProfile: Record<string, number>;
  } {
    const filter = profileId ? {
      labels: { profile_id: profileId }
    } : {};

    // Query execution count
    const countResult = this.query({
      metricName: 'agent.loop.execution.count',
      metricType: 'counter',
      ...filter,
    });

    // Query iteration histogram
    const iterationResult = this.query({
      metricName: 'agent.loop.iterations_per_execution',
      metricType: 'histogram',
      ...filter,
    });

    const totalExecutions = countResult.totalCount;

    // Calculate average iterations
    let avgIterations = 0;
    const iterationMetric = iterationResult.metrics.get('agent.loop.iterations_per_execution');
    if (iterationMetric && iterationMetric.timeSeries && iterationMetric.timeSeries.length > 0) {
      const values = iterationMetric.timeSeries.map(ts => ts.value);
      avgIterations = values.reduce((sum, v) => sum + v, 0) / values.length;
    }

    // Group by profile
    const byProfile: Record<string, number> = {};
    const countMetric = countResult.metrics.get('agent.loop.execution.count');
    if (countMetric) {
      for (const [labelKey, labelAgg] of countMetric.byLabel.entries()) {
        try {
          const labels = JSON.parse(labelKey);
          if (labels.profile_id) {
            byProfile[labels.profile_id] = (byProfile[labels.profile_id] || 0) + labelAgg.value;
          }
        } catch (error) {
          logger.warn("Failed to parse label key", { labelKey, error });
        }
      }
    }

    return {
      totalExecutions,
      avgIterations,
      avgToolCalls: 0, // Would need additional tracking
      byProfile,
    };
  }
}
```

### Step 1.4: Unified Metrics Manager

**文件**: `sdk/core/metrics/unified-metrics-manager.ts`

```typescript
/**
 * Unified Metrics Manager
 * 
 * Manages all metrics collectors and provides centralized access.
 * Integrated into GlobalContext for easy access throughout the SDK.
 */

import type { GlobalContext } from "../global-context.js";
import { WorkflowMetricsCollector } from "./workflow-metrics-collector.js";
import { NodeMetricsCollector } from "./node-metrics-collector.js";
import { AgentMetricsCollector } from "./agent-metrics-collector.js";
import type { EventMetricsCollector } from "./event-collector.js";
import type { MetricCollectorConfig, MetricReport } from "./types.js";
import { createContextualLogger } from "../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "UnifiedMetricsManager" });

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
  private eventCollector: EventMetricsCollector | null = null;
  private reportSubscribers: Array<(report: MetricReport) => void | Promise<void>> = [];
  private reportingTimer: NodeJS.Timeout | null = null;

  constructor(
    private globalContext: GlobalContext,
    config?: MetricsManagerConfig
  ) {
    this.workflowMetrics = new WorkflowMetricsCollector(config?.workflowMetrics);
    this.nodeMetrics = new NodeMetricsCollector(config?.nodeMetrics);
    this.agentMetrics = new AgentMetricsCollector(config?.agentMetrics);

    // Get event collector from event registry
    try {
      this.eventCollector = this.globalContext.eventRegistry.getMetricsCollector();
    } catch (error) {
      logger.warn("Event metrics collector not available", { error });
    }

    // Setup periodic reporting if enabled
    if (config?.enablePeriodicReporting) {
      this.setupPeriodicReporting(config.reportingInterval || 60000); // Default: 1 minute
    }

    logger.info("Unified Metrics Manager initialized");
  }

  /**
   * Get all collectors
   */
  getCollectors(): {
    workflow: WorkflowMetricsCollector;
    node: NodeMetricsCollector;
    agent: AgentMetricsCollector;
    event: EventMetricsCollector | null;
  } {
    return {
      workflow: this.workflowMetrics,
      node: this.nodeMetrics,
      agent: this.agentMetrics,
      event: this.eventCollector,
    };
  }

  /**
   * Generate comprehensive report from all collectors
   */
  async generateReport(options?: {
    timeRange?: { from: number; to: number };
    includeTrends?: boolean;
  }): Promise<MetricReport> {
    const timestamp = Date.now();

    // Gather summary statistics
    const workflowStats = this.workflowMetrics.getWorkflowUsageStats();
    const agentStats = this.agentMetrics.getAgentStats();
    const eventSummary = this.eventCollector?.generateSummary();

    const report: MetricReport = {
      timestamp,
      summary: {
        totalMetrics: this.getTotalMetricCount(),
        byType: {
          counter: 0,
          gauge: 0,
          histogram: 0,
          summary: 0,
        },
        byCategory: {
          workflow: workflowStats.totalExecutions,
          agent: agentStats.totalExecutions,
          event: eventSummary?.totalEvents || 0,
        },
      },
      topMetrics: [
        ...this.workflowMetrics.getTopWorkflows(5).map(wf => ({
          metricName: 'workflow.execution.count',
          value: wf.executionCount,
          labels: { workflow_id: wf.workflowId },
        })),
        ...this.nodeMetrics.getTopNodeTemplates(5).map(nt => ({
          metricName: 'node.template.instantiation.count',
          value: nt.instantiationCount,
          labels: { template_name: nt.templateName },
        })),
      ],
      anomalies: [],
    };

    // Notify subscribers
    for (const subscriber of this.reportSubscribers) {
      try {
        await subscriber(report);
      } catch (error) {
        logger.error("Error in report subscriber", { error });
      }
    }

    return report;
  }

  /**
   * Subscribe to periodic reports
   */
  onReport(callback: (report: MetricReport) => void | Promise<void>): () => void {
    this.reportSubscribers.push(callback);
    return () => {
      const index = this.reportSubscribers.indexOf(callback);
      if (index > -1) {
        this.reportSubscribers.splice(index, 1);
      }
    };
  }

  /**
   * Flush all metrics
   */
  async flushAll(): Promise<void> {
    logger.debug("Flushing all metrics collectors");
    
    await Promise.all([
      this.workflowMetrics.flush(),
      this.nodeMetrics.flush(),
      this.agentMetrics.flush(),
      this.eventCollector?.flush(),
    ].filter(Boolean));
  }

  /**
   * Dispose all collectors and stop periodic reporting
   */
  dispose(): void {
    logger.info("Disposing Unified Metrics Manager");
    
    if (this.reportingTimer) {
      clearInterval(this.reportingTimer);
      this.reportingTimer = null;
    }

    this.workflowMetrics.dispose();
    this.nodeMetrics.dispose();
    this.agentMetrics.dispose();
    this.reportSubscribers = [];
  }

  /**
   * Setup periodic reporting
   */
  private setupPeriodicReporting(interval: number): void {
    this.reportingTimer = setInterval(async () => {
      try {
        await this.generateReport();
      } catch (error) {
        logger.error("Error generating periodic report", { error });
      }
    }, interval);

    logger.info("Periodic reporting enabled", { interval });
  }

  /**
   * Get total metric count across all collectors
   */
  private getTotalMetricCount(): number {
    // This would query each collector and sum up
    return 0; // Placeholder
  }
}
```

## Phase 2: 集成到执行流程

### Step 2.1: 更新 Service Identifiers

**文件**: `sdk/core/di/service-identifiers.ts`

添加新的 identifier:

```typescript
// Add to existing identifiers
export const MetricsManager = Symbol('MetricsManager');
```

### Step 2.2: 注册到 GlobalContext

**文件**: `sdk/core/global-context.ts`

在构造函数中初始化 MetricsManager:

```typescript
import { UnifiedMetricsManager } from "./metrics/unified-metrics-manager.js";

// In constructor or initialization method
this.container.register(Identifiers.MetricsManager, {
  useValue: new UnifiedMetricsManager(this, {
    enablePeriodicReporting: true,
    reportingInterval: 60000,
  }),
});
```

### Step 2.3: 集成到 Workflow Lifecycle

**文件**: `sdk/workflow/execution/coordinators/workflow-lifecycle-coordinator.ts`

修改 `execute` 方法(参考设计文档中的示例)。

### Step 2.4: 集成到 Node Execution

**文件**: `sdk/workflow/execution/coordinators/node-execution-coordinator.ts`

在 `executeNode` 方法中添加 metrics 记录。

### Step 2.5: 集成到 Agent Loop Executor

**文件**: `sdk/agent/execution/executors/agent-loop-executor.ts`

在 `execute` 方法中添加 metrics 记录。

## Phase 3: API 层实现

### Step 3.1: Metrics Resource API

**文件**: `sdk/api/shared/resources/metrics/metrics-resource-api.ts`

创建完整的 API 类(参考设计文档)。

### Step 3.2: 添加到 APIDependencyManager

**文件**: `sdk/api/shared/core/sdk-dependencies.ts`

```typescript
// Add getter method
getMetricsManager(): UnifiedMetricsManager {
  return this.globalContext.container.get(Identifiers.MetricsManager);
}
```

## Phase 4: CLI Commands

### Step 4.1: 创建 Metrics Command

**文件**: `apps/cli-app/src/commands/metrics/index.ts`

```typescript
import { Command } from "commander";
import { MetricsAdapter } from "../../adapters/metrics-adapter.js";
import { handleError } from "../../utils/error-handler.js";
import * as output from "../../utils/output.js";

export function createMetricsCommand(): Command {
  const metricsCmd = new Command("metrics").description("View and export metrics");

  // Workflow metrics
  metricsCmd
    .command("workflow")
    .description("View workflow execution metrics")
    .option("--workflow-id <id>", "Filter by workflow ID")
    .option("--top <n>", "Show top N workflows", "10")
    .action(async (options) => {
      try {
        const adapter = new MetricsAdapter();
        
        if (options.workflowId) {
          const stats = await adapter.getWorkflowMetrics({ workflowId: options.workflowId });
          output.output(JSON.stringify(stats, null, 2));
        } else {
          const topWorkflows = await adapter.getTopWorkflows(parseInt(options.top));
          output.output("Top Workflows by Execution Count:");
          topWorkflows.forEach((wf, i) => {
            output.output(`  ${i + 1}. ${wf.workflowId}: ${wf.executionCount} executions (${(wf.successRate * 100).toFixed(1)}% success)`);
          });
        }
      } catch (error) {
        handleError(error, { operation: "getWorkflowMetrics" });
      }
    });

  // Node template metrics
  metricsCmd
    .command("node-templates")
    .description("View node template usage metrics")
    .option("--type <type>", "Filter by node type")
    .option("--top <n>", "Show top N templates", "10")
    .action(async (options) => {
      try {
        const adapter = new MetricsAdapter();
        const topTemplates = await adapter.getTopNodeTemplates(parseInt(options.top));
        
        output.output("Top Node Templates by Instantiation Count:");
        topTemplates.forEach((nt, i) => {
          output.output(`  ${i + 1}. ${nt.templateName} (${nt.nodeType}): ${nt.instantiationCount}`);
        });
      } catch (error) {
        handleError(error, { operation: "getNodeTemplateMetrics" });
      }
    });

  // Agent metrics
  metricsCmd
    .command("agents")
    .description("View agent loop metrics")
    .option("--profile <id>", "Filter by profile ID")
    .action(async (options) => {
      try {
        const adapter = new MetricsAdapter();
        const stats = await adapter.getAgentMetrics({ profileId: options.profile });
        output.output(JSON.stringify(stats, null, 2));
      } catch (error) {
        handleError(error, { operation: "getAgentMetrics" });
      }
    });

  // Export metrics
  metricsCmd
    .command("export")
    .description("Export metrics to file")
    .requiredOption("--format <format>", "Export format (json|prometheus)")
    .requiredOption("--output <file>", "Output file path")
    .action(async (options) => {
      try {
        const adapter = new MetricsAdapter();
        const content = await adapter.exportMetrics(options.format);
        
        const fs = await import("fs/promises");
        await fs.writeFile(options.output, content, "utf-8");
        output.infoLog(`Metrics exported to ${options.output}`);
      } catch (error) {
        handleError(error, { operation: "exportMetrics" });
      }
    });

  return metricsCmd;
}
```

### Step 4.2: Metrics Adapter

**文件**: `apps/cli-app/src/adapters/metrics-adapter.ts`

```typescript
import { BaseAdapter } from "./base-adapter.js";

export class MetricsAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  async getWorkflowMetrics(options?: { workflowId?: string }): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.metrics;
      return api.getWorkflowMetrics(options);
    }, "Get workflow metrics");
  }

  async getTopWorkflows(limit: number = 10): Promise<any[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.metrics;
      return api.getTopWorkflows(limit);
    }, "Get top workflows");
  }

  async getTopNodeTemplates(limit: number = 10): Promise<any[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.metrics;
      return api.getNodeTemplateMetrics();
    }, "Get node template metrics");
  }

  async getAgentMetrics(options?: { profileId?: string }): Promise<any> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.metrics;
      return api.getAgentMetrics(options);
    }, "Get agent metrics");
  }

  async exportMetrics(format: "json" | "prometheus"): Promise<string> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.metrics;
      if (format === "prometheus") {
        return api.exportPrometheus();
      } else {
        const report = await api.generateReport();
        return JSON.stringify(report, null, 2);
      }
    }, "Export metrics");
  }
}
```

## Phase 5: 测试

### Step 5.1: 单元测试

为每个 Collector 创建测试文件:
- `sdk/core/metrics/__tests__/workflow-metrics-collector.test.ts`
- `sdk/core/metrics/__tests__/node-metrics-collector.test.ts`
- `sdk/core/metrics/__tests__/agent-metrics-collector.test.ts`

### Step 5.2: 集成测试

创建端到端测试验证整个流程:
- `sdk/__tests__/integration/metrics-integration.test.ts`

## 后续步骤

完成上述实施后:

1. ✅ 运行测试确保所有功能正常
2. ✅ 更新文档说明如何使用 Metrics API
3. ✅ 添加示例代码展示常见用法
4. ✅ 考虑添加 Prometheus exporter
5. ✅ 规划持久化存储方案

## 总结

本实施计划提供了详细的代码实现指南,按照以下步骤执行:

1. **Phase 1**: 实现三个核心 Collectors 和 Unified Manager
2. **Phase 2**: 集成到执行流程的关键节点
3. **Phase 3**: 创建 API 层暴露查询接口
4. **Phase 4**: 添加 CLI 命令方便使用
5. **Phase 5**: 编写测试确保质量

预计总工时: 4-6 周(包含测试和优化)
