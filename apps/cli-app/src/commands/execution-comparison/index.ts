/**
 * Execution Comparison Command
 * Compare two or more workflow executions
 */

import { Command } from "commander";
import { ExecutionComparisonAdapter } from "../../adapters/execution-comparison-adapter.js";
import {
  formatExecutionComparison,
  formatRangeComparison,
} from "../../utils/formatters/comparison-formatters.js";
import { getRouter } from "../../utils/output-router.js";
import type { ID } from "@wf-agent/types";

export function createExecutionComparisonCommand(): Command {
  const cmd = new Command("compare");
  cmd.description("Compare two or more workflow executions");

  cmd
    .command("two <exec1Id> <exec2Id>")
    .description("Compare two executions")
    .option("--json", "Output as JSON")
    .action(async (exec1Id: string, exec2Id: string, options) => {
      const adapter = new ExecutionComparisonAdapter();
      const comparison = await adapter.compareExecutions(
        exec1Id as ID,
        exec2Id as ID
      );

      const router = getRouter();
      if (options.json) {
        router.render(comparison, {
          type: "detail",
          entity: "execution_comparison",
          message: "Execution comparison completed",
        });
      } else {
        router.render(formatExecutionComparison(comparison), {
          type: "detail",
          entity: "execution_comparison",
          format: () => formatExecutionComparison(comparison),
        });
      }
    });

  cmd
    .command("range <execIds...>")
    .description("Compare multiple executions for trend analysis")
    .option("--json", "Output as JSON")
    .action(async (execIds: string[], options) => {
      if (execIds.length < 2) {
        throw new Error("Need at least 2 execution IDs to compare");
      }

      const adapter = new ExecutionComparisonAdapter();
      const comparison = await adapter.compareRange(execIds as ID[]);

      const router = getRouter();
      if (options.json) {
        router.render(comparison, {
          type: "detail",
          entity: "range_comparison",
          message: "Range comparison completed",
        });
      } else {
        router.render(formatRangeComparison(comparison), {
          type: "detail",
          entity: "range_comparison",
          format: () => formatRangeComparison(comparison),
        });
      }
    });

  cmd
    .command("trend <execIds...>")
    .description("Analyze performance trend across executions")
    .option("--json", "Output as JSON")
    .action(async (execIds: string[], options) => {
      if (execIds.length < 2) {
        throw new Error("Need at least 2 execution IDs to analyze");
      }

      const adapter = new ExecutionComparisonAdapter();
      const trend = await adapter.analyzePerformanceTrend(execIds as ID[]);

      const router = getRouter();
      if (options.json) {
        router.render(trend, {
          type: "detail",
          entity: "trend",
          message: "Trend analysis completed",
        });
      } else {
        const lines = [
          "",
          "PERFORMANCE TREND ANALYSIS",
          "==========================",
          "",
          `Trend: ${trend.trend.toUpperCase()}`,
          `Average Duration: ${trend.avgDuration}ms`,
          `Average Errors: ${trend.avgErrors}`,
          "",
        ];
        router.render(lines.join("\n"), {
          type: "detail",
          entity: "trend",
          format: () => lines.join("\n"),
        });
      }
    });

  return cmd;
}
