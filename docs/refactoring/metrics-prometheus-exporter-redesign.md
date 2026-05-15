# Metrics Prometheus 导出系统 - 新设计方案

## 🎯 问题分析

### 当前实现的致命缺陷

1. **硬编码导出逻辑**
   ```typescript
   // ❌ 糟糕的设计 - 在 API 层手动遍历所有 collector
   lines.push(`workflow_execution_total ${workflowStats.totalExecutions}`);
   lines.push(`node_execution_total{node_type="${nodeType}"} ${stats.totalCount}`);
   ```
   
2. **缺乏可扩展性**
   - 每添加一个新指标类型，都需要修改 `exportAsPrometheus()` 方法
   - Collector 和导出逻辑紧耦合
   - 无法支持自定义指标

3. **违反单一职责原则**
   - API 层不应该知道如何格式化每个 Collector 的指标
   - Collector 应该自己负责导出自己的指标

4. **代码复用性差**
   - 标签格式化逻辑分散
   - 没有统一的导出接口
   - 每个 Collector 的统计方法返回格式不一致

---

## 💡 新设计方案

### 核心思想：**策略模式 + 访问者模式**

```
┌─────────────────────────────────────────────────┐
│           MetricsResourceAPI                     │
│  (协调导出流程，不关心具体格式)                    │
└──────────────┬──────────────────────────────────┘
               │
               │ export(format: "prometheus")
               ▼
┌─────────────────────────────────────────────────┐
│        PrometheusExporter (策略)                 │
│  - 遍历所有 Collector                            │
│  - 调用每个 Collector 的 toPrometheus()          │
│  - 组装最终的输出                                │
└──────────────┬──────────────────────────────────┘
               │
    ┌──────────┼──────────┬──────────┐
    │          │          │          │
    ▼          ▼          ▼          ▼
┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐
│Workflow│ │ Node   │ │ Agent  │ │ Event  │
│Collector│ │Collector│ │Collector│ │Collector│
│        │ │        │ │        │ │        │
│toProm- │ │toProm- │ │toProm- │ │toProm- │
│etheus()│ │etheus()│ │etheus()│ │etheus()│
└────────┘ └────────┘ └────────┘ └────────┘
```

### 设计优势

1. ✅ **开闭原则** - 新增 Collector 无需修改导出逻辑
2. ✅ **单一职责** - 每个 Collector 负责自己的导出格式
3. ✅ **可测试性** - 可以单独测试每个 Collector 的导出
4. ✅ **可扩展性** - 轻松支持其他导出格式（JSON, OpenMetrics 等）

---

## 📐 详细设计

### 1. 定义导出接口

#### 1.1 MetricExporter 接口

```typescript
// sdk/core/metrics/types.ts

/**
 * Metric exporter interface
 * Each collector can implement this to support multiple export formats
 */
export interface MetricExporter {
  /**
   * Export metrics in Prometheus exposition format
   * @returns Array of formatted metric lines (without trailing newline)
   */
  toPrometheus(): string[];
  
  /**
   * Export metrics as JSON
   * @returns JSON-serializable object
   */
  toJSON(): Record<string, unknown>;
}
```

#### 1.2 扩展 MetricCollector 接口

```typescript
// sdk/core/metrics/types.ts

export interface MetricCollector extends MetricExporter {
  // ... existing methods ...
}
```

---

### 2. 实现 Prometheus 格式化工具

#### 2.1 PrometheusFormatter 工具类

