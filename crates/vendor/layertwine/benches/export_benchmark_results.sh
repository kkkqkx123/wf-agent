#!/bin/bash

# 导出基准测试结果到 benches/results 目录

set -e

# 创建结果目录
mkdir -p benches/results

echo "运行基准测试并导出结果..."

# 运行所有基准测试
cargo bench --bench diff -- --save-baseline baseline
cargo bench --bench merge -- --save-baseline baseline
cargo bench --bench inverse -- --save-baseline baseline
cargo bench --bench core_types -- --save-baseline baseline
cargo bench --bench storage -- --save-baseline baseline
cargo bench --bench serialization -- --save-baseline baseline

# 复制基准测试结果到 results 目录
echo "导出基准测试结果..."

# 创建一个汇总文档
cat > benches/results/summary.md << 'EOF'
# 基准测试结果汇总

本目录包含 Layertwine 项目的性能基准测试结果。

## 测试概述

- **测试日期**: $(date '+%Y-%m-%d %H:%M:%S')
- **测试环境**: $(uname -a)
- **Rust 版本**: $(rustc --version)
- **Cargo 版本**: $(cargo --version)

## 测试文件

1. **diff.json** - Diff 引擎性能测试结果
2. **merge.json** - Merge 引擎性能测试结果
3. **inverse.json** - Inverse 引擎性能测试结果
4. **core_types.json** - 核心类型计算性能测试结果
5. **storage.json** - 存储操作性能测试结果
6. **serialization.json** - 序列化性能测试结果

## 导出说明

原始基准测试数据保存在 `target/criterion/` 目录中，包含详细的统计信息和可视化图表。
本目录中的 JSON 文件是汇总后的测试结果。

## 性能分析

详细的性能分析请参见：`docs/issue/performance_benchmark_analysis.md`
EOF

# 提取基准测试的关键指标
echo "提取基准测试指标..."

# 创建一个脚本来解析 Criterion 的 JSON 输出
cat > benches/results/extract_results.py << 'PYTHON_EOF'
#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

def extract_criterion_results(benchmark_name):
    """提取 Criterion 基准测试结果"""
    criterion_dir = Path("target/criterion")
    results = {}

    if not criterion_dir.exists():
        return results

    # 遍历基准测试目录
    for item in criterion_dir.iterdir():
        if item.is_dir():
            json_file = item / "new" / "estimates.json"
            if json_file.exists():
                try:
                    with open(json_file, 'r') as f:
                        data = json.load(f)
                        if 'mean' in data and 'median' in data:
                            results[item.name] = {
                                'mean_ns': data['mean']['point_estimate'],
                                'std_dev_ns': data['mean']['standard_deviation'],
                                'median_ns': data['median']['point_estimate'],
                                'min_ns': data.get('mean', {}).get('confidence_interval', {}).get('lower_bound', 0),
                                'max_ns': data.get('mean', {}).get('confidence_interval', {}).get('upper_bound', 0)
                            }
                except (json.JSONDecodeError, KeyError) as e:
                    continue

    return results

# 为每个基准测试提取结果
benchmarks = ['diff', 'merge', 'inverse', 'core_types', 'storage', 'serialization']
all_results = {}

for benchmark in benchmarks:
    results = extract_criterion_results(benchmark)
    if results:
        all_results[benchmark] = results

# 保存汇总结果
output_file = "benches/results/benchmark_summary.json"
with open(output_file, 'w') as f:
    json.dump(all_results, f, indent=2)

print(f"基准测试结果已保存到: {output_file}")

# 生成可读的报告
report_file = "benches/results/benchmark_report.txt"
with open(report_file, 'w') as f:
    f.write("Layertwine 项目基准测试结果报告\n")
    f.write("=" * 60 + "\n\n")
    f.write(f"生成时间: {os.popen('date').read().strip()}\n\n")

    for benchmark, results in all_results.items():
        f.write(f"## {benchmark.upper()} 基准测试\n")
        f.write("-" * 40 + "\n")
        for test_name, metrics in results.items():
            f.write(f"测试: {test_name}\n")
            f.write(f"  平均时间: {metrics['mean_ns']:.2f} ns\n")
            f.write(f"  中位数: {metrics['median_ns']:.2f} ns\n")
            f.write(f"  标准差: {metrics['std_dev_ns']:.2f} ns\n")
            f.write(f"  最小值: {metrics['min_ns']:.2f} ns\n")
            f.write(f"  最大值: {metrics['max_ns']:.2f} ns\n")
            f.write("\n")

print(f"基准测试报告已保存到: {report_file}")
PYTHON_EOF

# 运行 Python 脚本提取结果
if command -v python3 &> /dev/null; then
    python3 benches/results/extract_results.py
else
    echo "警告: Python3 未安装，无法自动提取基准测试结果"
    echo "基准测试结果保存在 target/criterion/ 目录中"
fi

echo ""
echo "基准测试完成！结果保存在 benches/results/ 目录中"