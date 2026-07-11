/**
 * Iteration Formatters
 * Format iteration analysis data for CLI display
 */

import type {
  ExtendedIterationDetail,
  ExtendedIterationHistorySummary,
  DecisionAnalysis,
  ExecutionPathAnalysis,
  DecisionOutcome,
  ErrorContextRecord,
  IterationSystemMetrics,
  IterationLLMMetrics,
} from "@wf-agent/sdk/api";

/**
 * Format extended iteration detail
 */
export function formatIterationDetail(
  detail: ExtendedIterationDetail,
): string {
  const lines: string[] = [];

  lines.push(`📋 Iteration #${detail.iteration ?? 0}`);
  lines.push("─".repeat(50));

  // Basic Info
  if (detail.startTime) {
    const date = new Date(detail.startTime);
    lines.push(`Started: ${date.toISOString()}`);
  }
  if (detail.endTime) {
    lines.push(`Duration: ${((detail.endTime - (detail.startTime ?? 0)) / 1000).toFixed(2)}s`);
  }

  // Decisions
  if (detail.decisions && detail.decisions.length > 0) {
    lines.push("\n🤔 Decisions:");
    detail.decisions.forEach((decision, i) => {
      lines.push(`  ${i + 1}. ${decision.description}`);
      lines.push(`     Selected: ${decision.selectedOption}`);
      if (decision.confidence) {
        lines.push(`     Confidence: ${(decision.confidence * 100).toFixed(1)}%`);
      }
    });
  }

  // Execution Path
  if (detail.executionPath) {
    lines.push("\n🔄 Execution Path:");
    lines.push(`   Path: ${detail.executionPath.description}`);
    lines.push(`   Steps: ${detail.executionPath.steps.length}`);
    if (detail.executionPath.isOptimal) {
      lines.push("   ✓ Optimal path");
    }
  }

  // Tool Calls
  if (detail.toolCalls && detail.toolCalls.length > 0) {
    lines.push(`\n🔧 Tool Calls (${detail.toolCalls.length}):`);
    detail.toolCalls.slice(0, 5).forEach((call, i) => {
      const toolName = (call as any).toolName ?? "unknown";
      const status = (call as any).status ?? "executed";
      lines.push(`   ${i + 1}. ${toolName} [${status}]`);
    });
    if (detail.toolCalls.length > 5) {
      lines.push(`   ... and ${detail.toolCalls.length - 5} more`);
    }
  }

  // Metrics
  if (detail.systemMetrics) {
    lines.push("\n⚙️  System Metrics:");
    lines.push(
      `   Duration: ${detail.systemMetrics.durationMs.toFixed(0)}ms`,
    );
    lines.push(
      `   CPU Time: ${detail.systemMetrics.cpuTimeMs.toFixed(0)}ms`,
    );
    lines.push(`   Memory Peak: ${detail.systemMetrics.memoryPeakMb.toFixed(1)}MB`);
  }

  // LLM Metrics
  if (detail.llmMetrics && detail.llmMetrics.length > 0) {
    lines.push("\n🤖 LLM Metrics:");
    detail.llmMetrics.forEach((metric, i) => {
      lines.push(`   Call #${i + 1}:`);
      lines.push(`     Model: ${metric.model}`);
      lines.push(
        `     Tokens: ${metric.inputTokens} in, ${metric.outputTokens} out`,
      );
      lines.push(`     Cost: $${metric.costUsd.toFixed(6)}`);
      lines.push(`     Duration: ${metric.durationMs}ms`);
    });
  }

  // Quality & Errors
  if (detail.qualityScore !== undefined) {
    const score = (detail.qualityScore * 100).toFixed(0);
    lines.push(`\n📊 Quality Score: ${score}%`);
  }

  if (detail.errors && detail.errors.length > 0) {
    lines.push(`\n⚠️  Errors (${detail.errors.length}):`);
    detail.errors.forEach((error, i) => {
      lines.push(`   ${i + 1}. [${error.errorType}] ${error.errorMessage}`);
      if (error.recoveryAction) {
        lines.push(`      Recovery: ${error.recoveryAction}`);
      }
    });
  }

  if (detail.requiresRevision) {
    lines.push("\n🔴 Requires Revision");
  }

  // Optimization Opportunities
  if (
    detail.optimizationOpportunities &&
    detail.optimizationOpportunities.length > 0
  ) {
    lines.push("\n💡 Optimization Opportunities:");
    detail.optimizationOpportunities.forEach((opp, i) => {
      lines.push(`   ${i + 1}. ${opp}`);
    });
  }

  return lines.join("\n");
}

