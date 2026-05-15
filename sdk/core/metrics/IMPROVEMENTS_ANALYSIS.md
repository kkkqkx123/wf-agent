# Metrics System Advanced Improvements Analysis

## 概述

本文档详细分析 metrics 系统的四个高级改进方向，提供实现方案、权衡分析和优先级建议。

---

## 1. 实现真正的 Histogram（累积 Bucket Counts）

### 当前问题

```typescript
// base-collector.ts 当前实现
observeHistogram(metricName: string, value: number, labels?: Record<string, string>): void {
  const metric = {
    metricName,
    metricType: "histogram" as const,
    timestamp: now(),
    labels: labels || {},
    value,
    buckets: [], // ❌ 空数组，无法累积统计
    sum: value,
    count: 1,
  };
  this.record(metric);
}
```

**问题**：
- 每次观察都创建独立的 histogram 记录
- buckets 为空，无法反映值分布
- 查询时无法计算正确的百分位数
- Prometheus 导出时缺少 bucket 信息

### 解决方案 A：基于预定义 Buckets 的累积直方图（推荐）

#### 设计思路

维护每个 `(metricName + labels)` 组合的累积 bucket counts：

```typescript
interface HistogramState {
  buckets: Map<number, number>; // upperBound -> cumulative count
  sum: number;
  count: number;
  lastUpdate: number;
}

class BaseMetricCollector {
  private histogramStates: Map<string, HistogramState> = new Map();
  
  // 标准 buckets（Prometheus 默认）
  private static readonly DEFAULT_BUCKETS = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, Infinity
  ];
  
  observeHistogram(metricName: string, value: number, labels?: Record<string, string>): void {
    const key = this.getHistogramKey(metricName, labels);
    let state = this.histogramStates.get(key);
    
    if (!state) {
      state = this.initializeHistogramState();
      this.histogramStates.set(key, state);
    }
    
    // 更新累积 buckets
    for (const [bound, _count] of state.buckets.entries()) {
      if (value <= bound) {
        state.buckets.set(bound, state.buckets.get(bound)! + 1);
      }
    }
    
    state.sum += value;
    state.count += 1;
    state.lastUpdate = now();
    
    // 同时记录原始指标用于时间序列
    const metric: HistogramMetric = {
      metricName,
      metricType: "histogram",
      timestamp: now(),
      labels: labels || {},
      value,
      buckets: this.serializeBuckets(state.buckets),
      sum: state.sum,
      count: state.count,
    };
    this.record(metric);
  }
  
  private getHistogramKey(metricName: string, labels?: Record<string, string>): string {
    return `${metricName}:${JSON.stringify(labels || {})}`;
  }
  
  private initializeHistogramState(): HistogramState {
    const buckets = new Map<number, number>();
    for (const bound of BaseMetricCollector.DEFAULT_BUCKETS) {
      buckets.set(bound, 0);
    }
    return { buckets, sum: 0, count: 0, lastUpdate: 0 };
  }
  
  private serializeBuckets(buckets: Map<number, number>): HistogramBucket[] {
    return Array.from(buckets.entries())
      .map(([upperBound, count]) => ({ upperBound, count }))
      .sort((a, b) => a.upperBound - b.upperBound);
  }
}
```

#### 优点
- ✅ 符合 Prometheus histogram 规范
- ✅ 支持高效的百分位数估算
- ✅ 内存占用可控（固定 bucket 数量）
- ✅ 可以跨时间窗口聚合

#### 缺点
- ❌ 需要额外的状态管理
- ❌ flush 时需要正确处理状态重置
- ❌ 自定义 buckets 需要额外配置

#### 适用场景
- 延迟分布监控（API 响应时间、工作流执行时间）
- 资源使用量分布（内存、CPU）
- 任何需要百分位数分析的场景

---

### 解决方案 B：动态 Buckets（自适应）

```typescript
interface AdaptiveHistogramState {
  observations: number[]; // 保留最近 N 个观测值
  maxObservations: number;
  sum: number;
  count: number;
}

// 在查询时动态计算 buckets
queryHistogram(metricName: string): HistogramMetric {
  const state = this.histogramStates.get(metricName);
  if (!state) return null;
  
  // 根据实际数据分布动态生成 buckets
  const buckets = this.calculateDynamicBuckets(state.observations);
  
  return {
    metricName,
    metricType: "histogram",
    timestamp: now(),
    labels: {},
    value: state.sum / state.count, // avg
    buckets,
    sum: state.sum,
    count: state.count,
  };
}
```

