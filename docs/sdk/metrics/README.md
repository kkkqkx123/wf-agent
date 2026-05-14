# Metrics 系统文档

## 📊 概述

Metrics 系统为 Workflow、Node Template 和 Agent Loop 提供全面的指标收集和分析能力,帮助你:

- 🔍 **识别高频使用的组件**: 了解哪些工作流和节点模板最受欢迎
- 📈 **监控性能趋势**: 追踪执行时长、成功率等关键指标
- 🎯 **优化资源配置**: 基于使用数据做出更好的架构决策
- ⚠️ **发现潜在问题**: 通过异常检测及时发现失败率上升等问题

## 📚 文档导航

### 入门指南

- [🚀 快速开始](./quick-start.md) - 5分钟上手 Metrics 系统
  - 基本使用方法
  - CLI 命令示例
  - API 参考
  - 常见问题

### 架构设计

- [🏗️ 设计方案](../../architecture/metrics-design.md) - 完整的系统设计文档
  - 设计原则和架构层次
  - 核心组件详细说明
  - 集成点和扩展机制
  - 持久化方案演进路线

### 实施指南

- [🔧 实施计划](../../plan/metrics-implementation-plan.md) - 详细的代码实现指南
  - Phase 1-5 逐步实施步骤
  - 完整代码示例
  - 测试策略
  - 预计工时评估

### 高级主题 (待创建)

- [📖 高级用法](./metrics-advanced.md) - 深入理解和使用
  - 自定义指标
  - 告警配置
  - Prometheus 集成
  - 性能调优

## 🎯 核心功能

### 1. Workflow Metrics

追踪工作流的执行情况:

```typescript
// 获取最常用的工作流
const topWorkflows = await sdk.metrics.getTopWorkflows(10);

// 查看特定工作流详情
const stats = await sdk.metrics.getWorkflowMetrics({
  workflowId: "code-review-v2"
});
// {
//   totalExecutions: 456,
//   successRate: 0.952,
//   avgDuration: 3200,
//   p95Duration: 8500,
//   p99Duration: 15000,
//   byVersion: { "1.0": 123, "2.0": 333 }
// }
```

**关键指标**:
- ✅ 执行次数(按 ID、版本、类型分组)
- ✅ 成功率和失败原因
- ✅ 执行时长分布(P50, P95, P99)
- ✅ 活跃执行数

### 2. Node Template Metrics

分析节点模板的使用情况:

```typescript
// 获取最常用的节点模板
const topTemplates = await sdk.metrics.getTopNodeTemplates(10);

// 按类型统计
const statsByType = await sdk.metrics.getNodeExecutionStatsByType();
```

**关键指标**:
- ✅ 模板实例化次数
- ✅ 节点执行频率(按类型)
- ✅ 执行时长和成功率
- ✅ Token 使用量(LLM 节点)

### 3. Agent Loop Metrics

监控 Agent 配置的调用:

```typescript
// 获取 Agent 统计
const agentStats = await sdk.metrics.getAgentMetrics();
// {
//   totalExecutions: 890,
//   avgIterations: 4.5,
//   avgToolCalls: 12.3,
//   byProfile: {
//     "senior-developer": 456,
//     "code-reviewer": 234,
//     "data-analyst": 200
//   }
// }
```

**关键指标**:
- ✅ 执行次数(按 Profile ID)
- ✅ 迭代次数分布
- ✅ Tool 调用频率
- ✅ Token 使用效率

## 🛠️ 使用方式

### TypeScript API

```typescript
import { SDK } from "@wf-agent/sdk";

const sdk = new SDK();

// 查询指标
const metrics = await sdk.metrics.getWorkflowMetrics();
const report = await sdk.metrics.generateReport();

// 导出
const prometheusData = await sdk.metrics.exportPrometheus();
```

### CLI 命令

```bash
# 查看工作流指标
wf-agent metrics workflow --top 10

# 查看节点模板
wf-agent metrics node-templates --type LLM

# 查看 Agent 统计
wf-agent metrics agents --profile senior-developer

# 导出指标
wf-agent metrics export --format json --output metrics.json
```

## 📊 典型应用场景

### 场景 1: 识别热门工作流

**问题**: 想知道哪些工作流被最频繁使用,以便优化性能和资源分配。

**解决方案**:
```bash
wf-agent metrics workflow --top 20
```

**输出**:
```
Top Workflows by Execution Count:
  1. code-review-v2: 456 executions (95.2% success)
  2. data-processing: 234 executions (98.1% success)
  3. deployment-pipeline: 189 executions (92.6% success)
  ...
```

**行动**: 针对高频工作流进行性能优化,确保稳定性。

### 场景 2: 分析 Node 模板使用情况

**问题**: 想了解哪些节点类型最常用,以便优化模板库。

**解决方案**:
```typescript
const templates = await sdk.metrics.getTopNodeTemplates(10);
const byType = await sdk.metrics.getNodeExecutionStatsByType();
```

**洞察**:
- LLM 节点占 60% 的执行量
- 某些自定义模板很少使用,可以考虑移除
- SCRIPT 节点的平均执行时长较长,需要优化

### 场景 3: 监控 Agent 效率

**问题**: Agent 的迭代次数是否合理?是否需要调整配置?