/**
 * Format iteration history summary
 */
export function formatIterationHistorySummary(
  summary: ExtendedIterationHistorySummary,
): string {
  const lines: string[] = [];

  lines.push("📊 Agent Loop Iteration Summary");
  lines.push("─".repeat(50));

  lines.push(`Total Iterations: ${summary.totalIterations}`);
  lines.push(`Total Duration: ${(summary.totalDuration / 1000).toFixed(2)}s`);
  lines.push(
    `Average Duration: ${summary.averageDuration.toFixed(0)}ms per iteration`,
  );

  if (summary.totalToolCalls !== undefined) {
    lines.push(`Total Tool Calls: ${summary.totalToolCalls}`);
  }

  // Enhanced Metrics
  if (summary.averageQualityScore !== undefined) {
    const score = (summary.averageQualityScore * 100).toFixed(1);
    lines.push(`\n📈 Quality Metrics:`);
    lines.push(`   Average Quality Score: ${score}%`);
  }

  if (summary.revisionsRequired !== undefined) {
    lines.push(
      `   Iterations Requiring Revision: ${summary.revisionsRequired}`,
    );
  }

  if (summary.totalErrors !== undefined) {
    lines.push(`\n⚠️  Error Analysis:`);
    lines.push(`   Total Errors: ${summary.totalErrors}`);
    if (summary.errorRate !== undefined) {
      lines.push(`   Error Rate: ${(summary.errorRate * 100).toFixed(1)}%`);
    }
    if (summary.totalRecoveryActions !== undefined) {
      lines.push(`   Recovery Actions: ${summary.totalRecoveryActions}`);
    }
    if (summary.recoverySuccessRate !== undefined) {
      lines.push(
        `   Recovery Success Rate: ${(summary.recoverySuccessRate * 100).toFixed(1)}%`,
      );
    }
  }

  // Decision Analysis
  if (summary.totalDecisions !== undefined) {
    lines.push(`\n🤔 Decision Analysis:`);
    lines.push(`   Total Decisions: ${summary.totalDecisions}`);
  }

  // Resource Metrics
  if (summary.totalLLMTokens !== undefined) {
    lines.push(`\n💰 Resource Metrics:`);
    lines.push(`   Total LLM Tokens: ${summary.totalLLMTokens.toLocaleString()}`);
    if (summary.averageLLMTokensPerIteration !== undefined) {
      lines.push(
        `   Average Tokens/Iteration: ${summary.averageLLMTokensPerIteration.toFixed(0)}`,
      );
    }
  }

  if (summary.peakMemoryUsage !== undefined) {
    lines.push(
      `   Peak Memory: ${(summary.peakMemoryUsage / 1024 / 1024).toFixed(1)}MB`,
    );
  }

  if (summary.totalAPICalls !== undefined) {
    lines.push(`   Total API Calls: ${summary.totalAPICalls}`);
  }

  // Optimization Summary
  if (
    summary.topOptimizations &&
    summary.topOptimizations.length > 0
  ) {
    lines.push(`\n💡 Top Optimization Opportunities:`);
    summary.topOptimizations.slice(0, 3).forEach((opt, i) => {
      lines.push(`   ${i + 1}. ${opt.opportunity} (${opt.frequency}x)`);
      if (opt.estimatedImpact) {
        lines.push(`      Impact: ${opt.estimatedImpact}`);
      }
    });
  }

  // Performance Analysis
  if (summary.slowestTools && summary.slowestTools.length > 0) {
    lines.push(`\n🐢 Slowest Tools:`);
    summary.slowestTools.slice(0, 3).forEach((tool, i) => {
      lines.push(
        `   ${i + 1}. ${tool.toolName}: ${tool.averageTime.toFixed(0)}ms avg`,
      );
    });
  }

  if (summary.frequentTools && summary.frequentTools.length > 0) {
    lines.push(`\n🔧 Most Frequent Tools:`);
    summary.frequentTools.slice(0, 5).forEach((tool, i) => {
      lines.push(`   ${i + 1}. ${tool.toolName}: ${tool.count} calls`);
    });
  }

  return lines.join("\n");
}