```typescript
// sdk/core/metrics/utils/prometheus-formatter.ts

/**
 * Prometheus metric type
 */
export type PrometheusMetricType = 
  | 'counter'
  | 'gauge'
  | 'histogram'
  | 'summary';

/**
 * Prometheus metric definition
 */
export interface PrometheusMetric {
  /** Metric name (snake_case) */
  name: string;
  /** Metric type */
  type: PrometheusMetricType;
  /** Help text */
  help: string;
  /** Labels and value */
  samples: PrometheusSample[];
}

/**
 * Prometheus sample (a single metric line)
 */
export interface PrometheusSample {
  /** Label key-value pairs */
  labels?: Record<string, string>;
  /** Metric value */
  value: number;
  /** Optional timestamp (Unix milliseconds) */
  timestamp?: number;
}

/**
 * Utility class for formatting metrics in Prometheus exposition format
 */
export class PrometheusFormatter {
  /**
   * Format a complete Prometheus metric with HELP, TYPE, and samples
   */
  static formatMetric(metric: PrometheusMetric): string[] {
    const lines: string[] = [];
    
    // Add HELP comment
    lines.push(`# HELP ${metric.name} ${metric.help}`);
    
    // Add TYPE declaration
    lines.push(`# TYPE ${metric.name} ${metric.type}`);
    
    // Add samples
    for (const sample of metric.samples) {
      lines.push(this.formatSample(metric.name, sample));
    }
    
    return lines;
  }
  
  /**
   * Format a single metric sample
   */
  static formatSample(name: string, sample: PrometheusSample): string {
    let line = name;
    
    // Add labels if present
    if (sample.labels && Object.keys(sample.labels).length > 0) {
      const labelStr = this.formatLabels(sample.labels);
      line += labelStr;
    }
    
    // Add value
    line += ` ${sample.value}`;
    
    // Add timestamp if present
    if (sample.timestamp) {
      line += ` ${sample.timestamp}`;
    }
    
    return line;
  }
  
  /**
   * Format labels in Prometheus format: {label1="value1",label2="value2"}
   */
  static formatLabels(labels: Record<string, string>): string {
    if (!labels || Object.keys(labels).length === 0) {
      return '';
    }
    
    const parts = Object.entries(labels)
      .map(([key, value]) => `${key}="${this.escapeLabelValue(value)}"`)
      .join(',');
    
    return `{${parts}}`;
  }
  
  /**
   * Escape special characters in label values
   */
  private static escapeLabelValue(value: string): string {
    return value
      .replace(/\\/g, '\\\\')
      .replace(/"/g, '\\"')
      .replace(/\n/g, '\\n');
  }
  
  /**
   * Combine multiple metrics into final output
   */
  static combine(metrics: string[][], addTimestamp: boolean = true): string {
    const allLines: string[] = [];
    
    for (const metricLines of metrics) {
      allLines.push(...metricLines);
    }
    
    // Add generation timestamp
    if (addTimestamp) {
      allLines.push(`# Generated at ${new Date().toISOString()}`);
    }
    
    return allLines.join('\n') + '\n';
  }
}
```

---

### 3. 重构 Collector 实现

#### 3.1 WorkflowMetricsCollector 示例

```typescript
// sdk/core/metrics/workflow-metrics-collector.ts

import { BaseMetricCollector } from "./base-collector.js";
import { PrometheusFormatter, type PrometheusMetric } from "./utils/prometheus-formatter.js";

export class WorkflowMetricsCollector extends BaseMetricCollector {
  // ... existing methods ...
  
  /**
   * Export workflow metrics in Prometheus format
   */
  toPrometheus(): string[] {
    const stats = this.getWorkflowUsageStats();
    const metrics: PrometheusMetric[] = [];
    
    // Total executions counter
    metrics.push({
      name: 'workflow_execution_total',
      type: 'counter',
      help: 'Total workflow executions',
      samples: [{ value: stats.totalExecutions }]
    });
    
    // Success rate gauge
    metrics.push({
      name: 'workflow_execution_success_rate',
      type: 'gauge',
      help: 'Workflow execution success rate (0-1)',
      samples: [{ value: stats.successRate }]
    });
    
    // Duration summary with quantiles
    metrics.push({
      name: 'workflow_execution_duration_seconds',
      type: 'summary',
      help: 'Workflow execution duration in seconds',
      samples: [
        { labels: { quantile: '0.5' }, value: stats.avgDuration / 1000 },
        { labels: { quantile: '0.95' }, value: stats.p95Duration / 1000 },
        { labels: { quantile: '0.99' }, value: stats.p99Duration / 1000 },
      ]
    });
    
    // Executions by version
    for (const [version, count] of Object.entries(stats.byVersion)) {
      metrics.push({
        name: 'workflow_execution_by_version_total',
        type: 'counter',
        help: 'Workflow executions grouped by version',
        samples: [{ labels: { version }, value: count }]
      });
    }
    
    // Format all metrics
    return metrics.flatMap(m => PrometheusFormatter.formatMetric(m));
  }
  
  /**
   * Export as JSON
   */
  toJSON(): Record<string, unknown> {
    return {
      type: 'workflow',
      stats: this.getWorkflowUsageStats(),
      topWorkflows: this.getTopWorkflows(10)
    };
  }
}
```

#### 3.2 NodeMetricsCollector 示例

```typescript
// sdk/core/metrics/node-metrics-collector.ts

