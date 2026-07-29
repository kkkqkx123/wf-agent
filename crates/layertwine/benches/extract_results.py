#!/usr/bin/env python3
"""
提取基准测试结果并保存到 benches/results 目录
"""

import json
import os
import sys
from pathlib import Path
from datetime import datetime

def extract_benchmark_results():
    """提取所有基准测试结果"""
    criterion_dir = Path("target/criterion")
    results = {}

    if not criterion_dir.exists():
        print("基准测试结果目录不存在，请先运行基准测试")
        return results

    print(f"扫描基准测试目录: {criterion_dir}")

    # 遍历基准测试目录
    for benchmark_dir in criterion_dir.iterdir():
        if benchmark_dir.is_dir() and not benchmark_dir.name.startswith('.'):
            # 查找新的结果文件
            json_file = benchmark_dir / "new" / "estimates.json"
            if json_file.exists():
                try:
                    with open(json_file, 'r') as f:
                        data = json.load(f)
                        if 'mean' in data and 'median' in data:
                            results[benchmark_dir.name] = {
                                'mean_ns': data['mean']['point_estimate'],
                                'std_dev_ns': data['mean']['standard_deviation'] if 'standard_deviation' in data['mean'] else data.get('std_dev', {}).get('point_estimate', 0),
                                'median_ns': data['median']['point_estimate'],
                                'mean_ci_lower': data['mean']['confidence_interval']['lower_bound'],
                                'mean_ci_upper': data['mean']['confidence_interval']['upper_bound'],
                                'median_ci_lower': data['median']['confidence_interval']['lower_bound'],
                                'median_ci_upper': data['median']['confidence_interval']['upper_bound'],
                                'unit': 'nanoseconds'
                            }
                            print(f"  - {benchmark_dir.name}: {data['mean']['point_estimate']:.2f} ns")
                except (json.JSONDecodeError, KeyError) as e:
                    print(f"  - {benchmark_dir.name}: 读取失败 - {e}")
                    continue

    return results

def classify_benchmark(name):
    """根据基准测试名称分类"""
    if 'diff' in name:
        return 'diff'
    elif 'merge' in name or 'apply_deltas' in name or 'merge_texts' in name:
        return 'merge'
    elif 'inverse' in name:
        return 'inverse'
    elif 'content_id' in name or 'delta_compute' in name or 'snapshot_compute' in name:
        return 'core_types'
    elif 'store' in name or 'get' in name or 'batch' in name:
        return 'storage'
    elif 'serialize' in name or 'deserialize' in name:
        return 'serialization'
    else:
        return 'other'

def generate_analysis(results):
    """生成性能分析报告"""
    analysis = {
        'performance_issues': [],
        'bottlenecks': [],
        'recommendations': []
    }

    # 分析性能问题
    for name, metrics in results.items():
        # 检查是否有很高的标准差（性能不稳定）
        if metrics['std_dev_ns'] > metrics['mean_ns'] * 0.2:
            analysis['performance_issues'].append({
                'benchmark': name,
                'issue': 'high_variance',
                'description': f"High performance variance, standard deviation is {(metrics['std_dev_ns']/metrics['mean_ns']*100):.1f}% of mean",
                'variance_ratio': metrics['std_dev_ns'] / metrics['mean_ns'],
                'severity': 'medium'
            })

        # 检查慢操作（超过10微秒）
        if metrics['mean_ns'] > 10000:
            category = classify_benchmark(name)
            analysis['bottlenecks'].append({
                'benchmark': name,
                'category': category,
                'description': f"High execution time: {metrics['mean_ns']/1000:.2f} microseconds",
                'value': metrics['mean_ns'],
                'severity': 'high' if metrics['mean_ns'] > 100000 else 'medium'
            })

        # 检查超慢操作（超过100微秒）
        if metrics['mean_ns'] > 100000:
            category = classify_benchmark(name)
            analysis['bottlenecks'].append({
                'benchmark': name,
                'category': category,
                'description': f"Critically high execution time: {metrics['mean_ns']/1000000:.2f} milliseconds",
                'value': metrics['mean_ns'],
                'severity': 'critical'
            })

    # 生成优化建议
    if analysis['performance_issues']:
        analysis['recommendations'].append({
            'category': 'stability',
            'priority': 'medium',
            'suggestion': 'Multiple benchmarks show high performance variance. Check environmental factors and code stability.',
            'affected_benchmarks': [issue['benchmark'] for issue in analysis['performance_issues']]
        })

    # 按严重程度分组瓶颈
    critical_bottlenecks = [b for b in analysis['bottlenecks'] if b.get('severity') == 'critical']
    high_bottlenecks = [b for b in analysis['bottlenecks'] if b.get('severity') == 'high']
    medium_bottlenecks = [b for b in analysis['bottlenecks'] if b.get('severity') == 'medium']

    if critical_bottlenecks:
        analysis['recommendations'].append({
            'category': 'critical_performance',
            'priority': 'critical',
            'suggestion': 'Critical performance bottlenecks detected. Immediate optimization required for large file operations.',
            'affected_benchmarks': [b['benchmark'] for b in critical_bottlenecks],
            'count': len(critical_bottlenecks)
        })

    if high_bottlenecks:
        analysis['recommendations'].append({
            'category': 'performance',
            'priority': 'high',
            'suggestion': 'High execution time operations detected. Consider performance analysis and optimization.',
            'affected_benchmarks': [b['benchmark'] for b in high_bottlenecks],
            'count': len(high_bottlenecks)
        })

    if medium_bottlenecks:
        analysis['recommendations'].append({
            'category': 'optimization',
            'priority': 'medium',
            'suggestion': 'Moderate execution time operations identified. Consider gradual optimization.',
            'affected_benchmarks': [b['benchmark'] for b in medium_bottlenecks],
            'count': len(medium_bottlenecks)
        })

    return analysis

