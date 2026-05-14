# Metrics 系统 - 快速开始指南

## 概述

Metrics 系统用于收集和分析 Workflow、Node Template 和 Agent Loop 的使用情况,帮助识别高频使用的组件和优化性能瓶颈。

## 核心功能

✅ **Workflow 调用追踪**: 统计每个工作流的执行次数、成功率、执行时长  
✅ **Node 模板分析**: 追踪节点模板的实例化和使用频率  
✅ **Agent Loop 监控**: 记录 Agent 配置的调用情况和迭代统计  
✅ **多维度查询**: 按 ID、类型、版本等维度聚合统计  
✅ **实时指标**: 基于事件驱动,低开销收集  

## 快速使用

### 1. 查询高频工作流

```typescript
import { SDK } from "@wf-agent/sdk";

const sdk = new SDK();

// 获取最常用的前10个工作流
const topWorkflows = await sdk.metrics.getTopWorkflows(10);

console.log("Most used workflows:");
topWorkflows.forEach((wf, index) => {
  console.log(`${index + 1}. ${wf.workflowId}`);
  console.log(`   Executions: ${wf.executionCount}`);
  console.log(`   Success Rate: ${(wf.successRate * 100).toFixed(1)}%`);
});
```

### 2. 查看特定工作流详情

```typescript
// 获取工作流的详细统计
const stats = await sdk.metrics.getWorkflowMetrics({
  workflowId: "code-review-v2"
});

console.log(`Total Executions: ${stats.totalExecutions}`);
console.log(`Success Rate: ${(stats.successRate * 100).toFixed(1)}%`);
console.log(`Average Duration: ${stats.avgDuration}ms`);
console.log(`P95 Duration: ${stats.p95Duration}ms`);
console.log(`P99 Duration: ${stats.p99Duration}ms`);

// 按版本分组
console.log("\nBy Version:");
Object.entries(stats.byVersion).forEach(([version, count]) => {
  console.log(`  v${version}: ${count} executions`);
});
```

### 3. 分析 Node 模板使用情况

```typescript
// 获取最常用的节点模板
const topTemplates = await sdk.metrics.getTopNodeTemplates(10);

console.log("Most instantiated node templates:");
topTemplates.forEach((nt, index) => {
  console.log(`${index + 1}. ${nt.templateName} (${nt.nodeType})`);
  console.log(`   Instantiations: ${nt.instantiationCount}`);
});

// 按节点类型统计
const statsByType = await sdk.metrics.getNodeExecutionStatsByType();
console.log("\nExecution stats by node type:");
Object.entries(statsByType).forEach(([type, stats]) => {
  console.log(`${type}:`);
  console.log(`  Total: ${stats.totalCount}`);
  console.log(`  Success Rate: ${(stats.successRate * 100).toFixed(1)}%`);
});
```

### 4. 监控 Agent Loop

```typescript
// 获取 Agent 使用统计
const agentStats = await sdk.metrics.getAgentMetrics();

console.log(`Total Agent Executions: ${agentStats.totalExecutions}`);
console.log(`Average Iterations: ${agentStats.avgIterations.toFixed(1)}`);
console.log(`Average Tool Calls: ${agentStats.avgToolCalls.toFixed(1)}`);

console.log("\nBy Profile:");
Object.entries(agentStats.byProfile).forEach(([profileId, count]) => {
  console.log(`  ${profileId}: ${count} executions`);
});
```

## CLI 命令

### 查看工作流指标

```bash
# 查看最常用的工作流
wf-agent metrics workflow --top 10

# 查看特定工作流详情
wf-agent metrics workflow --workflow-id code-review-v2
```

输出示例:
```
Top Workflows by Execution Count:
  1. code-review-v2: 456 executions (95.2% success)
  2. data-processing: 234 executions (98.1% success)
  3. deployment-pipeline: 189 executions (92.6% success)
```

### 查看 Node 模板指标

```bash
# 查看最常用的节点模板
wf-agent metrics node-templates --top 10

# 按类型过滤
wf-agent metrics node-templates --type LLM
```

输出示例:
```
Top Node Templates by Instantiation Count:
  1. code-analyzer (LLM): 1234
  2. test-runner (SCRIPT): 890
  3. file-processor (TOOL): 567
```

### 查看 Agent 指标

```bash
# 查看所有 Agent 统计
wf-agent metrics agents

# 查看特定 profile
wf-agent metrics agents --profile senior-developer
```

### 导出指标

```bash
# 导出为 JSON
wf-agent metrics export --format json --output metrics.json

# 导出为 Prometheus 格式
wf-agent metrics export --format prometheus --output metrics.prom
```

## API 参考

### MetricsResourceAPI

```typescript
interface MetricsResourceAPI {
  // Workflow metrics
  getWorkflowMetrics(options?: {
    workflowId?: string;
    timeRange?: { from: number; to: number };
  }): Promise<WorkflowMetrics>;
  
  getTopWorkflows(limit?: number): Promise<WorkflowSummary[]>;
  
  // Node template metrics
  getNodeTemplateMetrics(options?: {
    templateName?: string;
    nodeType?: string;
  }): Promise<NodeTemplateMetrics>;
  
  getTopNodeTemplates(limit?: number): Promise<NodeTemplateSummary[]>;
  
  getNodeExecutionStatsByType(): Promise<Record<string, NodeExecutionStats>>;
  
  // Agent metrics
  getAgentMetrics(options?: {
    profileId?: string;
  }): Promise<AgentMetrics>;
  
  // Export
  exportPrometheus(): Promise<string>;
  generateReport(options?: ReportOptions): Promise<MetricReport>;
}
```

