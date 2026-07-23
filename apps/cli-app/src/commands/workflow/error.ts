/**
 * Workflow Error Analysis Command Group
 * Subcommands for diagnosing workflow execution errors
 */

import { Command } from "commander";
import { WorkflowErrorAnalysisAdapter } from "../../adapters/workflow-error-analysis-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const output = getOutput();
const router = getRouter();

/**
 * Create Workflow Error Analysis Commands
 */
export function createWorkflowErrorCommands(): Command {
  const errorCmd = new Command("error").description("Analyze workflow execution errors");

  // workflow error analyze <execution-id>
  errorCmd
    .command("analyze <execution-id>")
    .description("Analyze root cause of errors in a workflow execution")
    .action(async (executionId: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Analyzing errors for workflow execution "${executionId}"...`);
        const adapter = new WorkflowErrorAnalysisAdapter();
        const result = await adapter.analyzeRootCause(executionId);

        router.render(result, {
          type: "detail",
          entity: "workflow-error-analysis",
          format: () => {
            if (!result.hasError) {
              return `No errors detected in workflow execution "${executionId}".`;
            }
            const lines: string[] = [];
            lines.push(`Root Cause Analysis for "${executionId}":`);
            lines.push(`  Can recover: ${result.canRecover}`);
            lines.push(`  Recommended action: ${result.recommendedAction ?? "N/A"}`);
            lines.push(`  Summary: ${result.summary}`);
            lines.push(`  Affected nodes: ${result.affectedNodes.join(", ") || "none"}`);
            if (result.rootCauseError) {
              lines.push(`  Root cause error ID: ${result.rootCauseError.id ?? "N/A"}`);
              lines.push(`  Error type: ${result.rootCauseError.errorType ?? "N/A"}`);
            }
            return lines.join("\n");
          },
          metadata: { executionId },
        });
      } catch (error) {
        handleError(error, { operation: "workflow-error-analyze", additionalInfo: { executionId } });
      }
    });

  // workflow error stats <execution-id>
  errorCmd
    .command("stats <execution-id>")
    .description("Get error statistics for a workflow execution")
    .action(async (executionId: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Getting error stats for workflow execution "${executionId}"...`);
        const adapter = new WorkflowErrorAnalysisAdapter();
        const result = await adapter.getErrorStatistics(executionId);

        router.render(result, {
          type: "detail",
          entity: "workflow-error-stats",
          format: () => {
            const lines: string[] = [];
            lines.push(`Error Statistics for "${executionId}":`);
            lines.push(`  Total errors: ${result.totalErrors}`);
            lines.push(`  Error recovery rate: ${(result.errorRecoveryRate * 100).toFixed(1)}%`);
            if (result.mostErrorProneNode) {
              lines.push(
                `  Most error-prone node: ${result.mostErrorProneNode.name ?? result.mostErrorProneNode.id} (${result.mostErrorProneNode.count} errors)`,
              );
            }
            if (Object.keys(result.byType).length > 0) {
              lines.push("  Errors by type:");
              for (const [typ, count] of Object.entries(result.byType)) {
                lines.push(`    ${typ}: ${count}`);
              }
            }
            if (Object.keys(result.byNodeName).length > 0) {
              lines.push("  Errors by node:");
              for (const [name, count] of Object.entries(result.byNodeName)) {
                lines.push(`    ${name}: ${count}`);
              }
            }
            return lines.join("\n");
          },
          metadata: { executionId },
        });
      } catch (error) {
        handleError(error, { operation: "workflow-error-stats", additionalInfo: { executionId } });
      }
    });

  // workflow error recover <execution-id> <error-id>
  errorCmd
    .command("recover <execution-id> <error-id>")
    .description("Get recovery proposal for a specific workflow error")
    .action(async (executionId: string, errorId: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Getting recovery proposal for error "${errorId}"...`);
        const adapter = new WorkflowErrorAnalysisAdapter();
        const result = await adapter.getRecoveryProposal(executionId, errorId);

        router.render(result, {
          type: "detail",
          entity: "workflow-error-recovery",
          format: () => {
            if (!result) {
              return `No recovery proposal found for error "${errorId}".`;
            }
            const lines: string[] = [];
            lines.push(`Recovery Proposal for error "${errorId}":`);
            lines.push(`  Action: ${result.action}`);
            lines.push(`  Likelihood: ${(result.likelihood * 100).toFixed(0)}%`);
            lines.push(`  Reason: ${result.reason}`);
            if (result.affectedNode) {
              lines.push(`  Affected node: ${result.affectedNode.name ?? result.affectedNode.id}`);
            }
            if (result.estimatedTimeToRecover != null) {
              lines.push(`  Estimated recovery time: ${result.estimatedTimeToRecover}ms`);
            }
            if (result.steps.length > 0) {
              lines.push("  Steps:");
              result.steps.forEach((s, i) => lines.push(`    ${i + 1}. ${s}`));
            }
            return lines.join("\n");
          },
          metadata: { executionId, errorId },
        });
      } catch (error) {
        handleError(error, { operation: "workflow-error-recover", additionalInfo: { executionId, errorId } });
      }
    });

  return errorCmd;
}
