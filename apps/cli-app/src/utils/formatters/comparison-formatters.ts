/**
 * Execution Comparison Formatters
 * Format execution comparison results for CLI output
 */

import type {
  ExecutionComparison,
  RangeComparison,
} from "../../adapters/execution-comparison-adapter.js";

export function formatExecutionComparison(
  comparison: ExecutionComparison
): string {
  const lines: string[] = [];

  lines.push("");
  lines.push("EXECUTION COMPARISON REPORT");
  lines.push("===========================");
  lines.push("");

  lines.push(
    `Comparing: ${comparison.executionId1} vs ${comparison.executionId2}`
  );
  lines.push("");

  // Performance section
  lines.push("PERFORMANCE METRICS");
  lines.push("-------------------");
  lines.push("");

  const perf = comparison.performance;

  // Duration
  const durationStatus =
    perf.verdict === "exec2_better"
      ? perf.duration.improved
        ? "[Improved]"
        : "[Degraded]"
      : "[Equal]";
  lines.push(
    `Duration ${durationStatus}: ${perf.duration.base}ms -> ${perf.duration.current}ms (${perf.duration.changePercent > 0 ? "+" : ""}${perf.duration.changePercent}%)`
  );

  // Throughput
  const throughputStatus = perf.throughput.improved ? "[Improved]" : "[Degraded]";
  lines.push(
    `Throughput ${throughputStatus}: ${perf.throughput.base.toFixed(2)} -> ${perf.throughput.current.toFixed(2)} ops/sec (${perf.throughput.changePercent > 0 ? "+" : ""}${perf.throughput.changePercent}%)`
  );

  // Success Rate
  const successStatus = perf.successRate.improved ? "[Improved]" : "[Degraded]";
  lines.push(
    `Success Rate ${successStatus}: ${perf.successRate.base.toFixed(1)}% -> ${perf.successRate.current.toFixed(1)}% (${perf.successRate.changePercent > 0 ? "+" : ""}${perf.successRate.changePercent}%)`
  );

  lines.push("");
  const verdictStatus =
    perf.verdict === "exec1_better"
      ? "[BETTER - Execution 1]"
      : perf.verdict === "exec2_better"
        ? "[BETTER - Execution 2]"
        : "[TIE]";
  lines.push(`Verdict: ${verdictStatus}`);
  lines.push("");

  // Errors section
  if (comparison.errors.totalErrorCount.change !== 0) {
    lines.push("ERROR ANALYSIS");
    lines.push("--------------");
    lines.push("");

    const errStatus = comparison.errors.totalErrorCount.improved ? "[OK]" : "[WARNING]";
    lines.push(
      `${errStatus} Total Errors: ${comparison.errors.totalErrorCount.base} -> ${comparison.errors.totalErrorCount.current} (${comparison.errors.totalErrorCount.changePercent > 0 ? "+" : ""}${comparison.errors.totalErrorCount.changePercent}%)`
    );

    if (comparison.errors.newErrors.length > 0) {
      lines.push(`  NEW Error Types: ${comparison.errors.newErrors.length}`);
      for (const err of comparison.errors.newErrors.slice(0, 3)) {
        lines.push(`    - ${err.type} (${err.count})`);
      }
    }

    if (comparison.errors.fixedErrors.length > 0) {
      lines.push(`  FIXED Error Types: ${comparison.errors.fixedErrors.length}`);
      for (const err of comparison.errors.fixedErrors.slice(0, 3)) {
        lines.push(`    - ${err.type}`);
      }
    }

    if (comparison.errors.commonErrors.length > 0) {
      lines.push(
        `  COMMON Error Types: ${comparison.errors.commonErrors.length}`
      );
      for (const err of comparison.errors.commonErrors.slice(0, 2)) {
        lines.push(`    - ${err.type}`);
      }
    }

    lines.push("");
  }

  // Interruptions section
  if (comparison.interruptions.totalCount.change !== 0) {
    lines.push("INTERRUPTION ANALYSIS");
    lines.push("---------------------");
    lines.push("");

    const intrStatus = comparison.interruptions.totalCount.improved
      ? "[OK]"
      : "[WARNING]";
    lines.push(
      `${intrStatus} Total Interruptions: ${comparison.interruptions.totalCount.base} -> ${comparison.interruptions.totalCount.current} (${comparison.interruptions.totalCount.changePercent > 0 ? "+" : ""}${comparison.interruptions.totalCount.changePercent}%)`
    );

    lines.push(
      `  Recovery Rate: ${(comparison.interruptions.recoveryRate.base * 100).toFixed(1)}% -> ${(comparison.interruptions.recoveryRate.current * 100).toFixed(1)}%`
    );

    lines.push("");
  }

  // Findings
  if (comparison.findings.length > 0) {
    lines.push("KEY FINDINGS");
    lines.push("------------");
    lines.push("");
    for (const finding of comparison.findings) {
      lines.push(`  - ${finding}`);
    }
    lines.push("");
  }

  // Recommendations
  if (comparison.recommendations.length > 0) {
    lines.push("RECOMMENDATIONS");
    lines.push("---------------");
    lines.push("");
    for (const rec of comparison.recommendations) {
      lines.push(`  - ${rec}`);
    }
    lines.push("");
  }

  return lines.join("\n");
}