**解决方案**:
```typescript
const stats = await sdk.metrics.getAgentMetrics();
console.log(`Average iterations: ${stats.avgIterations}`);
```

**优化**:
- 如果平均迭代次数 > 10,可能需要简化任务或增强 prompt
- 如果某些 profile 的 tool call 特别多,检查工具选择逻辑

### 场景 4: 性能瓶颈分析

**问题**: 工作流执行时间过长,需要找出瓶颈。

**解决方案**:
```typescript
const stats = await sdk.metrics.getWorkflowMetrics({
  workflowId: "slow-workflow"
});

console.log(`P95 duration: ${stats.p95Duration}ms`);
console.log(`P99 duration: ${stats.p99Duration}ms`);
```

**分析**:
- 对比不同版本的执行时长
- 识别执行时间异常的节点
- 优化慢查询或外部调用

## 🏗️ 架构概览

```
┌─────────────────────────────────────────┐
│         Metrics Query & Export          │  ← REST API / CLI / Dashboard
└────────────────┬────────────────────────┘
                 │
┌────────────────▼────────────────────────┐
│      Metrics Aggregation & Storage      │  ← SQLite / Time-series DB
└────────────────┬────────────────────────┘
                 │
┌────────────────▼────────────────────────┐
│       Universal Metrics Collectors      │  ← Workflow/Node/Agent Collectors
└────────────────┬────────────────────────┘
                 │
┌────────────────▼────────────────────────┐
│        Event-Driven Collection          │  ← Listen to lifecycle events
└────────────────┬────────────────────────┘
                 │
┌────────────────▼────────────────────────┐
│         Core Execution Engine           │  ← Workflow/Node/Agent Executors
└─────────────────────────────────────────┘
```

## 📈 关键指标说明

### Counter 指标 (累计计数)

- `workflow.execution.count`: 工作流执行次数
- `node.template.instantiation.count`: 节点模板实例化次数
- `agent.loop.execution.count`: Agent 执行次数

### Histogram 指标 (分布统计)

- `workflow.execution.duration`: 工作流执行时长
- `node.execution.duration`: 节点执行时长
- `agent.loop.iterations_per_execution`: 每次执行的迭代数

### Gauge 指标 (实时状态)

- `workflow.execution.active.count`: 当前活跃的工作流执行数

## 🔧 配置选项

```typescript
const sdk = new SDK({
  metrics: {
    enablePeriodicReporting: true,  // 启用定期报告
    reportingInterval: 60000,       // 报告间隔(毫秒)
    bufferSize: 100,                // 缓冲区大小
    flushInterval: 5000,            // Flush 间隔
    maxAge: 3600000,                // 数据保留时间
  }
});
```

## 🚀 实施路线图

### Phase 1: 基础实现 ✅ (已完成设计)
- [ ] Workflow Metrics Collector
- [ ] Node Metrics Collector
- [ ] Agent Metrics Collector
- [ ] Unified Metrics Manager

### Phase 2: API 和工具 ⏳
- [ ] Metrics Resource API
- [ ] CLI Commands
- [ ] SDK Integration

### Phase 3: 持久化 📋
- [ ] SQLite Storage Adapter
- [ ] Data Cleanup Strategy
- [ ] Performance Optimization

### Phase 4: 高级功能 📋
- [ ] Prometheus Exporter
- [ ] Dashboard Integration
- [ ] Alerting System
- [ ] Trend Analysis

## 💡 最佳实践

### 1. 定期监控关键指标

设置每日/每周报告,关注:
- 工作流成功率变化
- 执行时长趋势
- 异常错误模式

### 2. 基于数据优化

- 优先优化高频使用的工作流
- 移除低使用率的节点模板
- 调整迭代次数过多的 Agent 配置

### 3. 设置告警阈值

```typescript
const ALERT_THRESHOLDS = {
  'workflow.failure.rate': { warning: 0.1, critical: 0.3 },
  'workflow.duration.p95': { warning: 60000, critical: 300000 },
  'agent.iteration.avg': { warning: 8, critical: 15 },
};
```

### 4. 数据清理策略

- 开发环境: 保留最近 1 小时
- 测试环境: 保留最近 24 小时
- 生产环境: 保留最近 7-30 天(取决于存储后端)

## ❓ 常见问题

### Q: Metrics 会影响性能吗?

A: 影响极小(< 1% CPU),所有记录都是异步的,不阻塞主流程。

### Q: 数据会丢失吗?

A: 当前版本存储在内存中,重启后丢失。后续将支持持久化存储。

### Q: 如何自定义指标?

A: 直接使用 Collector API:
```typescript
const collector = sdk.metrics.getCollectors().workflow;
collector.incrementCounter('my.metric', { label: 'value' });
```

### Q: 支持导出到监控系统吗?

A: 支持 Prometheus 格式导出,也可订阅定期报告发送到自定义系统。

## 📖 相关文档

- [Event System Guide](../event-system-guide.md) - 事件系统详解
- [Workflow Execution](../data-flow/workflow-execution.md) - 工作流执行流程
- [Agent Loop Architecture](../architecture/agent-loop-architecture.md) - Agent 架构

## 🤝 贡献

欢迎提交 Issue 和 Pull Request!

## 📄 许可证

[项目许可证信息]

---

**最后更新**: 2026-05-14  
**版本**: v1.0.0 (设计中)