#### 优点
- ✅ 自动适应数据分布
- ✅ 不需要预先定义 buckets

#### 缺点
- ❌ 内存开销大（需要保存原始观测值）
- ❌ 计算复杂度高
- ❌ 不符合 Prometheus 规范

#### 适用场景
- 探索性数据分析
- 数据分布未知的场景

---

### 推荐实施方案

**采用方案 A（预定义 Buckets）**，理由：
1. 符合行业标准（Prometheus）
2. 性能可预测
3. 易于理解和维护
4. 支持跨实例聚合

**实施步骤**：
1. 在 `BaseMetricCollector` 中添加 `histogramStates` Map
2. 修改 `observeHistogram()` 维护累积状态
3. 在 `flush()` 中决定是否重置状态（可配置）
4. 更新 `aggregateMetrics()` 正确合并 histograms
5. 确保 `toPrometheus()` 输出完整的 bucket 信息

---

## 2. 实现真正的 Summary（滑动窗口或 t-digest）

### 当前问题

```typescript
// 当前实现只记录单个值
observeSummary(metricName: string, value: number, labels?: Record<string, string>): void {
  const metric = {
    metricName,
    metricType: "summary" as const,
    timestamp: now(),
    labels: labels || {},
    value,
    percentiles: [], // ❌ 空数组
    sum: value,
    count: 1,
  };
  this.record(metric);
}
```

### 解决方案 A：滑动窗口（简单高效，推荐）

#### 设计思路

维护固定大小的环形缓冲区，存储最近的观测值：

```typescript
interface SummaryState {
  ringBuffer: Float64Array; // 固定大小数组
  bufferSize: number;
  writeIndex: number;
  filledCount: number;
  sum: number;
  count: number;
  lastUpdate: number;
}

class BaseMetricCollector {
  private summaryStates: Map<string, SummaryState> = new Map();
  private static readonly DEFAULT_WINDOW_SIZE = 1000; // 保留最近 1000 个观测
  
  observeSummary(
    metricName: string, 
    value: number, 
    labels?: Record<string, string>,
    windowSize?: number
  ): void {
    const key = this.getSummaryKey(metricName, labels);
    let state = this.summaryStates.get(key);
    
    if (!state) {
      state = this.initializeSummaryState(windowSize);
      this.summaryStates.set(key, state);
    }
    
    // 写入环形缓冲区
    state.ringBuffer[state.writeIndex] = value;
    state.writeIndex = (state.writeIndex + 1) % state.bufferSize;
    state.filledCount = Math.min(state.filledCount + 1, state.bufferSize);
    
    state.sum += value;
    state.count += 1;
    state.lastUpdate = now();
    
    // 计算百分位数
    const percentiles = this.calculatePercentiles(state, [0.5, 0.9, 0.95, 0.99]);
    
    // 记录指标
    const metric: SummaryMetric = {
      metricName,
      metricType: "summary",
      timestamp: now(),
      labels: labels || {},
      value,
      percentiles,
      sum: state.sum,
      count: state.count,
    };
    this.record(metric);
  }
  
  private calculatePercentiles(
    state: SummaryState, 
    targets: number[]
  ): PercentileValue[] {
    // 提取有效数据
    const values = new Float64Array(state.filledCount);
    for (let i = 0; i < state.filledCount; i++) {
      const idx = (state.writeIndex + i) % state.bufferSize;
      values[i] = state.ringBuffer[idx];
    }
    
    // 排序（对于小数组，简单排序足够快）
    values.sort();
    
    // 计算百分位数
    return targets.map(p => ({
      percentile: p,
      value: this.getPercentileValue(values, p),
    }));
  }
  
  private getPercentileValue(sortedValues: Float64Array, percentile: number): number {
    if (sortedValues.length === 0) return 0;
    
    const index = percentile * (sortedValues.length - 1);
    const lower = Math.floor(index);
    const upper = Math.ceil(index);
    
    if (lower === upper) {
      return sortedValues[lower];
    }
    
    // 线性插值
    const weight = index - lower;
    return sortedValues[lower] * (1 - weight) + sortedValues[upper] * weight;
  }
  
  private initializeSummaryState(windowSize?: number): SummaryState {
    const size = windowSize || BaseMetricCollector.DEFAULT_WINDOW_SIZE;
    return {
      ringBuffer: new Float64Array(size),
      bufferSize: size,
      writeIndex: 0,
      filledCount: 0,
      sum: 0,
      count: 0,
      lastUpdate: 0,
    };
  }
}
```

