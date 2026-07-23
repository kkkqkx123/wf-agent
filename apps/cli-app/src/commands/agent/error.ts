/**
 * Agent Error Analysis Command Group
 * Subcommands for diagnosing agent loop execution errors
 */

import { Command } from "commander";
import { AgentErrorAnalysisAdapter } from "../../adapters/agent-error-analysis-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const output = getOutput();
const router = getRouter();

/**
 * Create Agent Error Analysis Commands
 */
export function createAgentErrorCommands(): Command {
  const errorCmd = new Command("error").description("Analyze agent loop execution errors");

  // agent error analyze <execution-id>
  errorCmd
    .command("analyze <execution-id>")
    .description("Analyze root cause of errors in an agent loop execution")
    .action(async (executionId: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Analyzing errors for execution "${executionId}"...`);
        const adapter = new AgentErrorAnalysisAdapter();
        const result = await adapter.analyzeRootCause(executionId);

        router.render(result, {
          type: "detail",
          entity: "agent-error-analysis",
          format: () => {
            if (!result.hasError) {
              return `No errors detected in execution "${executionId}".`;
            }
            const lines: string[] = [];
            lines.push(`Root Cause Analysis for "${executionId}":`);
            lines.push(`  Can recover: ${result.canRecover}`);
            lines.push(`  Recommended action: ${result.recommendedAction ?? "N/A"}`);
            lines.push(`  Summary: ${result.summary}`);
            if (result.rootCauseError) {
              lines.push(`  Root cause error ID: ${result.rootCauseError.id ?? "N/A"}`);
              lines.push(`  Error type: ${result.rootCauseError.errorType ?? "N/A"}`);
            }
            lines.push(`  Error chain length: ${result.errorChain.length}`);
            return lines.join("\n");
          },
          metadata: { executionId },
        });
      } catch (error) {
        handleError(error, { operation: "agent-error-analyze", additionalInfo: { executionId } });
      }
    });

  // agent error stats <execution-id>
  errorCmd
    .command("stats <execution-id>")
    .description("Get error statistics for an agent loop execution")
    .action(async (executionId: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Getting error stats for execution "${executionId}"...`);
        const adapter = new AgentErrorAnalysisAdapter();
        const result = await adapter.getErrorStatistics(executionId);

        router.render(result, {
          type: "detail",
          entity: "agent-error-stats",
          format: () => {
            const lines: string[] = [];
            lines.push(`Error Statistics for "${executionId}":`);
            lines.push(`  Total errors: ${result.totalErrors}`);
            lines.push(`  Error recovery rate: ${(result.errorRecoveryRate * 100).toFixed(1)}%`);
            if (result.mostCommonType) {
              lines.push(`  Most common type: ${result.mostCommonType}`);
            }
            if (Object.keys(result.byType).length > 0) {
              lines.push("  Errors by type:");
              for (const [typ, count] of Object.entries(result.byType)) {
                lines.push(`    ${typ}: ${count}`);
              }
            }
            return lines.join("\n");
          },
          metadata: { executionId },
        });
      } catch (error) {
        handleError(error, { operation: "agent-error-stats", additionalInfo: { executionId } });
      }
    });

  // agent error recover <execution-id> <error-id>
  errorCmd
    .command("recover <execution-id> <error-id>")
    .description("Get recovery proposal for a specific error")
    .action(async (executionId: string, errorId: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Getting recovery proposal for error "${errorId}"...`);
        const adapter = new AgentErrorAnalysisAdapter();
        const result = await adapter.getRecoveryProposal(executionId, errorId);

        router.render(result, {
          type: "detail",
          entity: "agent-error-recovery",
          format: () => {
            if (!result) {
              return `No recovery proposal found for error "${errorId}".`;
            }
            const lines: string[] = [];
            lines.push(`Recovery Proposal for error "${errorId}":`);
            lines.push(`  Action: ${result.action}`);
            lines.push(`  Likelihood: ${(result.likelihood * 100).toFixed(0)}%`);
            lines.push(`  Reason: ${result.reason}`);
            if (result.steps.length > 0) {
              lines.push("  Steps:");
              result.steps.forEach((s, i) => lines.push(`    ${i + 1}. ${s}`));
            }
            return lines.join("\n");
          },
          metadata: { executionId, errorId },
        });
      } catch (error) {
        handleError(error, { operation: "agent-error-recover", additionalInfo: { executionId, errorId } });
      }
    });

  return errorCmd;
}
