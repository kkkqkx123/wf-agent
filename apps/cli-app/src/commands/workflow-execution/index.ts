/**
 * Workflow Execution Command Group
 * 
 * Refactored to use ExecutionService for unified execution.
 * All workflow executions go through SDK. Terminals are used ONLY for display.
 */

import { Command } from "commander";
import { getOutput } from "../../utils/output.js";
import { formatWorkflowExecution, formatWorkflowExecutionList } from "../../utils/cli-formatters.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";
import { CLIValidationError } from "../../types/cli-types.js";
import { getSDKInstance } from "../../index.js";

// Import ExecutionService and TerminalManager
import { ExecutionService } from "../../services/execution/execution-service.js";
import { TerminalManager } from "../../services/terminal/terminal-manager.js";
import { WorkflowExecutionAdapter } from "../../adapters/workflow-execution-adapter.js";

const output = getOutput();

// Lazy initialization - create instances only when needed
let executionService: ExecutionService | null = null;
let terminalManager: TerminalManager | null = null;

function getExecutionService(): ExecutionService {
  if (!executionService) {
    const sdk = getSDKInstance();
    if (!sdk) {
      throw new Error("SDK instance not initialized. Make sure the CLI app has started.");
    }
    terminalManager = new TerminalManager();
    executionService = new ExecutionService(sdk, terminalManager);
  }
  return executionService;
}

function getTerminalManager(): TerminalManager {
  if (!terminalManager) {
    // Create standalone TerminalManager for listing terminals
    terminalManager = new TerminalManager();
  }
  return terminalManager;
}

/**
 * Create workflow execution command group
 */
export function createWorkflowExecutionCommands(): Command {
  const workflowExecutionCmd = new Command("execution").description("Manage workflow executions");

  // Execute workflow execution commands - The default mode is foreground detached.
  workflowExecutionCmd
    .command("run <workflow-id>")
    .description("Execute workflow")
    .option("-i, --input <json>", "Input data (JSON format)")
    .option("-v, --verbose", "Detailed output")
    .option("-b, --blocking", "Run in the current terminal (in a blocking manner).")
    .option("--background", "Running in the background (without displaying a terminal window)")
    .option("--log-file <path>", "Log file path during backend runtime")
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

          // Determine execution mode
          const mode = options.blocking ? 'blocking' : 
                      options.background ? 'background' : 'detached';

          // Execute via ExecutionService (unified path through SDK)
          const result = await getExecutionService().execute(workflowId, inputData, mode);

          // Display result based on mode
          if (mode === 'blocking') {
            // Blocking mode: Show final result
            output.output(formatWorkflowExecution(result.result as any, { verbose: options.verbose }));
          } else if (mode === 'background') {
            // Background mode: Show background execution info
            output.newLine();
            output.info("The workflow execution has been started in the background.");
            output.keyValue("Execution ID", result.executionId);
            output.keyValue("Process ID", String(result.pid));
            output.keyValue("Log file", result.logFile || `logs/workflow-${result.executionId}.log`);
            output.keyValue("Startup time", result.startTime.toISOString());
            output.newLine();
            output.info(
              `Use 'modular-agent execution status ${result.executionId}' to check execution status`,
            );
          } else {
            // Detached mode: Show terminal info
            output.newLine();
            output.info("The workflow execution has been started in a separate terminal.");
            output.keyValue("Execution ID", result.executionId);
            if (result.terminalId) {
              output.keyValue("Terminal ID", result.terminalId);
            }
            output.keyValue("Process ID", String(result.pid));
            output.keyValue("Startup time", result.startTime.toISOString());
            output.newLine();
            output.info(
              `Use 'modular-agent execution status ${result.executionId}' to check execution status`,
            );
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
        output.newLine();
        output.subsection("Execution Status:");
        output.keyValue("Execution ID", status.executionId);
        output.keyValue("Status", status.status);
        output.keyValue("Progress", `${status.progress || 'N/A'}%`);
        output.keyValue("Last update", status.lastUpdate.toISOString());
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
        output.info(`Execution cancelled: ${executionId}`);
      } catch (error) {
        handleError(error, {
          operation: "cancelExecution",
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
        output.newLine();
        output.output("There are no active terminals currently.");
        return;
      }

      output.newLine();
      output.subsection("Active Terminal:");
      terminals.forEach((terminal: any, index: number) => {
        output.output(`  ${index + 1}. ID: ${terminal.id}`);
        output.output(`     PID: ${terminal.pid}`);
        output.output(`     Status: ${terminal.status}`);
        output.output(`     Created at: ${terminal.createdAt.toISOString()}`);
        output.newLine();
      });
    });

  // Pause workflow execution command
  workflowExecutionCmd
    .command("pause <execution-id>")
    .description("Pause the workflow execution")
    .action(async (executionId: string) => {
      try {
        output.infoLog(`Pausing workflow execution: ${executionId}`);

        const adapter = new WorkflowExecutionAdapter();
        await adapter.pauseWorkflowExecution(executionId);
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

        const adapter = new WorkflowExecutionAdapter();
        await adapter.resumeWorkflowExecution(executionId);
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

        const adapter = new WorkflowExecutionAdapter();
        await adapter.stopWorkflowExecution(executionId);
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
        const adapter = new WorkflowExecutionAdapter();
        const executions = await adapter.listWorkflowExecutions();

        output.output(formatWorkflowExecutionList(executions, { table: options.table }));
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
        const adapter = new WorkflowExecutionAdapter();
        const execution = await adapter.getWorkflowExecution(executionId);

        output.output(formatWorkflowExecution(execution, { verbose: options.verbose }));
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

        const adapter = new WorkflowExecutionAdapter();
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