#### 优点
- ✅ 实现简单，易于理解
- ✅ 内存占用固定且可控
- ✅ 百分位数准确（基于真实数据）
- ✅ 自动淘汰旧数据（滑动窗口特性）

#### 缺点
- ❌ 排序操作 O(n log n)，但 n 较小（1000）时可接受
- ❌ 不能跨时间窗口精确聚合
- ❌ 高基数 labels 会导致多个窗口

#### 性能分析
```
窗口大小 = 1000
排序复杂度 = O(1000 * log(1000)) ≈ 10,000 次比较
现代 CPU 可在 < 1ms 内完成
内存占用 = 1000 * 8 bytes = 8KB per (metricName + labels)
```

---

### 解决方案 B：t-digest 算法（高级，高精度）

#### 什么是 t-digest？

t-digest 是一种数据结构，专门用于：
- 高效估算大规模数据集的百分位数
- 支持分布式聚合（多个 t-digest 可以合并）
- 内存占用远小于保存所有观测值

#### TypeScript 实现示例

```typescript
// 需要安装: npm install @swc-node/register tdigest
import { TDigest } from 'tdigest';

interface TDigestSummaryState {
  digest: TDigest;
  sum: number;
  count: number;
  lastUpdate: number;
}

observeSummaryWithTDigest(
  metricName: string, 
  value: number, 
  labels?: Record<string, string>
): void {
  const key = this.getSummaryKey(metricName, labels);
  let state = this.summaryStates.get(key) as TDigestSummaryState;
  
  if (!state) {
    state = {
      digest: new TDigest(),
      sum: 0,
      count: 0,
      lastUpdate: 0,
    };
    this.summaryStates.set(key, state);
  }
  
  state.digest.push(value);
  state.sum += value;
  state.count += 1;
  state.lastUpdate = now();
  
  // 计算百分位数（O(1) 查询）
  const percentiles = [
    { percentile: 0.5, value: state.digest.percentile(0.5) },
    { percentile: 0.9, value: state.digest.percentile(0.9) },
    { percentile: 0.95, value: state.digest.percentile(0.95) },
    { percentile: 0.99, value: state.digest.percentile(0.99) },
  ];
  
  const metric: SummaryMetric = {
    metricName,
    metricType: "summary",
    timestamp: now(),
    labels: labels || {},
    value,
    percentiles,
    sum: state.sum,
    count: state.count,
  };
  this.record(metric);
}
```

#### 优点
- ✅ 极高的百分位数精度
- ✅ 支持分布式聚合（多个实例可以合并）
- ✅ 内存效率高（~几 KB 即可处理百万级数据点）
- ✅ 查询速度快 O(1)

#### 缺点
- ❌ 需要外部依赖（tdigest 库）
- ❌ 实现复杂度高
- ❌ 调试困难

#### 适用场景
- 超大规模系统（每秒数万请求）
- 需要跨实例聚合的场景
- 对百分位数精度要求极高

---

### 推荐实施方案

**第一阶段：采用滑动窗口方案 A**
- 实现简单，无外部依赖
- 满足 95% 的使用场景
- 窗口大小可配置（默认 1000）

**第二阶段（可选）：引入 t-digest**
- 当遇到性能瓶颈或需要跨实例聚合时
- 作为高级功能提供

**实施步骤**：
1. 在 `BaseMetricCollector` 中添加 `summaryStates` Map
2. 实现环形缓冲区和百分位数计算
3. 修改 `observeSummary()` 维护状态
4. 在 `flush()` 中处理状态（可选：不清理以保留历史）
5. 添加配置项控制窗口大小

---

## 3. 添加监控（自监控能力）

### 设计目标