export class NodeMetricsCollector extends BaseMetricCollector {
  // ... existing methods ...
  
  toPrometheus(): string[] {
    const metrics: PrometheusMetric[] = [];
    
    // Node execution stats by type
    const nodeStats = this.getNodeExecutionStatsByType();
    for (const [nodeType, stats] of Object.entries(nodeStats)) {
      metrics.push({
        name: 'node_execution_total',
        type: 'counter',
        help: 'Total node executions by type',
        samples: [{ labels: { node_type: nodeType }, value: stats.totalCount }]
      });
      
      metrics.push({
        name: 'node_execution_success_rate',
        type: 'gauge',
        help: 'Node execution success rate by type',
        samples: [{ labels: { node_type: nodeType }, value: stats.successRate }]
      });
    }
    
    // Top templates
    const topTemplates = this.getTopNodeTemplates(10);
    for (const template of topTemplates) {
      metrics.push({
        name: 'node_template_instantiation_total',
        type: 'counter',
        help: 'Node template instantiation count',
        samples: [{
          labels: {
            template_name: template.templateName,
            node_type: template.nodeType
          },
          value: template.instantiationCount
        }]
      });
    }
    
    return metrics.flatMap(m => PrometheusFormatter.formatMetric(m));
  }
  
  toJSON(): Record<string, unknown> {
    return {
      type: 'node',
      statsByType: this.getNodeExecutionStatsByType(),
      topTemplates: this.getTopNodeTemplates(10)
    };
  }
}
```

#### 3.3 AgentMetricsCollector 示例

```typescript
// sdk/core/metrics/agent-metrics-collector.ts

export class AgentMetricsCollector extends BaseMetricCollector {
  // ... existing methods ...
  
  toPrometheus(): string[] {
    const stats = this.getAgentStats();
    const metrics: PrometheusMetric[] = [];
    
    // Total executions
    metrics.push({
      name: 'agent_loop_execution_total',
      type: 'counter',
      help: 'Total agent loop executions',
      samples: [{ value: stats.totalExecutions }]
    });
    
    // Average iterations
    metrics.push({
      name: 'agent_loop_iterations_avg',
      type: 'gauge',
      help: 'Average iterations per agent loop',
      samples: [{ value: stats.avgIterations }]
    });
    
    // By profile
    for (const [profileId, count] of Object.entries(stats.byProfile)) {
      metrics.push({
        name: 'agent_loop_execution_by_profile_total',
        type: 'counter',
        help: 'Agent loop executions by profile',
        samples: [{ labels: { profile_id: profileId }, value: count }]
      });
    }
    
    return metrics.flatMap(m => PrometheusFormatter.formatMetric(m));
  }
  
  toJSON(): Record<string, unknown> {
    return {
      type: 'agent',
      stats: this.getAgentStats()
    };
  }
}
```

---

### 4. 重构 MetricsResourceAPI

```typescript
// sdk/api/shared/resources/metrics/metrics-resource-api.ts

import { PrometheusFormatter } from "../../../../core/metrics/utils/prometheus-formatter.js";

export class MetricsResourceAPI {
  private metricsRegistry: MetricsRegistry;

  constructor(dependencies: APIDependencyManager) {
    this.metricsRegistry = dependencies.getGlobalContext().metricsRegistry;
    logger.info("MetricsResourceAPI initialized");
  }

  /**
   * Export metrics in specified format
   */
  async exportMetrics(format: MetricsExportFormat): Promise<string> {
    logger.debug("Exporting metrics", { format });
    
    switch (format) {
      case "json":
        return await this.exportAsJSON();
      case "prometheus":
        return await this.exportAsPrometheus();
      default:
        throw new Error(`Unsupported export format: ${format}`);
    }
  }

  /**
   * Export as JSON
   */
  private async exportAsJSON(): Promise<string> {
    const collectors = this.metricsRegistry.getCollectors();
    
    const result = {
      timestamp: Date.now(),
      workflow: collectors.workflow.toJSON(),
      node: collectors.node.toJSON(),
      agent: collectors.agent.toJSON(),
      event: collectors.event?.toJSON() || null,
    };
    
    return JSON.stringify(result, null, 2);
  }