def main():
    # 创建结果目录
    results_dir = Path("benches/results")
    results_dir.mkdir(parents=True, exist_ok=True)

    print("开始提取基准测试结果...")

    # 提取结果
    results = extract_benchmark_results()

    if not results:
        print("没有找到基准测试结果")
        return 1

    print(f"\n共提取了 {len(results)} 个基准测试结果")

    # 按类别组织结果
    categorized_results = {}
    for name, metrics in results.items():
        category = classify_benchmark(name)
        if category not in categorized_results:
            categorized_results[category] = {}
        categorized_results[category][name] = metrics

    # 保存完整的 JSON 结果
    output_file = results_dir / "benchmark_summary.json"
    summary_data = {
        'timestamp': datetime.now().isoformat(),
        'total_benchmarks': len(results),
        'categories': {k: len(v) for k, v in categorized_results.items()},
        'results': categorized_results
    }
    with open(output_file, 'w') as f:
        json.dump(summary_data, f, indent=2)

    print(f"完整的基准测试结果已保存到: {output_file}")

    # 生成可读的文本报告
    report_file = results_dir / "benchmark_report.txt"
    with open(report_file, 'w', encoding='utf-8') as f:
        f.write("Layertwine Benchmark Results Report\n")
        f.write("=" * 80 + "\n")
        f.write(f"Generated: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n")
        f.write(f"Total Benchmarks: {len(results)}\n\n")

        for category, category_results in categorized_results.items():
            f.write(f"## {category.upper()} Benchmarks\n")
            f.write("-" * 40 + "\n")
            for test_name, metrics in sorted(category_results.items()):
                f.write(f"Test: {test_name}\n")
                f.write(f"  Mean Time: {metrics['mean_ns']:.2f} ns ({metrics['mean_ns']/1000:.3f} μs)\n")
                f.write(f"  Median: {metrics['median_ns']:.2f} ns\n")
                f.write(f"  Std Dev: {metrics['std_dev_ns']:.2f} ns ({metrics['std_dev_ns']/metrics['mean_ns']*100:.1f}%)\n")
                f.write(f"  95% CI: [{metrics['mean_ci_lower']:.2f}, {metrics['mean_ci_upper']:.2f}] ns\n")
                f.write("\n")

    print(f"可读的基准测试报告已保存到: {report_file}")

    # 生成性能分析
    analysis = generate_analysis(results)

    # 保存分析结果
    analysis_file = results_dir / "performance_analysis.json"
    analysis_data = {
        'timestamp': datetime.now().isoformat(),
        'total_benchmarks': len(results),
        'analysis': analysis
    }
    with open(analysis_file, 'w') as f:
        json.dump(analysis_data, f, indent=2)

    print(f"性能分析结果已保存到: {analysis_file}")

    # 生成分析报告
    if analysis['performance_issues'] or analysis['bottlenecks']:
        print("\n" + "=" * 80)
        print("PERFORMANCE ISSUES DETECTED:")
        print("=" * 80)

        if analysis['performance_issues']:
            print("\nHigh Variance Benchmarks:")
            for issue in analysis['performance_issues']:
                print(f"  - {issue['benchmark']}: {issue['description']}")

        if analysis['bottlenecks']:
            print("\nPerformance Bottlenecks:")
            for bottleneck in analysis['bottlenecks']:
                severity = bottleneck.get('severity', 'unknown').upper()
                print(f"  - [{severity}] {bottleneck['benchmark']}: {bottleneck['description']}")

        if analysis['recommendations']:
            print("\nRecommendations:")
            for recommendation in analysis['recommendations']:
                priority = recommendation.get('priority', 'low').upper()
                print(f"  - [{priority}] {recommendation['suggestion']}")

    return 0

if __name__ == "__main__":
    sys.exit(main())