metrics 系统本身应该暴露关键指标，用于：
- 检测内存泄漏
- 优化性能
- 故障诊断
- 容量规划

### 需要监控的指标

```typescript
interface CollectorInternalMetrics {
  // 缓冲区状态
  bufferSize: number;
  bufferUtilization: number; // current / max
  
  // 操作统计
  recordCount: number;
  flushCount: number;
  queryCount: number;
  
  // 性能指标
  avgFlushDuration: number;
  avgQueryDuration: number;
  lastFlushDuration: number;
  
  // 清理统计
  cleanupCount: number;
  expiredMetricsRemoved: number;
  lastCleanupTime: number;
  
  // 错误统计
  flushErrorCount: number;
  reportErrorCount: number;
  
  // 订阅者统计
  activeSubscriptions: number;
  
  // 内存估算
  estimatedMemoryUsage: number; // bytes
}
```

### 实施方案

#### 方案 A：内置自监控 Collector（推荐）

```typescript
export abstract class BaseMetricCollector implements MetricCollector {
  protected internalMetrics: CollectorInternalMetrics = {
    bufferSize: 0,
    bufferUtilization: 0,
    recordCount: 0,
    flushCount: 0,
    queryCount: 0,
    avgFlushDuration: 0,
    avgQueryDuration: 0,
    lastFlushDuration: 0,
    cleanupCount: 0,
    expiredMetricsRemoved: 0,
    lastCleanupTime: 0,
    flushErrorCount: 0,
    reportErrorCount: 0,
    activeSubscriptions: 0,
    estimatedMemoryUsage: 0,
  };
  
  // 内部指标 collector（可选：单独实例或复用）
  private selfMetricsCollector?: BaseMetricCollector;
  
  record(metric: Metric): void {
    const startTime = now();
    
    // ... 原有逻辑 ...
    
    // 更新内部指标
    this.internalMetrics.recordCount += 1;
    this.internalMetrics.bufferSize = this.metricsBuffer.length;
    this.internalMetrics.bufferUtilization = 
      this.metricsBuffer.length / this.config.bufferSize;
    this.updateMemoryEstimate();
    
    // 如果启用了自监控，记录内部指标
    if (this.selfMetricsCollector) {
      this.selfMetricsCollector.incrementCounter(
        'metrics.internal.record.count',
        { collector: this.constructor.name }
      );
    }
  }
  
  async flush(): Promise<void> {
    const startTime = now();
    try {
      await this.doFlush();
      
      // 更新成功统计
      const duration = now() - startTime;
      this.internalMetrics.flushCount += 1;
      this.internalMetrics.lastFlushDuration = duration;
      this.updateAverage('avgFlushDuration', duration);
      
    } catch (error) {
      // 更新错误统计
      this.internalMetrics.flushErrorCount += 1;
      throw error;
    }
  }
  
  query(filter: MetricFilter): MetricQueryResult {
    const startTime = now();
    const result = this.doQuery(filter);
    const duration = now() - startTime;
    
    this.internalMetrics.queryCount += 1;
    this.updateAverage('avgQueryDuration', duration);
    
    return result;
  }
  
  private cleanupExpiredMetrics(): void {
    const startTime = now();
    const originalCount = this.metricsBuffer.length;
    
    // ... 原有清理逻辑 ...
    
    const removedCount = originalCount - this.metricsBuffer.length;
    this.internalMetrics.cleanupCount += 1;
    this.internalMetrics.expiredMetricsRemoved += removedCount;
    this.internalMetrics.lastCleanupTime = now();
    
    // 记录清理性能
    if (this.selfMetricsCollector && removedCount > 0) {
      this.selfMetricsCollector.observeHistogram(
        'metrics.internal.cleanup.duration',
        now() - startTime
      );
      this.selfMetricsCollector.incrementCounter(
        'metrics.internal.cleanup.removed_count',
        {},
        removedCount
      );
    }
  }
  
  private updateMemoryEstimate(): void {
    // 粗略估算：每个 metric 约 500 bytes
    this.internalMetrics.estimatedMemoryUsage = 
      this.metricsBuffer.length * 500;
  }
  
  private updateAverage(field: keyof CollectorInternalMetrics, value: number): void {
    const currentAvg = this.internalMetrics[field] as number;
    const count = field === 'avgFlushDuration' 
      ? this.internalMetrics.flushCount 
      : this.internalMetrics.queryCount;
    
    if (count > 0) {
      this.internalMetrics[field] = currentAvg + (value - currentAvg) / count;
    }
  }
  
  /**
   * 获取内部监控指标
   */
  getInternalMetrics(): CollectorInternalMetrics {
    return { ...this.internalMetrics };
  }
  
  /**
   * 导出内部指标为 Prometheus 格式
   */
  exportInternalMetrics(): string[] {
    const metrics: string[] = [];
    const m = this.internalMetrics;
    
    metrics.push(`# HELP metrics_buffer_size Current buffer size`);
    metrics.push(`# TYPE metrics_buffer_size gauge`);
    metrics.push(`metrics_buffer_size{collector="${this.constructor.name}"} ${m.bufferSize}`);
    
    metrics.push(`# HELP metrics_record_total Total records`);
    metrics.push(`# TYPE metrics_record_total counter`);
    metrics.push(`metrics_record_total{collector="${this.constructor.name}"} ${m.recordCount}`);
    
    metrics.push(`# HELP metrics_flush_duration_seconds Average flush duration`);
    metrics.push(`# TYPE metrics_flush_duration_seconds gauge`);
    metrics.push(`metrics_flush_duration_seconds{collector="${this.constructor.name}"} ${(m.avgFlushDuration / 1000).toFixed(3)}`);
    
    metrics.push(`# HELP metrics_cleanup_removed_total Total expired metrics removed`);
    metrics.push(`# TYPE metrics_cleanup_removed_total counter`);
    metrics.push(`metrics_cleanup_removed_total{collector="${this.constructor.name}"} ${m.expiredMetricsRemoved}`);
    
    return metrics;
  }
}
```

#### 方案 B：单独的 MetricsMonitor 类

```typescript
export class MetricsMonitor {
  private collectors: WeakMap<BaseMetricCollector, CollectorInternalMetrics> = new WeakMap();
  
