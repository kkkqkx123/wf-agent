/**
 * Execution Comparison Analysis
 * Compares two or more executions to identify performance, error, and interruption differences
 */

import type { ID } from "@wf-agent/types";

export interface Delta {
  base: number;
  current: number;
  change: number;
  changePercent: number;
  improved: boolean;
}

export interface DurationDelta extends Delta {
  unit: "ms";
}

export interface ThroughputDelta extends Delta {
  unit: "ops/sec";
}

export interface SuccessRateDelta extends Delta {
  unit: "%";
}

export interface ErrorOccurrence {
  type: string;
  count: number;
  percentage: number;
  samples: string[];
}

export interface ErrorComparison {
  totalErrorCount: Delta;
  newErrors: ErrorOccurrence[];
  fixedErrors: ErrorOccurrence[];
  commonErrors: ErrorOccurrence[];
}

export interface InterruptionComparison {
  totalCount: Delta;
  recoveryRate: Delta;
  averageDuration: Delta;
  byType?: Record<string, Delta>;
}

export interface ExecutionComparison {
  executionId1: ID;
  executionId2: ID;
  performance: {
    duration: DurationDelta;
    throughput: ThroughputDelta;
    successRate: SuccessRateDelta;
    verdict: 'exec1_better' | 'exec2_better' | 'equal';
  };
  errors: ErrorComparison;
  interruptions: InterruptionComparison;
  findings: string[];
  recommendations: string[];
}

export interface RangeComparison {
  executionIds: ID[];
  performanceTrend: 'improving' | 'degrading' | 'stable';
  errorTrend: 'improving' | 'degrading' | 'stable';
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

/**
 * Execution Comparison Analysis Implementation
 */
export class ComparisonAnalysis {
  constructor(private sdk: any) {}

  /**
   * Compare two executions
   */
  async compare(exec1Id: ID, exec2Id: ID): Promise<ExecutionComparison> {
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

    return this.buildComparison(perf1, perf2, err1, err2);
  }

  private buildComparison(perf1: any, perf2: any, err1: any, err2: any): ExecutionComparison {
    const dur1 = perf1.totalDuration || 0;
    const dur2 = perf2.totalDuration || 0;
    const durationDelta = dur2 - dur1;

    return {
      executionId1: perf1.executionId,
      executionId2: perf2.executionId,
      performance: {
        duration: {
          base: dur1,
          current: dur2,
          change: durationDelta,
          changePercent: dur1 > 0 ? (durationDelta / dur1) * 100 : 0,
          improved: durationDelta < 0,
          unit: "ms",
        },
        throughput: {
          base: perf1.summary?.operationsPerSecond || 0,
          current: perf2.summary?.operationsPerSecond || 0,
          change: (perf2.summary?.operationsPerSecond || 0) - (perf1.summary?.operationsPerSecond || 0),
          changePercent: 0,
          improved: (perf2.summary?.operationsPerSecond || 0) > (perf1.summary?.operationsPerSecond || 0),
          unit: "ops/sec",
        },
        successRate: {
          base: perf1.summary?.successRate || 0,
          current: perf2.summary?.successRate || 0,
          change: (perf2.summary?.successRate || 0) - (perf1.summary?.successRate || 0),
          changePercent: 0,
          improved: (perf2.summary?.successRate || 0) > (perf1.summary?.successRate || 0),
          unit: "%",
        },
        verdict: durationDelta < 0 ? 'exec2_better' : 'exec1_better',
      },
      errors: {
        totalErrorCount: {
          base: err1?.totalErrors || 0,
          current: err2?.totalErrors || 0,
          change: (err2?.totalErrors || 0) - (err1?.totalErrors || 0),
          changePercent: 0,
          improved: (err2?.totalErrors || 0) < (err1?.totalErrors || 0),
        },
        newErrors: [],
        fixedErrors: [],
        commonErrors: [],
      },
      interruptions: {
        totalCount: { base: 0, current: 0, change: 0, changePercent: 0, improved: false },
        recoveryRate: { base: 0, current: 0, change: 0, changePercent: 0, improved: false },
        averageDuration: { base: 0, current: 0, change: 0, changePercent: 0, improved: false },
      },
      findings: ["Comparison completed"],
      recommendations: ["Continue monitoring"],
    };
  }
}