/**
 * Format decision analysis
 */
export function formatDecisionAnalysis(
  analysis: DecisionAnalysis,
): string {
  const lines: string[] = [];

  lines.push("🤔 Decision Analysis");
  lines.push("─".repeat(50));

  lines.push(`Total Decisions: ${analysis.totalDecisions}`);
  lines.push(
    `Average Confidence: ${(analysis.averageConfidence * 100).toFixed(1)}%`,
  );
  lines.push(`Decision Reversals: ${analysis.reversalCount}`);

  if (Object.keys(analysis.decisionTypes).length > 0) {
    lines.push("\nDecision Types:");
    Object.entries(analysis.decisionTypes).forEach(([type, count]) => {
      lines.push(`  • ${type}: ${count}`);
    });
  }

  if (analysis.frequentDecisions.length > 0) {
    lines.push("\nMost Common Decisions:");
    analysis.frequentDecisions.slice(0, 5).forEach((decision, i) => {
      lines.push(`  ${i + 1}. ${decision.description} (${decision.frequency}x)`);
    });
  }

  return lines.join("\n");
}

/**
 * Format execution path analysis
 */
export function formatExecutionPathAnalysis(
  analysis: ExecutionPathAnalysis,
): string {
  const lines: string[] = [];

  lines.push("🔄 Execution Path Analysis");
  lines.push("─".repeat(50));

  lines.push(`Total Unique Paths: ${analysis.totalPaths}`);
  lines.push(`Average Path Length: ${analysis.averagePathLength.toFixed(1)} steps`);
  lines.push(
    `Path Complexity: ${(analysis.complexityScore * 100).toFixed(0)}%`,
  );
  lines.push(`Optimal Paths: ${analysis.optimalPathCount}`);

  if (analysis.mostCommonPath) {
    lines.push(`Most Common Path: ${analysis.mostCommonPath}`);
  }

  return lines.join("\n");
}

/**
 * Format iteration summary as simple list
 */
export function formatIterationTable(
  iterations: ExtendedIterationDetail[],
): string {
  if (iterations.length === 0) {
    return "No iterations found.";
  }

  const lines: string[] = [];
  lines.push("Iterations:");
  lines.push("─".repeat(80));

  iterations.forEach((iter) => {
    const num = iter.iteration ?? "N/A";
    const duration = iter.systemMetrics?.durationMs.toFixed(0) ?? "N/A";
    const quality = iter.qualityScore
      ? `${(iter.qualityScore * 100).toFixed(0)}%`
      : "N/A";
    const toolCalls = iter.toolCalls?.length ?? 0;
    const errors = iter.errors?.length ?? 0;

    lines.push(
      `#${num.toString().padEnd(4)} | ${duration.padEnd(8)}ms | Quality: ${quality.padEnd(5)} | Tools: ${toolCalls.toString().padEnd(3)} | Errors: ${errors}`,
    );
  });

  return lines.join("\n");
}

/**
 * Format decision outcomes
 */
