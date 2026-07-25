/**
 * Workflow Execution Command Group
 * 
 * Refactored to use ExecutionService for unified execution.
 * All workflow executions go through SDK. Terminals are used ONLY for display.
 */

import { Command } from "commander";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { getFormatter } from "../../utils/formatter.js";
import { formatWorkflowExecution, formatWorkflowExecutionList } from "../../utils/formatters/index.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";
import { CLIValidationError } from "../../types/cli-types.js";

// Import container for dependency injection
import { getContainer } from "../../services/container.js";

const output = getOutput();
const router = getRouter();

/**
 * Get ExecutionService from container
 */
function getExecutionService() {
  return getContainer().getExecutionService();
}

/**
 * Get TerminalManager from container
 */
function getTerminalManager() {
  return getContainer().getTerminalManager();
}

/**
 * Get WorkflowExecutionAdapter from container
 */
function getWorkflowExecutionAdapter() {
  return getContainer().getWorkflowExecutionAdapter();
}

/**
 * Create workflow execution command group
 */
export function createWorkflowExecutionCommands(): Command {
  const workflowExecutionCmd = new Command("execution").description("Manage workflow executions");

  // Execute workflow execution commands
  // Modes:
  //   - default (no flag): async in-process execution with real-time event display via node-pty terminal
  //   - --blocking: synchronous execution, waits for completion in the current terminal
  //   - --background: async execution, output redirected to log file, no terminal window
  workflowExecutionCmd
    .command("run <workflow-id>")
    .description("Execute a workflow (default: async in-process with real-time event terminal)")
    .option("-i, --input <json>", "Input data (JSON format)")
    .option("-v, --verbose", "Detailed output")
    .option("-b, --blocking", "Run synchronously in the current terminal and wait for completion")
    .option("--background", "Run asynchronously in the background with output redirected to log file")
    .option("--log-file <path>", "Log file path for background execution")
    .action(
      async (
        workflowId,
        options: CommandOptions & {
          input?: string;
          blocking?: boolean;
          background?: boolean;
          logFile?: string;
        },
      ) => {
        try {
          output.infoLog(`Starting workflow execution: ${workflowId}`);

          let inputData: Record<string, unknown> = {};
          if (options.input) {
            try {
              inputData = JSON.parse(options.input);
            } catch (_error) {
              handleError(new CLIValidationError("The input data must be in valid JSON format."), {
                operation: "runWorkflowExecution",
                additionalInfo: { workflowId, input: options.input },
              });
              return;
            }
          }

          // Determine execution mode (default: foreground)
          const mode = options.blocking ? 'blocking' : 
                      options.background ? 'background' : 'foreground';

          // Execute via ExecutionService (unified path through SDK)
          const result = await getExecutionService().execute(workflowId, inputData, mode);

          // Display result based on mode
          if (mode === 'blocking') {
            router.render(result.result, {
              type: "detail",
              entity: "execution",
              format: () => formatWorkflowExecution(result.result!, { verbose: options.verbose }),
            });
          } else if (mode === 'background') {
            // Background mode: Show background execution info
            router.render(result, {
              type: "detail",
              entity: "execution",
              format: () => {
                let text = "\nThe workflow execution has been started in the background.\n";
                text += getFormatter().keyValue("Execution ID", result.executionId) + "\n";
                text += getFormatter().keyValue("Process ID", String(result.pid)) + "\n";
                text += getFormatter().keyValue("Log file", result.logFile || `logs/workflow-${result.executionId}.log`) + "\n";
                text += getFormatter().keyValue("Startup time", result.startTime.toISOString()) + "\n";
                text += `\nUse 'modular-agent execution status ${result.executionId}' to check execution status\n`;
                return text;
              },
              message: `Workflow execution started in background: ${result.executionId}`,
              metadata: { executionId: result.executionId, pid: result.pid, mode: 'background' },
            });
          } else {
            // Foreground mode: Show terminal info and wait for completion
            router.render(result, {
              type: "detail",
              entity: "execution",
              format: () => {
                let text = "\nThe workflow execution has been started in a foreground terminal.\n";
                text += getFormatter().keyValue("Execution ID", result.executionId) + "\n";
                text += getFormatter().keyValue("Process ID", String(result.pid)) + "\n";
                text += getFormatter().keyValue("Startup time", result.startTime.toISOString()) + "\n";
                text += getFormatter().keyValue("Detached", String(result.detached)) + "\n";
                return text;
              },
              message: `Workflow execution started in foreground terminal: ${result.executionId}`,
              metadata: { executionId: result.executionId, pid: result.pid, mode: 'foreground', detached: result.detached },
            });

            // Wait for execution to complete before returning (prevents postAction
            // shutdown from killing the in-process execution prematurely)
            if (result.completion) {
              await result.completion;
              output.infoLog('Foreground execution completed');
            }
          }
        } catch (error) {
          handleError(error, {
            operation: "runWorkflowExecution",
            additionalInfo: { workflowId },
          });
        }
      },
    );

  // View execution status command
  workflowExecutionCmd
    .command("status <execution-id>")
    .description("Check the execution status")
    .action(async executionId => {
      try {
        const status = await getExecutionService().monitorExecution(executionId);
        router.render(status, {
          type: "detail",
          entity: "execution",
          format: () => {
            let text = "\n";
            text += getFormatter().subsection("Execution Status:") + "\n";
            text += getFormatter().keyValue("Execution ID", status.executionId) + "\n";
            text += getFormatter().keyValue("Status", status.status) + "\n";
            text += getFormatter().keyValue("Progress", `${status.progress || 'N/A'}%`) + "\n";
            text += getFormatter().keyValue("Last update", status.lastUpdate.toISOString()) + "\n";
            return text;
          },
          metadata: { executionId: executionId },
        });
      } catch (error) {
        handleError(error, {
          operation: "getExecutionStatus",
          additionalInfo: { executionId },
        });
      }
    });

  // Cancel execution command
  workflowExecutionCmd
    .command("cancel <execution-id>")
    .description("Cancel workflow execution")
    .action(async executionId => {
      try {
        await getExecutionService().stopExecution(executionId);
        router.render(null, {
          type: "action",
          message: `Execution cancelled: ${executionId}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "cancelExecution",
          additionalInfo: { executionId },
        });
      }
    });

  // Follow execution in real-time: stream events to the main terminal
  workflowExecutionCmd
    .command("follow <execution-id>")
    .description("Follow execution progress in real-time by streaming events to stdout")
    .action(async (executionId: string) => {
      try {
        output.infoLog(`Following execution: ${executionId} (press Ctrl+C to stop watching)`);
        await getExecutionService().followExecution(executionId);
      } catch (error) {
        handleError(error, {
          operation: "followExecution",
          additionalInfo: { executionId },
        });
      }
    });

  // Addition: List active terminal commands
  workflowExecutionCmd
    .command("terminals")
    .description("List all active terminals")
    .action(() => {
      const terminals = getTerminalManager().getActiveTerminals();
      if (terminals.length === 0) {
        router.render([], {
          type: "list",
          entity: "terminal",
          format: () => "There are no active terminals currently.",
        });
        return;
      }

      router.render(terminals, {
        type: "list",
        entity: "terminal",
        format: () => {
          let text = "\n";
          text += getFormatter().subsection("Active Terminal:") + "\n";
          terminals.forEach((terminal: any, index: number) => {
            text += `  ${index + 1}. ID: ${terminal.id}\n`;
            text += `     PID: ${terminal.pid}\n`;
            text += `     Status: ${terminal.status}\n`;
            text += `     Created at: ${terminal.createdAt.toISOString()}\n\n`;
          });
          return text;
        },
        metadata: { count: terminals.length },
      });
    });

  // Pause workflow execution command
  workflowExecutionCmd
    .command("pause <execution-id>")
    .description("Pause the workflow execution")
    .action(async (executionId: string) => {
      try {
        output.infoLog(`Pausing workflow execution: ${executionId}`);

        const adapter = getWorkflowExecutionAdapter();
        await adapter.pauseWorkflowExecution(executionId);

        router.render(
          { executionId },
          {
            type: "action",
            entity: "execution",
            message: `Execution paused: ${executionId}`,
            format: () => `Workflow execution paused: ${executionId}`,
          },
        );
      } catch (error) {
        handleError(error, {
          operation: "pauseWorkflowExecution",
          additionalInfo: { executionId },
        });
      }
    });

  // Workflow execution recovery command
  workflowExecutionCmd
    .command("resume <execution-id>")
    .description("Resume the workflow execution")
    .action(async (executionId: string) => {
      try {
        output.infoLog(`Resuming workflow execution: ${executionId}`);

        const adapter = getWorkflowExecutionAdapter();
        await adapter.resumeWorkflowExecution(executionId);

        router.render(
          { executionId },
          {
            type: "action",
            entity: "execution",
            message: `Execution resumed: ${executionId}`,
            format: () => `Workflow execution resumed: ${executionId}`,
          },
        );
      } catch (error) {
        handleError(error, {
          operation: "resumeWorkflowExecution",
          additionalInfo: { executionId },
        });
      }
    });

  // Stop workflow execution command
  workflowExecutionCmd
    .command("stop <execution-id>")
    .description("Stop the workflow execution")
    .action(async (executionId: string) => {
      try {
        output.infoLog(`Stopping workflow execution: ${executionId}`);

        const adapter = getWorkflowExecutionAdapter();
        await adapter.stopWorkflowExecution(executionId);

        router.render(
          { executionId },
          {
            type: "action",
            entity: "execution",
            message: `Execution stopped: ${executionId}`,
            format: () => `Workflow execution stopped: ${executionId}`,
          },
        );
      } catch (error) {
        handleError(error, {
          operation: "stopWorkflowExecution",
          additionalInfo: { executionId },
        });
      }
    });

  // List workflow execution commands
  workflowExecutionCmd
    .command("list")
    .description("List all workflow executions")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .action(async (options: CommandOptions) => {
      try {
        const adapter = getWorkflowExecutionAdapter();
        const executions = await adapter.listWorkflowExecutions();

        router.render(executions, {
          type: "list",
          entity: "execution",
          format: () => formatWorkflowExecutionList(executions, { table: options.table }),
          metadata: { total: executions.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "listWorkflowExecutions",
        });
      }
    });

  // View workflow execution details command
  workflowExecutionCmd
    .command("show <execution-id>")
    .description("View workflow execution details")
    .option("-v, --verbose", "Detailed output")
    .action(async (executionId: string, options: CommandOptions) => {
      try {
        const adapter = getWorkflowExecutionAdapter();
        const execution = await adapter.getWorkflowExecution(executionId);

        router.render(execution, {
          type: "detail",
          entity: "execution",
          format: () => formatWorkflowExecution(execution, { verbose: options.verbose }),
        });
      } catch (error) {
        handleError(error, {
          operation: "getWorkflowExecution",
          additionalInfo: { executionId },
        });
      }
    });

  // Delete workflow execution command
  workflowExecutionCmd
    .command("delete <execution-id>")
    .description("Delete the workflow execution")
    .option("-f, --force", "Forced deletion, without prompting for confirmation")
    .action(async (executionId: string, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete workflow execution: ${executionId}`);
          // In practical applications, interactive confirmation can be added here.
          output.infoLog("Use the --force option to skip the confirmation.");
          return;
        }

        const adapter = getWorkflowExecutionAdapter();
        await adapter.deleteWorkflowExecution(executionId);
      } catch (error) {
        handleError(error, {
          operation: "deleteWorkflowExecution",
          additionalInfo: { executionId },
        });
      }
    });

  return workflowExecutionCmd;
}
