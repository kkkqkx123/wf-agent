/**
 * Progress Tracking Command
 * Monitor real-time execution progress
 */

import { Command } from "commander";
import { ProgressTrackingAdapter } from "../../adapters/progress-tracking-adapter.js";
import {
  formatProgressMetrics,
  formatProgressTable,
} from "../../utils/formatters/progress-formatters.js";
import { getRouter } from "../../utils/output-router.js";
import type { ID } from "@wf-agent/types";

export function createProgressCommand(): Command {
  const cmd = new Command("progress");
  cmd.description("Monitor execution progress");

  cmd
    .command("watch <executionId>")
    .description("Watch execution progress in real-time")
    .option("--interval <ms>", "Poll interval in milliseconds", "1000")
    .action(async (executionId: string, options) => {
      const adapter = new ProgressTrackingAdapter();
      const pollInterval = parseInt(options.interval, 10);

      const router = getRouter();

      // Watch progress until completion
      const startTime = Date.now();
      const completion = await adapter.watchProgress(
        executionId as ID,
        (metrics) => {
          router.render(metrics, {
            type: "detail",
            entity: "progress",
            format: () => formatProgressTable(metrics),
          });
        },
        pollInterval
      );

      const totalTime = Date.now() - startTime;
      router.render(completion, {
        type: "detail",
        entity: "progress",
        format: () =>
          `\n[OK] Execution completed in ${(totalTime / 1000).toFixed(1)}s\n\n${formatProgressMetrics(completion)}`,
      });
    });

  cmd
    .command("snapshot <executionId>")
    .description("Get current progress snapshot")
    .option("--json", "Output as JSON")
    .option("--table", "Output as table")
    .action(async (executionId: string, options) => {
      const adapter = new ProgressTrackingAdapter();
      const metrics = await adapter.getProgress(executionId as ID);

      const router = getRouter();
      let formatFn: () => string;
      if (options.json) {
        formatFn = () => JSON.stringify(metrics, null, 2);
      } else if (options.table) {
        formatFn = () => formatProgressTable(metrics);
      } else {
        formatFn = () => formatProgressMetrics(metrics);
      }

      router.render(metrics, {
        type: "detail",
        entity: "progress",
        format: formatFn,
      });
    });

  cmd
    .command("status <executionId>")
    .description("Get execution status")
    .option("--json", "Output as JSON")
    .action(async (executionId: string, options) => {
      const adapter = new ProgressTrackingAdapter();
      const metrics = await adapter.getProgress(executionId as ID);

      const router = getRouter();
      const status = {
        id: metrics.executionId,
        status: metrics.status,
        progress: `${metrics.progressPercentage.toFixed(1)}%`,
        iteration: `${metrics.iteration}/${metrics.totalIterations}`,
        elapsed: `${(metrics.elapsedTime / 1000).toFixed(1)}s`,
        remaining: metrics.estimatedRemainingTime > 0
          ? `${(metrics.estimatedRemainingTime / 1000).toFixed(1)}s`
          : "N/A",
        completionTime: metrics.estimatedCompletionTime
          ? metrics.estimatedCompletionTime.toLocaleTimeString()
          : "N/A",
      };

      let formatFn: () => string;
      if (options.json) {
        formatFn = () => JSON.stringify(status, null, 2);
      } else {
        formatFn = () => {
          const lines = [
            "",
            "EXECUTION STATUS",
            "================",
            "",
            `ID: ${status.id}`,
            `Status: ${status.status}`,
            `Progress: ${status.progress}`,
            `Iteration: ${status.iteration}`,
            `Elapsed: ${status.elapsed}`,
            `Remaining: ${status.remaining}`,
            `Est. Completion: ${status.completionTime}`,
            "",
          ];
          return lines.join("\n");
        };
      }

      router.render(status, {
        type: "detail",
        entity: "progress",
        format: formatFn,
      });
    });

  return cmd;
}