  register(collector: BaseMetricCollector): void {
    this.collectors.set(collector, this.initializeMetrics());
  }
  
  getMetrics(collector: BaseMetricCollector): CollectorInternalMetrics | undefined {
    return this.collectors.get(collector);
  }
  
  generateReport(): MetricsSystemReport {
    // 聚合所有 collector 的指标
  }
}
```

---

### 推荐实施方案

**采用方案 A（内置自监控）**，理由：
1. 实现简单，无需额外组件
2. 低开销（只是计数器更新）
3. 可以直接通过 `getInternalMetrics()` 访问
4. 支持 Prometheus 导出

**关键监控指标优先级**：
1. 🔴 **bufferSize / bufferUtilization** - 检测内存泄漏
2. 🔴 **flushErrorCount** - 检测持久化失败
3. 🟡 **expiredMetricsRemoved** - 验证清理机制工作
4. 🟡 **avgFlushDuration** - 性能监控
5. 🟢 **estimatedMemoryUsage** - 容量规划

**实施步骤**：
1. 添加 `internalMetrics` 字段
2. 在各个方法中更新相关指标
3. 实现 `getInternalMetrics()` 和 `exportInternalMetrics()`
4. 在定期报告中包含内部指标
5. 添加告警阈值配置（可选）

---

## 4. 性能优化（环形缓冲区）

### 当前问题分析

```typescript
// 当前使用普通数组
protected metricsBuffer: Metric[] = [];

record(metric: Metric): void {
  this.metricsBuffer.push(metric); // O(1) amortized
  
  if (this.metricsBuffer.length >= this.config.bufferSize) {
    this.flush(); // 触发 flush
  }
}

cleanupExpiredMetrics(): void {
  this.metricsBuffer = this.metricsBuffer.filter(...); // ❌ O(n) 创建新数组
}
```

**性能瓶颈**：
1. `filter()` 创建新数组，导致 GC 压力
2. 高频场景下数组扩容开销
3. 内存碎片化

---

### 解决方案：环形缓冲区（Ring Buffer）

#### 设计思路

使用固定大小的循环数组，避免动态扩容和频繁分配：

```typescript
class RingBuffer<T> {
  private buffer: Array<T | null>;
  private capacity: number;
  private writeIndex: number = 0;
  private readIndex: number = 0;
  private size: number = 0;
  
