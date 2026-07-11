/**
 * Execution Comparison Adapter
 * Wraps ExecutionComparisonAPI for CLI usage
 */

import { BaseAdapter } from "./base-adapter.js";
import type { ID } from "@wf-agent/types";

export interface ExecutionComparison {
  executionId1: ID;
  executionId2: ID;
  performance: {
    duration: {
      base: number;
      current: number;
      change: number;
      changePercent: number;
      improved: boolean;
      unit: "ms";
    };
    throughput: {
      base: number;
      current: number;
      change: number;
      changePercent: number;
      improved: boolean;
      unit: "ops/sec";
    };
    successRate: {
      base: number;
      current: number;
      change: number;
      changePercent: number;
      improved: boolean;
      unit: "%";
    };
    verdict: "exec1_better" | "exec2_better" | "equal";
  };
  errors: {
    totalErrorCount: {
      base: number;
      current: number;
      change: number;
      changePercent: number;
      improved: boolean;
    };
    newErrors: Array<{
      type: string;
      count: number;
      percentage: number;
      samples: string[];
    }>;
    fixedErrors: Array<{
      type: string;
      count: number;
      percentage: number;
      samples: string[];
    }>;
    commonErrors: Array<{
      type: string;
      count: number;
      percentage: number;
      samples: string[];
    }>;
  };
  interruptions: {
    totalCount: {
      base: number;
      current: number;
      change: number;
      changePercent: number;
      improved: boolean;
    };
    recoveryRate: {
      base: number;
      current: number;
      change: number;
      changePercent: number;
      improved: boolean;
    };
    averageDuration: {
      base: number;
      current: number;
      change: number;
      changePercent: number;
      improved: boolean;
    };
    byType?: Record<
      string,
      {
        base: number;
        current: number;
        change: number;
        changePercent: number;
        improved: boolean;
      }
    >;
  };
  findings: string[];
  recommendations: string[];
}

export interface RangeComparison {
  executionIds: ID[];
  performanceTrend: "improving" | "degrading" | "stable";
  errorTrend: "improving" | "degrading" | "stable";
  avgDuration: number;
  minDuration: number;
  maxDuration: number;
  avgErrorCount: number;
  maxErrorCount: number;
  bestPerformer?: ID;
  worstPerformer?: ID;
  mostReliable?: ID;
  findings: string[];
}

