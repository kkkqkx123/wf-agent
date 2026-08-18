# 基准测试结果使用指南

## 概述

本指南说明如何使用 `benches/results/` 目录中的基准测试结果文件。

## 文件说明

### 1. benchmark_summary.json
包含所有基准测试的详细数据，结构如下：

```json
{
  "timestamp": "2026-06-10T02:50:09.752181",
  "total_benchmarks": 32,
  "categories": {
    "core_types": 8,
    "diff": 11,
    "merge": 12,
    "inverse": 1
  },
  "results": {
    "core_types": {
      "test_name": {
        "mean_ns": 1234.56,
        "std_dev_ns": 78.90,
        "median_ns": 1230.00,
        "mean_ci_lower": 1200.00,
        "mean_ci_upper": 1270.00,
        "median_ci_lower": 1215.00,
        "median_ci_upper": 1245.00,
        "unit": "nanoseconds"
      }
    }
  }
}
```

### 2. performance_analysis.json
包含性能分析和优化建议，结构如下：

```json
{
  "timestamp": "2026-06-10T02:50:09.753190",
  "total_benchmarks": 32,
  "analysis": {
    "performance_issues": [...],
    "bottlenecks": [...],
    "recommendations": [...]
  }
}
```

### 3. benchmark_report.txt
人类可读的文本报告，包含格式化的测试结果。

## 使用示例

### Python 示例

```python
import json

# 读取基准测试结果
with open('benches/results/benchmark_summary.json', 'r') as f:
    data = json.load(f)

# 获取特定测试结果
test_result = data['results']['diff']['diff_to_line_diff_small_10_lines_10_percent']
print(f"平均时间: {test_result['mean_ns']:.2f} ns")
print(f"标准差: {test_result['std_dev_ns']:.2f} ns")

# 获取性能瓶颈
with open('benches/results/performance_analysis.json', 'r') as f:
    analysis = json.load(f)

for bottleneck in analysis['analysis']['bottlenecks']:
    if bottleneck['severity'] == 'critical':
        print(f"严重瓶颈: {bottleneck['benchmark']}")
        print(f"描述: {bottleneck['description']}")
```

### JavaScript 示例

```javascript
// 读取基准测试结果
const fs = require('fs');
const data = JSON.parse(fs.readFileSync('benches/results/benchmark_summary.json', 'utf8'));

// 获取特定测试结果
const testResult = data.results.diff['diff_to_line_diff_small_10_lines_10_percent'];
console.log(`平均时间: ${testResult.mean_ns.toFixed(2)} ns`);

// 获取性能瓶颈
const analysis = JSON.parse(fs.readFileSync('benches/results/performance_analysis.json', 'utf8'));
analysis.analysis.bottlenecks
  .filter(b => b.severity === 'critical')
  .forEach(bottleneck => {
    console.log(`严重瓶颈: ${bottleneck.benchmark}`);
    console.log(`描述: ${bottleneck.description}`);
  });
```

## 重新生成结果

### 运行单个基准测试

```bash
cargo bench --bench diff diff_small
```

### 运行所有基准测试

```bash
cargo bench
```

### 提取结果

```bash
python3 benches/extract_results.py
```

## 性能监控

### 设置性能基线

```bash
cargo bench -- --save-baseline baseline
```

### 与基线比较

```bash
cargo bench -- --baseline baseline
```

## 集成到 CI/CD

### GitHub Actions 示例

```yaml
name: Performance Tests

on: [push, pull_request]

jobs:
  benchmark:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run benchmarks
        run: cargo bench
      - name: Extract results
        run: python3 benches/extract_results.py
      - name: Upload results
        uses: actions/upload-artifact@v3
        with:
          name: benchmark-results
          path: benches/results/
```

## 常见问题

### Q: 如何添加新的基准测试？

A: 在 `benches/` 目录创建新的 `.rs` 文件，然后在 `Cargo.toml` 中添加对应的 `[[bench]]` 条目。

### Q: 如何比较不同版本的性能？

A: 使用 Criterion 的 baseline 功能：
```bash
# 保存当前版本为基线
cargo bench -- --save-baseline v1.0

# 比较新版本与基线
cargo bench -- --baseline v1.0
```

### Q: 如何解读置信区间？

A: 95% 置信区间表示如果我们重复运行测试 100 次，有 95 次的结果会落在这个范围内。置信区间越窄，结果越稳定。

### Q: 标准差过高说明什么？

A: 标准差过高表示性能不稳定，可能受系统负载、缓存状态或其他环境因素影响。标准差超过平均值的 20% 通常被认为是高方差。

## 性能指标说明

### mean_ns（平均时间）
所有样本的算术平均值，是最常用的性能指标。

### std_dev_ns（标准差）
衡量结果的离散程度，标准差越小表示性能越稳定。

### median_ns（中位数）
将所有样本排序后位于中间的值，对异常值不敏感。

### 置信区间
表示真实平均值可能落入的范围，通常使用 95% 置信水平。

---

**文档版本**: 1.0
**最后更新**: 2026-06-10