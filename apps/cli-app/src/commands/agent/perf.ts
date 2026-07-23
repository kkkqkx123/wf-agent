/**
 * Agent Performance Analysis Command Group
 * Subcommands for analyzing agent loop execution performance
 */

import { Command } from "commander";
import { AgentPerformanceAnalysisAdapter } from "../../adapters/agent-performance-analysis-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const output = getOutput();
const router = getRouter();

/**
 * Create Agent Performance Analysis Commands
 */
export function createAgentPerfCommands(): Command {
  const perfCmd = new Command("perf").description("Analyze agent loop execution performance");

  // agent perf profile <execution-id>
  perfCmd
    .command("profile <execution-id>")
    .description("Get full performance profile for an agent loop execution")
    .action(async (executionId: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Analyzing performance for execution "${executionId}"...`);
        const adapter = new AgentPerformanceAnalysisAdapter();
        const result = await adapter.analyzePerformance(executionId);

        router.render(result, {
          type: "detail",
          entity: "agent-perf-profile",
          format: () => {
            if (!result) {
              return `No performance data found for execution "${executionId}".`;
            }
            const lines: string[] = [];
            lines.push(`Performance Profile for "${executionId}":`);
            lines.push(`  Status: ${result.status}`);
            lines.push(`  Total iterations: ${result.totalIterations}`);
            lines.push(`  Total tool calls: ${result.totalToolCalls}`);
            lines.push(`  Total duration: ${result.totalDuration ?? "N/A"}ms`);
            lines.push(`  Performance tier: ${result.performanceTier}`);
            lines.push(`  Success rate: ${(result.summary.successRate * 100).toFixed(1)}%`);
            lines.push(`  Avg iteration duration: ${result.summary.avgIterationDuration.toFixed(1)}ms`);
            lines.push(`  Ops/sec: ${result.summary.operationsPerSecond.toFixed(1)}`);
            if (result.bottlenecks.length > 0) {
              lines.push("  Bottlenecks:");
              result.bottlenecks.forEach((b) => {
                lines.push(`    [${b.severity}] ${b.type} at ${b.location} — ${b.duration}ms (${b.percentage}%)`);
              });
            }
            return lines.join("\n");
          },
          metadata: { executionId },
        });
      } catch (error) {
        handleError(error, { operation: "agent-perf-profile", additionalInfo: { executionId } });
      }
    });

  // agent perf bottleneck <execution-id>
  perfCmd
    .command("bottleneck <execution-id>")
    .description("Identify performance bottlenecks in an agent loop execution")
    .action(async (executionId: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Identifying bottlenecks for execution "${executionId}"...`);
        const adapter = new AgentPerformanceAnalysisAdapter();
        const result = await adapter.analyzePerformance(executionId);

        router.render(result, {
          type: "detail",
          entity: "agent-perf-bottleneck",
          format: () => {
            if (!result || result.bottlenecks.length === 0) {
              return `No bottlenecks identified for execution "${executionId}".`;
            }
            const lines: string[] = [];
            lines.push(`Bottlenecks in "${executionId}":`);
            result.bottlenecks
              .sort((a, b) => b.percentage - a.percentage)
              .forEach((b) => {
                lines.push(`  [${b.severity.toUpperCase()}] ${b.type}: ${b.duration}ms (${b.percentage}% of total)`);
                lines.push(`    Location: ${b.location}`);
              });
            return lines.join("\n");
          },
          metadata: { executionId },
        });
      } catch (error) {
        handleError(error, { operation: "agent-perf-bottleneck", additionalInfo: { executionId } });
      }
    });

  // agent perf trend <execution-id>
  perfCmd
    .command("trend <execution-id>")
    .description("Get performance trend and iteration comparison for an agent loop")
    .action(async (executionId: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Computing performance trend for execution "${executionId}"...`);
        const adapter = new AgentPerformanceAnalysisAdapter();
        const result = await adapter.getIterationComparison(executionId);

        router.render(result, {
          type: "detail",
          entity: "agent-perf-trend",
          format: () => {
            const lines: string[] = [];
            lines.push(`Performance Trend for "${executionId}":`);
            lines.push(`  Total iterations: ${result.totalIterations}`);
            lines.push(`  Average duration: ${result.averageDuration.toFixed(1)}ms`);
            lines.push(`  Variance: ${result.variance.toFixed(1)}`);
            lines.push(`  Trend: ${result.trend}`);
            if (result.fastestIteration) {
              lines.push(`  Fastest iteration: #${result.fastestIteration.iteration} (${result.fastestIteration.duration}ms)`);
            }
            if (result.slowestIteration) {
              lines.push(`  Slowest iteration: #${result.slowestIteration.iteration} (${result.slowestIteration.duration}ms)`);
            }
            return lines.join("\n");
          },
          metadata: { executionId },
        });
      } catch (error) {
        handleError(error, { operation: "agent-perf-trend", additionalInfo: { executionId } });
      }
    });

  return perfCmd;
}