export function formatRangeComparison(comparison: RangeComparison): string {
  const lines: string[] = [];

  lines.push("");
  lines.push("EXECUTION TREND ANALYSIS");
  lines.push("========================");
  lines.push("");

  lines.push(`Executions Analyzed: ${comparison.executionIds.length}`);
  lines.push("");

  lines.push("PERFORMANCE TREND");
  lines.push("------------------");
  const perfStatus =
    comparison.performanceTrend === "improving"
      ? "[Improving]"
      : comparison.performanceTrend === "degrading"
        ? "[Degrading]"
        : "[Stable]";
  lines.push(perfStatus);
  lines.push(
    `  Average Duration: ${comparison.avgDuration}ms (min: ${comparison.minDuration}ms, max: ${comparison.maxDuration}ms)`
  );
  lines.push("");

  lines.push("ERROR TREND");
  lines.push("-----------");
  const errStatus =
    comparison.errorTrend === "improving"
      ? "[Improving]"
      : comparison.errorTrend === "degrading"
        ? "[Degrading]"
        : "[Stable]";
  lines.push(errStatus);
  lines.push(
    `  Average Error Count: ${comparison.avgErrorCount.toFixed(1)} (max: ${comparison.maxErrorCount})`
  );
  lines.push("");

  lines.push("STATISTICS");
  lines.push("----------");
  if (comparison.bestPerformer) {
    lines.push(`  Best Performer: ${comparison.bestPerformer}`);
  }
  if (comparison.worstPerformer) {
    lines.push(`  Slowest: ${comparison.worstPerformer}`);
  }
  if (comparison.mostReliable) {
    lines.push(`  Most Reliable: ${comparison.mostReliable}`);
  }
  lines.push("");

  if (comparison.findings.length > 0) {
    lines.push("FINDINGS");
    lines.push("--------");
    lines.push("");
    for (const finding of comparison.findings) {
      lines.push(`  - ${finding}`);
    }
    lines.push("");
  }

  return lines.join("\n");
}

export function formatPerformanceDelta(delta: {
  base: number;
  current: number;
  changePercent: number;
  improved: boolean;
  unit?: string;
}): string {
  const status = delta.improved ? "[OK]" : "[WARNING]";
  const unit = delta.unit || "";
  const sign = delta.changePercent > 0 ? "+" : "";
  return `${status} ${delta.base}${unit} -> ${delta.current}${unit} (${sign}${delta.changePercent}%)`;
}

export function formatErrorComparison(errors: any): string {
  const lines: string[] = [];

  lines.push("ERROR BREAKDOWN");
  lines.push("");

  if (errors.newErrors?.length > 0) {
    lines.push(`NEW ERRORS (${errors.newErrors.length}):`);
    for (const err of errors.newErrors) {
      lines.push(`  - ${err.type}: ${err.count} occurrence(s)`);
    }
  }

  if (errors.fixedErrors?.length > 0) {
    lines.push(`FIXED ERRORS (${errors.fixedErrors.length}):`);
    for (const err of errors.fixedErrors) {
      lines.push(`  - ${err.type}`);
    }
  }

  if (errors.commonErrors?.length > 0) {
    lines.push(`COMMON ERRORS (${errors.commonErrors.length}):`);
    for (const err of errors.commonErrors) {
      lines.push(`  - ${err.type}: ${err.count} occurrence(s)`);
    }
  }

  return lines.join("\n");
}

export function formatRecommendations(recommendations: string[]): string {
  if (recommendations.length === 0) return "";

  const lines: string[] = [];
  lines.push("RECOMMENDATIONS");
  lines.push("");

  for (const rec of recommendations) {
    lines.push(`  - ${rec}`);
  }

  return lines.join("\n");
}