### 数据类型

```typescript
interface WorkflowMetrics {
  totalExecutions: number;
  successRate: number;
  avgDuration: number;
  p95Duration: number;
  p99Duration: number;
  byVersion: Record<string, number>;
}

interface WorkflowSummary {
  workflowId: string;
  workflowName?: string;
  executionCount: number;
  successRate: number;
}

interface NodeTemplateSummary {
  templateName: string;
  nodeType: string;
  instantiationCount: number;
}

interface NodeExecutionStats {
  totalCount: number;
  successRate: number;
  avgDuration: number;
}

interface AgentMetrics {
  totalExecutions: number;
  avgIterations: number;
  avgToolCalls: number;
  byProfile: Record<string, number>;
}
```

## 架构说明

### 组件层次

```
┌─────────────────────────────────────┐
│      Metrics Resource API           │  ← 对外接口
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│     Unified Metrics Manager         │  ← 统一管理
└──┬──────────┬──────────┬───────────┘
   │          │          │
┌──▼───┐  ┌──▼────┐  ┌─▼────────┐
│Workflow│  │ Node  │  │  Agent   │  ← 专用 Collectors
│Metrics │  │Metrics│  │ Metrics  │
└────────┘  └───────┘  └──────────┘
```

### 数据流

```
执行事件 → EventRegistry → Metrics Collector → Buffer → Query/Export
   ↑                                              ↓
   └──────────────────────────────────────────────┘
                    (定期 Flush)
```

## 关键指标说明

### Workflow 指标

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `workflow.execution.count` | Counter | 执行次数(按 workflow_id, version, execution_type) |
| `workflow.execution.success.count` | Counter | 成功次数 |
| `workflow.execution.failure.count` | Counter | 失败次数(按 error_type) |
| `workflow.execution.duration` | Histogram | 执行时长分布 |
| `workflow.execution.node_count` | Histogram | 节点数量分布 |
| `workflow.execution.active.count` | Gauge | 当前活跃执行数 |

### Node 指标

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `node.template.instantiation.count` | Counter | 模板实例化次数 |
| `node.execution.count` | Counter | 节点执行次数 |
| `node.execution.success.count` | Counter | 成功次数 |
| `node.execution.failure.count` | Counter | 失败次数 |
| `node.execution.duration` | Histogram | 执行时长分布 |
| `node.execution.token_usage` | Histogram | Token 使用量(LLM 节点) |

### Agent 指标

| 指标名称 | 类型 | 说明 |
|---------|------|------|
| `agent.loop.execution.count` | Counter | 执行次数(按 profile_id) |
| `agent.loop.iteration.count` | Counter | 迭代总次数 |
| `agent.loop.duration` | Histogram | 执行时长分布 |
| `agent.loop.iterations_per_execution` | Histogram | 每次执行的迭代数 |
| `agent.loop.tokens_per_iteration` | Histogram | 每次迭代的 Token 数 |
| `agent.tool.call.count` | Counter | Tool 调用次数 |

## 配置选项

### 初始化配置

```typescript
const sdk = new SDK({
  metrics: {
    enablePeriodicReporting: true,  // 启用定期报告
    reportingInterval: 60000,       // 报告间隔(毫秒)
    bufferSize: 100,                // 缓冲区大小
    flushInterval: 5000,            // Flush 间隔
    maxAge: 3600000,                // 数据保留时间(1小时)
  }
});
```

### 禁用 Metrics

如果不需要 metrics 收集,可以完全禁用:

```typescript
const sdk = new SDK({
  metrics: {
    enabled: false,  // 禁用 metrics 系统
  }
});
```

## 性能考虑

### 开销评估

- **CPU 开销**: < 1% (异步记录,不阻塞主流程)
- **内存占用**: ~10MB (默认 buffer 100条,可配置)
- **I/O 影响**: 最小(批量 flush,非同步写入)

### 优化建议

1. **生产环境**: 启用持久化存储,避免重启丢失数据
2. **高负载场景**: 增大 buffer size,减少 flush 频率
3. **长期运行**: 配置数据清理策略,控制内存增长

## 常见问题

### Q: Metrics 会影响执行性能吗?

A: 影响极小。所有 metrics 记录都是异步的,不会阻塞主执行流程。实际测试显示 CPU 开销 < 1%。

### Q: 数据会持久化保存吗?

A: 当前版本数据存储在内存中,重启后会丢失。后续版本将支持 SQLite 和其他持久化后端。

### Q: 如何清理旧数据?

A: 可以通过配置 `maxAge` 自动清理,或手动调用 `flush()` 清空 buffer。

### Q: 支持自定义指标吗?

A: 是的,可以直接使用 `MetricCollector` API 记录自定义指标:

```typescript
const collector = sdk.metrics.getCollectors().workflow;
collector.incrementCounter('my.custom.metric', { label: 'value' });
```

### Q: 如何集成到监控系统?

A: 可以使用 `exportPrometheus()` 方法导出 Prometheus 格式的指标,或订阅定期报告:

```typescript
sdk.metrics.onReport((report) => {
  // 发送到你的监控系统
  sendToMonitoringSystem(report);
});
```

## 下一步

- 📖 阅读 [完整设计文档](../architecture/metrics-design.md)
- 🔧 查看 [实施计划](../plan/metrics-implementation-plan.md)
- 💡 学习 [高级用法](./metrics-advanced.md) (待创建)

## 反馈

如有问题或建议,请提交 Issue 或联系开发团队。