export function formatDecisionOutcomes(
  decisions: DecisionOutcome[],
): string {
  if (decisions.length === 0) {
    return "No decisions made.";
  }

  const lines: string[] = [];
  lines.push("🤔 Decisions Made:");
  lines.push("─".repeat(50));

  decisions.forEach((decision, i) => {
    lines.push(`\n${i + 1}. ${decision.description}`);
    lines.push(
      `   Selected: ${decision.selectedOption}`,
    );
    lines.push(`   Options: ${decision.availableOptions.join(", ")}`);
    if (decision.reasoning) {
      lines.push(`   Reasoning: ${decision.reasoning}`);
    }
    if (decision.confidence) {
      lines.push(
        `   Confidence: ${(decision.confidence * 100).toFixed(1)}%`,
      );
    }
  });

  return lines.join("\n");
}

/**
 * Format error context
 */
export function formatErrorContexts(
  errors: ErrorContextRecord[],
): string {
  if (errors.length === 0) {
    return "No errors occurred.";
  }

  const lines: string[] = [];
  lines.push(`⚠️  Errors (${errors.length}):`);
  lines.push("─".repeat(50));

  errors.forEach((error, i) => {
    lines.push(`\n${i + 1}. [${error.errorType}] ${error.errorMessage}`);
    if (error.toolCallId) {
      lines.push(`   Tool Call: ${error.toolCallId}`);
    }
    if (error.recoveryAction) {
      lines.push(`   Recovery Action: ${error.recoveryAction}`);
      lines.push(
        `   Recovery Success: ${error.recoverySuccess ? "✓ Yes" : "✗ No"}`,
      );
    }
  });

  return lines.join("\n");
}

/**
 * Format system metrics
 */
export function formatSystemMetrics(
  metrics: IterationSystemMetrics,
): string {
  const lines: string[] = [];

  lines.push("⚙️  System Metrics");
  lines.push("─".repeat(50));
  lines.push(`Iteration: #${metrics.iteration}`);
  lines.push(`Timestamp: ${new Date(metrics.timestamp).toISOString()}`);
  lines.push(`Total Duration: ${metrics.durationMs}ms`);
  lines.push(`CPU Time: ${metrics.cpuTimeMs}ms`);
  lines.push(`Memory Peak: ${metrics.memoryPeakMb.toFixed(1)}MB`);

  if (metrics.diskIoBytes !== undefined) {
    lines.push(`Disk I/O: ${(metrics.diskIoBytes / 1024).toFixed(2)}KB`);
  }

  if (metrics.networkIoBytes !== undefined) {
    lines.push(`Network I/O: ${(metrics.networkIoBytes / 1024).toFixed(2)}KB`);
  }

  return lines.join("\n");
}

/**
 * Format LLM metrics
 */
export function formatLLMMetrics(metrics: IterationLLMMetrics[]): string {
  if (metrics.length === 0) {
    return "No LLM metrics available.";
  }

  const lines: string[] = [];
  lines.push("🤖 LLM Metrics");
  lines.push("─".repeat(50));

  metrics.forEach((metric, i) => {
    lines.push(`\nCall #${i + 1}:`);
    lines.push(`  Model: ${metric.model}`);
    lines.push(`  Input Tokens: ${metric.inputTokens.toLocaleString()}`);
    lines.push(`  Output Tokens: ${metric.outputTokens.toLocaleString()}`);
    lines.push(`  Total Tokens: ${(metric.inputTokens + metric.outputTokens).toLocaleString()}`);
    lines.push(`  Cost: $${metric.costUsd.toFixed(6)}`);
    lines.push(`  Duration: ${metric.durationMs}ms`);

    if (metric.cacheReadTokens || metric.cacheCreationTokens) {
      if (metric.cacheReadTokens) {
        lines.push(`  Cache Read: ${metric.cacheReadTokens}`);
      }
      if (metric.cacheCreationTokens) {
        lines.push(`  Cache Creation: ${metric.cacheCreationTokens}`);
      }
    }
  });

  const totalCost = metrics.reduce((sum, m) => sum + m.costUsd, 0);
  const totalTokens = metrics.reduce(
    (sum, m) => sum + m.inputTokens + m.outputTokens,
    0,
  );
  lines.push(`\nTotal Cost: $${totalCost.toFixed(6)}`);
  lines.push(`Total Tokens: ${totalTokens.toLocaleString()}`);

  return lines.join("\n");
}