  constructor(capacity: number) {
    this.capacity = capacity;
    this.buffer = new Array(capacity).fill(null);
  }
  
  push(item: T): void {
    if (this.size === this.capacity) {
      // 缓冲区满，覆盖最旧的元素
      this.readIndex = (this.readIndex + 1) % this.capacity;
      this.size--;
    }
    
    this.buffer[this.writeIndex] = item;
    this.writeIndex = (this.writeIndex + 1) % this.capacity;
    this.size++;
  }
  
  toArray(): T[] {
    const result: T[] = [];
    for (let i = 0; i < this.size; i++) {
      const idx = (this.readIndex + i) % this.capacity;
      if (this.buffer[idx] !== null) {
        result.push(this.buffer[idx] as T);
      }
    }
    return result;
  }
  
  filter(predicate: (item: T) => boolean): T[] {
    const result: T[] = [];
    for (let i = 0; i < this.size; i++) {
      const idx = (this.readIndex + i) % this.capacity;
      const item = this.buffer[idx];
      if (item !== null && predicate(item)) {
        result.push(item);
      }
    }
    return result;
  }
  
  clear(): void {
    this.buffer.fill(null);
    this.writeIndex = 0;
    this.readIndex = 0;
    this.size = 0;
  }
  
  getSize(): number {
    return this.size;
  }
  
  isFull(): boolean {
    return this.size === this.capacity;
  }
}
```

#### 集成到 BaseMetricCollector

```typescript
export abstract class BaseMetricCollector implements MetricCollector {
  protected metricsBuffer: RingBuffer<Metric>; // 改用环形缓冲区
  private config: Required<MetricCollectorConfig>;
  
  constructor(config?: MetricCollectorConfig) {
    this.config = {
      bufferSize: config?.bufferSize ?? 100,
      // ...
    };
    
    // 初始化环形缓冲区（容量略大于 bufferSize 以留有余地）
    this.metricsBuffer = new RingBuffer<Metric>(
      Math.ceil(this.config.bufferSize * 1.2)
    );
  }
  
  record(metric: Metric): void {
    // ... 验证逻辑 ...
    
    this.metricsBuffer.push(metric);
    
    // 检查是否需要 flush
    if (this.metricsBuffer.getSize() >= this.config.bufferSize) {
      this.flush().catch(...);
    }
  }
  
  private cleanupExpiredMetrics(): void {
    const cutoffTime = now() - this.config.maxAge;
    const remaining = this.metricsBuffer.filter(
      (metric) => metric.timestamp >= cutoffTime
    );
    
    // 重建缓冲区
    this.metricsBuffer.clear();
    for (const metric of remaining) {
      this.metricsBuffer.push(metric);
    }
  }
  
  query(filter: MetricFilter): MetricQueryResult {
    const allMetrics = this.metricsBuffer.toArray();
    // ... 原有过滤逻辑 ...
  }
  
  async flush(): Promise<void> {
    const metricsToFlush = this.metricsBuffer.toArray();
    
    if (metricsToFlush.length > 0) {
      await this.persistMetrics(metricsToFlush);
      
      // 清空缓冲区
      this.metricsBuffer.clear();
    }
  }
}
```

---

### 性能对比

| 操作 | 普通数组 | 环形缓冲区 | 提升 |
|------|---------|-----------|------|
| push | O(1)* | O(1) | 稳定 |
| filter | O(n) + 分配新数组 | O(n) + 无分配 | 减少 GC |
| clear | O(n) | O(1) | 显著 |
| 内存占用 | 动态增长 | 固定 | 可预测 |
| GC 压力 | 高 | 低 | 显著降低 |

*\* 数组扩容时 O(n)*

---

### 权衡分析

#### 优点
- ✅ 固定内存占用，无意外增长
- ✅ 减少 GC 压力（无频繁数组分配）
- ✅ `clear()` 操作 O(1)
- ✅ 适合高频写入场景

#### 缺点
- ❌ 实现复杂度增加
- ❌ 缓冲区满时会丢弃最旧数据（需权衡）
- ❌ 遍历稍慢（需要处理环绕）
- ❌ 不适合需要随机访问的场景

---

### 推荐实施方案

**分阶段实施**：

**阶段 1：评估必要性**
- 先实施前三个改进（Histogram、Summary、自监控）
- 通过自监控数据判断是否需要环形缓冲区
- 关注指标：`bufferUtilization`、GC 频率、内存增长曲线

**阶段 2：条件启用**
```typescript
interface MetricCollectorConfig {
  useRingBuffer?: boolean; // 默认 false
  ringBufferCapacity?: number;
}