  /**
   * Export in Prometheus format
   * 
   * This is now MUCH simpler - just delegate to each collector!
   */
  private async exportAsPrometheus(): Promise<string> {
    const collectors = this.metricsRegistry.getCollectors();
    
    // Each collector exports its own metrics
    const allMetrics: string[][] = [
      collectors.workflow.toPrometheus(),
      collectors.node.toPrometheus(),
      collectors.agent.toPrometheus(),
    ];
    
    // Add event metrics if available
    if (collectors.event) {
      try {
        allMetrics.push(collectors.event.toPrometheus());
      } catch (error) {
        logger.warn("Failed to export event metrics", { error });
      }
    }
    
    // Combine all metrics with proper formatting
    return PrometheusFormatter.combine(allMetrics);
  }
}
```

---

## 🎨 设计对比

### Before (旧方案)

```typescript
// ❌ 硬编码、难以维护
private async exportAsPrometheus(): Promise<string> {
  const lines: string[] = [];
  
  // Manually format each metric
  const workflowStats = collectors.workflow.getWorkflowUsageStats();
  lines.push(`# HELP workflow_execution_total ...`);
  lines.push(`# TYPE workflow_execution_total counter`);
  lines.push(`workflow_execution_total ${workflowStats.totalExecutions}`);
  
  // Repeat for every metric...
  const nodeStats = collectors.node.getNodeExecutionStatsByType();
  for (const [nodeType, stats] of Object.entries(nodeStats)) {
    lines.push(`node_execution_total{node_type="${nodeType}"} ${stats.totalCount}`);
  }
  
  // And more...
  return lines.join("\n") + "\n";
}
```

**问题**:
- ❌ 100+ 行硬编码
- ❌ 每加一个指标都要改这里
- ❌ 无法复用
- ❌ 容易出错

### After (新方案)

```typescript
// ✅ 简洁、可扩展
private async exportAsPrometheus(): Promise<string> {
  const collectors = this.metricsRegistry.getCollectors();
  
  // Delegate to each collector
  const allMetrics: string[][] = [
    collectors.workflow.toPrometheus(),
    collectors.node.toPrometheus(),
    collectors.agent.toPrometheus(),
  ];
  
  return PrometheusFormatter.combine(allMetrics);
}
```

**优势**:
- ✅ 仅 10 行核心代码
- ✅ 新增 Collector 自动支持
- ✅ 完全解耦
- ✅ 易于测试

---

## 📦 文件结构

```
sdk/core/metrics/
├── types.ts                          # 添加 MetricExporter 接口
├── utils/
│   ├── prometheus-formatter.ts       # ✨ 新增：Prometheus 格式化工具
│   └── index.ts                      # 导出工具
├── workflow-metrics-collector.ts     # 实现 toPrometheus()
├── node-metrics-collector.ts         # 实现 toPrometheus()
├── agent-metrics-collector.ts        # 实现 toPrometheus()
├── event-collector.ts                # 实现 toPrometheus()
└── ...

sdk/api/shared/resources/metrics/
└── metrics-resource-api.ts           # 简化导出逻辑
```

---

## 🧪 测试策略

### 1. 单元测试 PrometheusFormatter

```typescript
// sdk/core/metrics/__tests__/prometheus-formatter.test.ts

describe('PrometheusFormatter', () => {
  it('should format labels correctly', () => {
    const labels = { node_type: 'LLM', status: 'success' };
    const result = PrometheusFormatter.formatLabels(labels);
    expect(result).toBe('{node_type="LLM",status="success"}');
  });
  
  it('should handle empty labels', () => {
    expect(PrometheusFormatter.formatLabels({})).toBe('');
    expect(PrometheusFormatter.formatLabels(undefined as any)).toBe('');
  });
  
  it('should escape special characters', () => {
    const labels = { message: 'value with "quotes" and \\backslash' };
    const result = PrometheusFormatter.formatLabels(labels);
    expect(result).toContain('\\"');
    expect(result).toContain('\\\\');
  });
  
  it('should format complete metric', () => {
    const metric: PrometheusMetric = {
      name: 'test_counter',
      type: 'counter',
      help: 'Test counter',
      samples: [
        { value: 10 },
        { labels: { label1: 'value1' }, value: 20 }
      ]
    };
    
    const lines = PrometheusFormatter.formatMetric(metric);
    expect(lines).toEqual([
      '# HELP test_counter Test counter',
      '# TYPE test_counter counter',
      'test_counter 10',
      'test_counter{label1="value1"} 20'
    ]);
  });
});
```

### 2. 单元测试 Collector 导出

```typescript
// sdk/core/metrics/__tests__/workflow-collector-export.test.ts

