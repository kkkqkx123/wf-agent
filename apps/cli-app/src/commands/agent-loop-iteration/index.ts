/**
 * Agent Loop Iteration Commands
 * Commands for analyzing agent loop iterations
 */

import { Command } from "commander";
import { IterationAnalysisAdapter } from "../../adapters/iteration-analysis-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { formatDecisionAnalysis, formatExecutionPathAnalysis } from "../../utils/formatters/iteration-formatters.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";
import type { ID } from "@wf-agent/types";

const output = getOutput();
const router = getRouter();
let adapter: IterationAnalysisAdapter | null = null;

function getAdapter(): IterationAnalysisAdapter {
  if (!adapter) {
    adapter = new IterationAnalysisAdapter();
  }
  return adapter;
}

/**
 * Create Agent Loop Iteration Commands
 */
export function createIterationCommands(): Command {
  const iterationCmd = new Command("iteration")
    .description("Agent Loop iteration analysis and tracking");

  // Show iteration detail or summary
  iterationCmd
    .command("show <agent-loop-id> [iteration-index]")
    .description("Show detailed information about agent loop iterations")
    .option("--json", "Output as JSON")
    .action(
      async (
        agentLoopId: ID,
        iterationIndex: string | undefined,
        options: CommandOptions & {
          json?: boolean;
        },
      ) => {
        try {
          output.infoLog(
            `Fetching iteration details for agent loop: ${agentLoopId}`,
          );

          // Get summary
          const summary =
            await getAdapter().getIterationHistorySummary(agentLoopId);
          if (!summary) {
            output.warn("No iteration history found");
            return;
          }

          if (options.json) {
            router.render(summary, {
              type: "detail",
              entity: "iteration_summary",
              message: "Iteration summary retrieved",
            });
          } else {
            const text = `📊 Agent Loop Iteration Summary for ${agentLoopId}
─${"-".repeat(50)}
Total Iterations: ${summary.totalIterations}
Current Iteration: ${summary.currentIteration ?? "N/A"}
Max Iterations: ${summary.maxIterations ?? "N/A"}
Status: ${summary.status}`;

            router.render(text, {
              type: "detail",
              entity: "iteration_summary",
              format: () => text,
            });
          }
        } catch (error) {
          handleError(error, {
            operation: "showIteration",
            additionalInfo: { agentLoopId, iterationIndex },
          });
        }
      },
    );

  // List iterations
  iterationCmd
    .command("list <agent-loop-id>")
    .description("List all iterations for an agent loop")
    .option("--limit <number>", "Maximum number of iterations to show", "20")
    .option("--json", "Output as JSON")
    .option("--table", "Display as table")
    .action(
      async (
        agentLoopId: ID,
        options: CommandOptions & {
          limit?: string;
          json?: boolean;
          table?: boolean;
        },
      ) => {
        try {
          output.infoLog(`Listing iterations for agent loop: ${agentLoopId}`);

          const limit = options.limit ? parseInt(options.limit, 10) : 20;
          const iterations = await getAdapter().listIterations(agentLoopId, limit);

          if (iterations.length === 0) {
            output.warn("No iterations found");
            return;
          }

          if (options.json) {
            router.render(iterations, {
              type: "list",
              entity: "iterations",
              message: `Found ${iterations.length} iterations`,
            });
          } else if (options.table) {
            const table = iterations
              .slice(0, limit)
              .map(
                (iter) =>
                  `  #${iter.iteration.toString().padEnd(4)} | Status: ${iter.status.padEnd(10)} | Tools: ${(iter.toolCalls ?? 0).toString().padEnd(3)} | Errors: ${iter.errors ?? 0}`,
              )
              .join("\n");

            router.render(
              `📋 Iterations for ${agentLoopId}:\n${table}`,
              {
                type: "list",
                entity: "iterations",
                format: () => `📋 Iterations for ${agentLoopId}:\n${table}`,
              },
            );
          } else {
            const list = iterations
              .slice(0, limit)
              .map(
                (iter, i) =>
                  `  ${i + 1}. Iteration #${iter.iteration} - ${iter.status}`,
              )
              .join("\n");

            router.render(
              `📋 Iterations for ${agentLoopId} (showing ${iterations.length}):\n${list}`,
              {
                type: "list",
                entity: "iterations",
                format: () => `📋 Iterations for ${agentLoopId} (showing ${iterations.length}):\n${list}`,
              },
            );
          }
        } catch (error) {
          handleError(error, {
            operation: "listIterations",
            additionalInfo: { agentLoopId },
          });
        }
      },
    );

  // Analyze decisions
  iterationCmd
    .command("analyze <agent-loop-id>")
    .description("Analyze decision patterns across iterations")
    .option("--json", "Output as JSON")
    .action(
      async (
        agentLoopId: ID,
        options: CommandOptions & { json?: boolean },
      ) => {
        try {
          output.infoLog(`Analyzing decisions for agent loop: ${agentLoopId}`);

          const analysis = await getAdapter().analyzeDecisions(agentLoopId);
          if (!analysis) {
            output.warn("No decision data found");
            return;
          }

          if (options.json) {
            router.render(analysis, {
              type: "detail",
              entity: "decision_analysis",
              message: "Decision analysis completed",
            });
          } else {
            router.render(formatDecisionAnalysis(analysis), {
              type: "detail",
              entity: "decision_analysis",
              format: () => formatDecisionAnalysis(analysis),
            });
          }
        } catch (error) {
          handleError(error, {
            operation: "analyzeDecisions",
            additionalInfo: { agentLoopId },
          });
        }
      },
    );

  // Analyze execution paths
  iterationCmd
    .command("paths <agent-loop-id>")
    .description("Analyze execution paths taken across iterations")
    .option("--json", "Output as JSON")
    .action(
      async (
        agentLoopId: ID,
        options: CommandOptions & { json?: boolean },
      ) => {
        try {
          output.infoLog(
            `Analyzing execution paths for agent loop: ${agentLoopId}`,
          );

          const analysis = await getAdapter().analyzeExecutionPaths(agentLoopId);
          if (!analysis) {
            output.warn("No execution path data found");
            return;
          }

          if (options.json) {
            router.render(analysis, {
              type: "detail",
              entity: "execution_path_analysis",
              message: "Execution path analysis completed",
            });
          } else {
            router.render(formatExecutionPathAnalysis(analysis), {
              type: "detail",
              entity: "execution_path_analysis",
              format: () => formatExecutionPathAnalysis(analysis),
            });
          }
        } catch (error) {
          handleError(error, {
            operation: "analyzeExecutionPaths",
            additionalInfo: { agentLoopId },
          });
        }
      },
    );

  // Metrics summary
  iterationCmd
    .command("metrics <agent-loop-id> [iteration-index]")
    .description("Display iteration metrics")
    .option("--json", "Output as JSON")
    .action(
      async (
        agentLoopId: ID,
        iterationIndex: string | undefined,
        options: CommandOptions & { json?: boolean },
      ) => {
        try {
          output.infoLog(`Fetching metrics for agent loop: ${agentLoopId}`);

          const index = iterationIndex
            ? parseInt(iterationIndex, 10)
            : undefined;

          const metrics = await getAdapter().getIterationMetrics(agentLoopId, index);
          if (!metrics) {
            output.warn("No metrics found");
            return;
          }

          if (options.json) {
            router.render(metrics, {
              type: "detail",
              entity: "iteration_metrics",
              message: "Metrics retrieved",
            });
          } else {
            const text = `📊 Iteration Metrics for ${agentLoopId}
─${"-".repeat(50)}
${JSON.stringify(metrics, null, 2)}`;

            router.render(text, {
              type: "detail",
              entity: "iteration_metrics",
              format: () => text,
            });
          }
        } catch (error) {
          handleError(error, {
            operation: "metricsIteration",
            additionalInfo: { agentLoopId, iterationIndex },
          });
        }
      },
    );

  return iterationCmd;
}