constructor(config?: MetricCollectorConfig) {
  if (config?.useRingBuffer) {
    this.metricsBuffer = new RingBuffer<Metric>(
      config.ringBufferCapacity ?? this.config.bufferSize * 1.2
    );
  } else {
    this.metricsBuffer = [] as any; // 保持兼容性
  }
}
```

**阶段 3：全面切换**
- 当确认收益明显后，默认启用环形缓冲区
- 提供迁移指南

**适用场景判断**：
- ✅ **需要**：每秒 > 1000 次 record 调用
- ✅ **需要**：长时间运行服务（数天/数周）
- ❌ **不需要**：低频场景（每秒 < 100 次）
- ❌ **不需要**：短期运行的任务

---

## 综合实施路线图

### Phase 1：核心功能完善（1-2 周）

**优先级：🔴 高**

1. **实现累积 Histogram**
   - 添加 `histogramStates` Map
   - 修改 `observeHistogram()`
   - 更新 `toPrometheus()` 输出 buckets

2. **实现滑动窗口 Summary**
   - 添加 `summaryStates` Map
   - 实现环形缓冲区和百分位数计算
   - 修改 `observeSummary()`

3. **添加基础自监控**
   - 添加 `internalMetrics` 字段
   - 在关键方法中更新指标
   - 实现 `getInternalMetrics()`

**预期收益**：
- ✅ Histogram 和 Summary 可用
- ✅ 可以监控系统健康状态
- ✅ 符合 Prometheus 规范

---

### Phase 2：性能优化（1 周）

**优先级：🟡 中**

4. **实施环形缓冲区**
   - 实现 `RingBuffer<T>` 类
   - 集成到 `BaseMetricCollector`
   - 添加配置开关

5. **性能测试和优化**
   - 基准测试对比
   - 调整缓冲区大小
   - 优化热点路径

**预期收益**：
- ✅ 降低 GC 压力 30-50%
- ✅ 内存占用更可预测
- ✅ 高频场景性能提升

---

### Phase 3：高级功能（可选，2-3 周）

**优先级：🟢 低**

6. **t-digest 支持**
   - 集成 tdigest 库
   - 实现 `observeSummaryWithTDigest()`
   - 支持分布式聚合

7. **智能告警**
   - 基于自监控指标的告警规则
   - 异常检测（突增、突降）
   - 自动报告生成

8. **可视化仪表板**
   - Grafana 模板
   - 实时监控面板
   - 历史趋势分析

**预期收益**：
- ✅ 企业级监控能力
- ✅ 自动化运维支持
- ✅ 更好的可观测性

---

## 风险评估

### 技术风险

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|---------|
| Histogram 状态管理bug | 中 | 高 | 充分单元测试 |
| 滑动窗口内存泄漏 | 低 | 高 | 定期审查代码 |
| 环形缓冲区边界条件 | 中 | 中 | 边界测试用例 |
| 性能回归 | 低 | 中 | 基准测试对比 |

### 兼容性风险

- ⚠️ **Breaking Change**: Histogram/Summary 的内部表示变化
  - **缓解**: 保持 API 不变，仅内部实现变化
  - **缓解**: 提供迁移指南

- ⚠️ **Breaking Change**: `flush()` 行为可能变化
  - **缓解**: 添加配置项控制是否重置状态

---

## 测试策略

### 单元测试

```typescript
describe('Histogram Accumulation', () => {
  it('should accumulate bucket counts correctly', () => {
    collector.observeHistogram('test.duration', 0.5);
    collector.observeHistogram('test.duration', 1.5);
    collector.observeHistogram('test.duration', 3.0);
    
    const result = collector.query({ metricName: 'test.duration' });
    const histogram = result.metrics.get('test.duration');
    
    expect(histogram.buckets).toBeDefined();
    expect(histogram.count).toBe(3);
    // 验证 buckets 累积正确
  });
});