export class ExecutionComparisonAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  /**
   * Compare two executions
   */
  async compareExecutions(
    exec1Id: ID,
    exec2Id: ID
  ): Promise<ExecutionComparison> {
    return this.executeWithErrorHandling(async () => {
      const factory = this.sdk.getFactory();
      const deps = factory?.getDependencies?.();
      if (!deps) throw new Error("SDK dependencies not available");

      const perfApi = deps.getAgentPerformanceAnalysisAPI?.();
      const errorApi = deps.getAgentErrorAnalysisAPI?.();

      if (!perfApi || !errorApi) {
        throw new Error("Required APIs not available");
      }

      const perf1 = await perfApi.analyzePerformance(exec1Id);
      const perf2 = await perfApi.analyzePerformance(exec2Id);
      const err1 = await errorApi.getAdvancedErrorAnalysis(exec1Id);
      const err2 = await errorApi.getAdvancedErrorAnalysis(exec2Id);

      if (!perf1 || !perf2) throw new Error("Failed to fetch performance data");

      const comparison = this.buildComparison(perf1, perf2, err1, err2);
      this.logOperation(
        `Compared executions: ${exec1Id} vs ${exec2Id}`
      );
      return comparison;
    }, "Compare executions");
  }

  /**
   * Compare multiple executions over time
   */
  async compareRange(executionIds: ID[]): Promise<RangeComparison> {
    return this.executeWithErrorHandling(async () => {
      if (executionIds.length < 2) {
        throw new Error("Need at least 2 executions to compare");
      }

      const factory = this.sdk.getFactory();
      const deps = factory?.getDependencies?.();
      if (!deps) throw new Error("SDK dependencies not available");

      const perfApi = deps.getAgentPerformanceAnalysisAPI?.();
      if (!perfApi) throw new Error("Performance API not available");

      const performances = await Promise.all(
        executionIds.map((id) => perfApi.analyzePerformance(id))
      );

      const comparison = this.buildRangeComparison(
        executionIds,
        performances.filter(Boolean)
      );
      this.logOperation(
        `Compared ${executionIds.length} executions for trend analysis`
      );
      return comparison;
    }, "Compare execution range");
  }

  /**
   * Analyze performance trend
   */
  async analyzePerformanceTrend(
    executionIds: ID[]
  ): Promise<{
    trend: "improving" | "degrading" | "stable";
    avgDuration: number;
    avgErrors: number;
  }> {
    return this.executeWithErrorHandling(async () => {
      const comparison = await this.compareRange(executionIds);
      return {
        trend: comparison.performanceTrend,
        avgDuration: comparison.avgDuration,
        avgErrors: comparison.avgErrorCount,
      };
    }, "Analyze performance trend");
  }

  private buildComparison(
    perf1: any,
    perf2: any,
    err1: any,
    err2: any
  ): ExecutionComparison {
    const dur1 = perf1.totalDuration || 0;
    const dur2 = perf2.totalDuration || 0;
    const durationDelta = dur2 - dur1;
    const durationPercent =
      dur1 > 0 ? ((durationDelta / dur1) * 100).toFixed(2) : "0";

    const throughput1 = (perf1.totalToolCalls || 0) / (dur1 / 1000) || 0;
    const throughput2 = (perf2.totalToolCalls || 0) / (dur2 / 1000) || 0;
    const throughputDelta = throughput2 - throughput1;
    const throughputPercent =
      throughput1 > 0
        ? ((throughputDelta / throughput1) * 100).toFixed(2)
        : "0";

    const successRate1 =
      ((perf1.successfulNodes || 0) / (perf1.totalNodes || 1)) * 100;
    const successRate2 =
      ((perf2.successfulNodes || 0) / (perf2.totalNodes || 1)) * 100;
    const successRateDelta = successRate2 - successRate1;

    const errorCount1 = (err1?.errors || []).length;
    const errorCount2 = (err2?.errors || []).length;
    const errorDelta = errorCount2 - errorCount1;

    return {
      executionId1: perf1.executionId,
      executionId2: perf2.executionId,
      performance: {
        duration: {
          base: dur1,
          current: dur2,
          change: durationDelta,
          changePercent: Number(durationPercent),
          improved: durationDelta < 0,
          unit: "ms",
        },
        throughput: {
          base: Math.round(throughput1 * 100) / 100,
          current: Math.round(throughput2 * 100) / 100,
          change: Math.round(throughputDelta * 100) / 100,
          changePercent: Number(throughputPercent),
          improved: throughputDelta > 0,
          unit: "ops/sec",
        },
        successRate: {
          base: Math.round(successRate1 * 100) / 100,
          current: Math.round(successRate2 * 100) / 100,
          change: Math.round(successRateDelta * 100) / 100,
          changePercent: Number(successRateDelta.toFixed(2)),
          improved: successRateDelta > 0,
          unit: "%",
        },
        verdict:
          durationDelta < 0 && errorDelta <= 0
            ? "exec2_better"
            : durationDelta > 0 && errorDelta > 0
              ? "exec1_better"
              : "equal",
      },
      errors: {
        totalErrorCount: {
          base: errorCount1,
          current: errorCount2,
          change: errorDelta,
          changePercent:
            errorCount1 > 0
              ? Number(((errorDelta / errorCount1) * 100).toFixed(2))
              : 0,
          improved: errorDelta <= 0,
        },
        newErrors: this.extractNewErrors(err1, err2),
        fixedErrors: this.extractFixedErrors(err1, err2),
        commonErrors: this.extractCommonErrors(err1, err2),
      },
      interruptions: {
        totalCount: {
          base: (perf1.interruptions || []).length,
          current: (perf2.interruptions || []).length,
          change:
            (perf2.interruptions || []).length -
            (perf1.interruptions || []).length,
          changePercent:
            (perf1.interruptions || []).length > 0
              ? Number(
                  (
                    (((perf2.interruptions || []).length -
                      (perf1.interruptions || []).length) /
                      (perf1.interruptions || []).length) *
                    100
                  ).toFixed(2)
                )
              : 0,
          improved: (perf2.interruptions || []).length < (perf1.interruptions || []).length,
        },
        recoveryRate: {
          base: (perf1.recoveryRate || 0) * 100,
          current: (perf2.recoveryRate || 0) * 100,
          change: ((perf2.recoveryRate || 0) - (perf1.recoveryRate || 0)) * 100,
          changePercent:
            perf1.recoveryRate > 0
              ? Number(
                  (
                    (((perf2.recoveryRate || 0) - (perf1.recoveryRate || 0)) /
                      (perf1.recoveryRate || 0.001)) *
                    100
                  ).toFixed(2)
                )
              : 0,
          improved: (perf2.recoveryRate || 0) > (perf1.recoveryRate || 0),
        },
        averageDuration: {
          base: (perf1.avgInterruptionDuration || 0),
          current: (perf2.avgInterruptionDuration || 0),
          change: (perf2.avgInterruptionDuration || 0) - (perf1.avgInterruptionDuration || 0),
          changePercent:
            perf1.avgInterruptionDuration > 0
              ? Number(
                  (
                    (((perf2.avgInterruptionDuration || 0) -
                      (perf1.avgInterruptionDuration || 0)) /
                      (perf1.avgInterruptionDuration || 0.001)) *
                    100
                  ).toFixed(2)
                )
              : 0,
          improved: (perf2.avgInterruptionDuration || 0) < (perf1.avgInterruptionDuration || 0),
        },
      },
      findings: this.generateFindings(perf1, perf2, err1, err2),
      recommendations: this.generateRecommendations(perf1, perf2, err1, err2),
    };
  }

  private buildRangeComparison(
    executionIds: ID[],
    performances: any[]
  ): RangeComparison {
    const durations = performances
      .map((p) => p.totalDuration || 0);
    const errorCounts = performances.map((p) => (p.errors || []).length);

    const avgDuration =
      durations.length > 0
        ? durations.reduce((a, b) => a + b, 0) / durations.length
        : 0;
    const minDuration = Math.min(...durations);
    const maxDuration = Math.max(...durations);

    const avgErrorCount =
      errorCounts.length > 0
        ? errorCounts.reduce((a, b) => a + b, 0) / errorCounts.length
        : 0;
    const maxErrorCount = Math.max(...errorCounts);

    const bestPerformer = executionIds[
      durations.indexOf(minDuration)
    ];
    const worstPerformer = executionIds[
      durations.indexOf(maxDuration)
    ];
    const mostReliable =
      executionIds[
      errorCounts.indexOf(Math.min(...errorCounts))
    ];

    const performanceTrend = this.calculateTrend(durations);
    const errorTrend = this.calculateTrend(errorCounts);

    return {
      executionIds,
      performanceTrend,
      errorTrend,
      avgDuration: Math.round(avgDuration),
      minDuration: Math.round(minDuration),
      maxDuration: Math.round(maxDuration),
      avgErrorCount: Math.round(avgErrorCount * 100) / 100,
      maxErrorCount,
      bestPerformer,
      worstPerformer,
      mostReliable,
      findings: this.generateRangeFindings(
        performances,
        performanceTrend,
        errorTrend
      ),
    };
  }

  private calculateTrend(
    values: number[]
  ): "improving" | "degrading" | "stable" {
    if (values.length < 2) return "stable";

    const first = values.slice(0, Math.ceil(values.length / 2));
    const last = values.slice(Math.ceil(values.length / 2));

    const avgFirst = first.reduce((a, b) => a + b, 0) / first.length;
    const avgLast = last.reduce((a, b) => a + b, 0) / last.length;

    const change = avgLast - avgFirst;
    const threshold = avgFirst * 0.1;

    if (Math.abs(change) <= threshold) return "stable";
    return change < 0 ? "improving" : "degrading";
  }

  private extractNewErrors(err1: any, err2: any): any[] {
    const errors1 = new Set(
      (err1?.errors || []).map((e: any) => e.type)
    );
    return (err2?.errors || [])
      .filter((e: any) => !errors1.has(e.type))
      .slice(0, 5)
      .map((e: any) => ({
        type: e.type,
        count: 1,
        percentage: 100,
        samples: [e.message || ""],
      }));
  }

  private extractFixedErrors(err1: any, err2: any): any[] {
    const errors2 = new Set(
      (err2?.errors || []).map((e: any) => e.type)
    );
    return (err1?.errors || [])
      .filter((e: any) => !errors2.has(e.type))
      .slice(0, 5)
      .map((e: any) => ({
        type: e.type,
        count: 1,
        percentage: 100,
        samples: [e.message || ""],
      }));
  }

  private extractCommonErrors(err1: any, err2: any): any[] {
    const errors1 = new Map(
      (err1?.errors || []).map((e: any) => [e.type, e])
    );
    return (err2?.errors || [])
      .filter((e: any) => errors1.has(e.type))
      .slice(0, 5)
      .map((e: any) => ({
        type: e.type,
        count: 1,
        percentage: 50,
        samples: [e.message || ""],
      }));
  }

  private generateFindings(
    perf1: any,
    perf2: any,
    err1: any,
    err2: any
  ): string[] {
    const findings: string[] = [];

    if (perf2.totalDuration < perf1.totalDuration) {
      const improvement = (
        ((perf1.totalDuration - perf2.totalDuration) /
          perf1.totalDuration) *
        100
      ).toFixed(1);
      findings.push(
        `⚡ Performance improved by ${improvement}% (execution 2 is faster)`
      );
    } else if (perf2.totalDuration > perf1.totalDuration) {
      const degradation = (
        ((perf2.totalDuration - perf1.totalDuration) /
          perf1.totalDuration) *
        100
      ).toFixed(1);
      findings.push(
        `📉 Performance degraded by ${degradation}% (execution 2 is slower)`
      );
    }

    const errorCount2 = (err2?.errors || []).length;
    const errorCount1 = (err1?.errors || []).length;
    if (errorCount2 < errorCount1) {
      findings.push(
        `✅ Error count reduced by ${errorCount1 - errorCount2} errors`
      );
    } else if (errorCount2 > errorCount1) {
      findings.push(
        `⚠️ Error count increased by ${errorCount2 - errorCount1} errors`
      );
    }

    return findings;
  }

  private generateRecommendations(
    perf1: any,
    perf2: any,
    err1: any,
    err2: any
  ): string[] {
    const recommendations: string[] = [];

    if (perf2.totalDuration > perf1.totalDuration) {
      recommendations.push(
        "Consider profiling to identify performance bottlenecks"
      );
    }

    if ((err2?.errors || []).length > (err1?.errors || []).length) {
      recommendations.push(
        "Review recent changes that may have introduced new error types"
      );
    }

    if ((perf2.successfulNodes || 0) < (perf1.successfulNodes || 0)) {
      recommendations.push(
        "Check node configurations and error handling"
      );
    }

    return recommendations;
  }

  private generateRangeFindings(
    performances: any[],
    performanceTrend: string,
    errorTrend: string
  ): string[] {
    const findings: string[] = [];

    if (performanceTrend === "improving") {
      findings.push("📈 Performance is improving over time");
    } else if (performanceTrend === "degrading") {
      findings.push("📉 Performance is degrading over time");
    } else {
      findings.push("➡️  Performance is stable");
    }

    if (errorTrend === "improving") {
      findings.push("✅ Error rate is improving");
    } else if (errorTrend === "degrading") {
      findings.push("⚠️ Error rate is increasing");
    }

    const variance = this.calculateVariance(
      performances.map((p) => p.totalDuration || 0)
    );
    if (variance > 0.2) {
      findings.push(
        "⚠️ High variance in execution times - may indicate instability"
      );
    }

    return findings;
  }

  private calculateVariance(values: number[]): number {
    if (values.length < 2) return 0;
    const mean = values.reduce((a, b) => a + b, 0) / values.length;
    const variance =
      values.reduce((sum, val) => sum + Math.pow(val - mean, 2), 0) /
      values.length;
    const stdDev = Math.sqrt(variance);
    return stdDev / mean;
  }
}