describe('WorkflowMetricsCollector.toPrometheus()', () => {
  it('should export workflow metrics', () => {
    const collector = new WorkflowMetricsCollector();
    
    // Record some metrics
    collector.recordExecutionStart('wf-1', 'exec-1', { version: '1.0' });
    collector.recordExecutionComplete('wf-1', 'exec-1', {
      success: true,
      duration: 1000,
      nodeCount: 5
    });
    
    const lines = collector.toPrometheus();
    
    expect(lines).toContain('# HELP workflow_execution_total');
    expect(lines).toContain('# TYPE workflow_execution_total counter');
    expect(lines.some(l => l.includes('workflow_execution_total 1'))).toBe(true);
  });
});
```

### 3. 集成测试

```typescript
// sdk/api/__tests__/metrics-export.int.test.ts

describe('MetricsResourceAPI.exportMetrics()', () => {
  it('should export valid Prometheus format', async () => {
    const api = createMetricsAPI();
    const output = await api.exportMetrics('prometheus');
    
    // Verify format
    expect(output).toContain('# HELP');
    expect(output).toContain('# TYPE');
    expect(output).toContain('# Generated at');
    
    // Verify all collectors contributed
    expect(output).toContain('workflow_execution_total');
    expect(output).toContain('node_execution_total');
    expect(output).toContain('agent_loop_execution_total');
  });
  
  it('should export valid JSON', async () => {
    const api = createMetricsAPI();
    const output = await api.exportMetrics('json');
    
    const parsed = JSON.parse(output);
    expect(parsed).toHaveProperty('workflow');
    expect(parsed).toHaveProperty('node');
    expect(parsed).toHaveProperty('agent');
    expect(parsed).toHaveProperty('timestamp');
  });
});
```

---

## 🚀 实施步骤

### Phase 1: 创建工具类（30 分钟）

1. 创建 `sdk/core/metrics/utils/prometheus-formatter.ts`
2. 实现 `PrometheusFormatter` 类
3. 编写单元测试

### Phase 2: 更新类型定义（15 分钟）

1. 在 `types.ts` 中添加 `MetricExporter` 接口
2. 扩展 `MetricCollector` 接口

### Phase 3: 实现 Collector 导出（2 小时）

1. 为 `WorkflowMetricsCollector` 实现 `toPrometheus()` 和 `toJSON()`
2. 为 `NodeMetricsCollector` 实现导出
3. 为 `AgentMetricsCollector` 实现导出
4. 为 `EventMetricsCollector` 实现导出

### Phase 4: 重构 API 层（30 分钟）

1. 简化 `MetricsResourceAPI.exportAsPrometheus()`
2. 实现 `exportAsJSON()`
3. 删除旧的硬编码逻辑

### Phase 5: 测试与验证（1 小时）

1. 运行所有单元测试
2. 运行集成测试
3. 手动验证 Prometheus 导出格式
4. 性能测试

---

## 📊 收益分析

| 维度 | 旧方案 | 新方案 | 提升 |
|------|--------|--------|------|
| **代码行数** | ~150 行硬编码 | ~30 行核心逻辑 | ⬇️ 80% |
| **可维护性** | 每加指标需改 API | 只需改 Collector | ⬆️ 高 |
| **可测试性** | 难以单独测试 | 每个 Collector 独立测试 | ⬆️ 高 |
| **可扩展性** | 不支持自定义 | 轻松添加新 Collector | ⬆️ 高 |
| **错误率** | 手动拼接易出错 | 工具类保证格式正确 | ⬇️ 低 |
| **代码复用** | 无复用 | Formatter 可复用 | ⬆️ 高 |

---

## 🎯 总结

这个新设计方案通过：

1. **策略模式** - 将导出逻辑委托给每个 Collector
2. **工具类封装** - 统一的 Prometheus 格式化逻辑
3. **接口抽象** - 清晰的契约，易于扩展

彻底解决了旧方案的痛点，使 Metrics 系统真正具备生产级别的可维护性和可扩展性。

是否要我立即开始实施这个新方案？