describe('Summary Percentiles', () => {
  it('should calculate accurate percentiles', () => {
    // 记录 100 个值
    for (let i = 1; i <= 100; i++) {
      collector.observeSummary('test.latency', i);
    }
    
    const result = collector.query({ metricName: 'test.latency' });
    const summary = result.metrics.get('test.latency');
    
    expect(summary.percentiles).toBeDefined();
    expect(summary.percentiles.find(p => p.percentile === 0.5)?.value).toBeCloseTo(50, 1);
    expect(summary.percentiles.find(p => p.percentile === 0.95)?.value).toBeCloseTo(95, 1);
  });
});

describe('Self-Monitoring', () => {
  it('should track internal metrics', () => {
    const metrics = collector.getInternalMetrics();
    
    collector.record({ /* ... */ });
    
    const updated = collector.getInternalMetrics();
    expect(updated.recordCount).toBe(metrics.recordCount + 1);
    expect(updated.bufferSize).toBe(metrics.bufferSize + 1);
  });
});
```

### 集成测试

```typescript
describe('End-to-End Metrics Flow', () => {
  it('should handle high-frequency recording', async () => {
    const iterations = 10000;
    const start = now();
    
    for (let i = 0; i < iterations; i++) {
      collector.observeHistogram('perf.test', Math.random() * 100);
    }
    
    const duration = now() - start;
    const throughput = iterations / (duration / 1000);
    
    console.log(`Throughput: ${throughput.toFixed(0)} ops/sec`);
    expect(throughput).toBeGreaterThan(1000); // 至少 1000 ops/sec
  });
  
  it('should not leak memory over time', async () => {
    const initialMemory = process.memoryUsage().heapUsed;
    
    // 模拟长时间运行
    for (let hour = 0; hour < 24; hour++) {
      for (let i = 0; i < 1000; i++) {
        collector.record({ /* ... */ });
      }
      await collector.flush();
      await sleep(100); // 模拟时间流逝
    }
    
    const finalMemory = process.memoryUsage().heapUsed;
    const growth = (finalMemory - initialMemory) / initialMemory;
    
    expect(growth).toBeLessThan(0.1); // 内存增长 < 10%
  });
});
```

### 基准测试

```typescript
// benchmarks/metrics.bench.ts
import { bench, describe } from 'vitest';

describe('RingBuffer vs Array', () => {
  const size = 10000;
  
  bench('Array push + filter', () => {
    const arr: number[] = [];
    for (let i = 0; i < size; i++) {
      arr.push(i);
    }
    arr.filter(x => x > size / 2);
  });
  
  bench('RingBuffer push + filter', () => {
    const rb = new RingBuffer<number>(size);
    for (let i = 0; i < size; i++) {
      rb.push(i);
    }
    rb.filter(x => x > size / 2);
  });
});
```

---

## 总结与建议

### 立即实施（本周）

1. ✅ **累积 Histogram** - 已有完整方案，风险低，收益高
2. ✅ **滑动窗口 Summary** - 实现简单，满足大部分需求
3. ✅ **基础自监控** - 低成本，高价值

### 短期计划（1 个月内）

4. 🔄 **环形缓冲区** - 基于自监控数据决策
5. 🔄 **性能优化** - 针对性优化热点路径

### 长期规划（季度）

6. 📅 **t-digest 支持** - 视业务需求而定
7. 📅 **高级告警** - 结合运维体系
8. 📅 **可视化** - 提升可观测性

### 关键成功因素

- ✅ **渐进式实施**：不要一次性全部改动
- ✅ **充分测试**：每个改进都要有完整的测试覆盖
- ✅ **监控先行**：先加自监控，再优化性能
- ✅ **向后兼容**：保持 API 稳定
- ✅ **文档完善**：记录设计决策和使用指南

---

## 附录：参考资源

1. **Prometheus Histograms**: https://prometheus.io/docs/practices/histograms/
2. **t-digest Paper**: https://github.com/tdunning/t-digest
3. **Ring Buffer Implementation**: https://en.wikipedia.org/wiki/Circular_buffer
4. **Node.js Performance**: https://nodejs.org/en/docs/guides/simple-profiling/
