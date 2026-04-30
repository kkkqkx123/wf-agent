/**
 * Workflow Execution Command Group
 */

import { Command } from "commander";
import { WorkflowExecutionAdapter } from "../../adapters/workflow-execution-adapter.js";
import { getOutput } from "../../utils/output.js";
import { formatWorkflowExecution, formatWorkflowExecutionList } from "../../utils/cli-formatters.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";
import { CLIValidationError } from "../../types/cli-types.js";

// Add import statements
import { TerminalManager } from "../../terminal/terminal-manager.js";
import { TaskExecutor } from "../../terminal/task-executor.js";
import { CommunicationBridge } from "../../terminal/communication-bridge.js";

const output = getOutput();

// Create a global instance
const terminalManager = new TerminalManager();
const taskExecutor = new TaskExecutor();
const communicationBridge = new CommunicationBridge();

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
            } catch (error) {
              handleError(new CLIValidationError("The input data must be in valid JSON format."), {
                operation: "runWorkflowExecution",
                additionalInfo: { workflowId, input: options.input },
              });
              return;
            }
          }

          if (options.blocking) {
            // Run in the current terminal (in a blocking manner).
            const adapter = new WorkflowExecutionAdapter();
            const execution = await adapter.executeWorkflow(workflowId, inputData);
            output.output(formatWorkflowExecution(execution, { verbose: options.verbose }));
          } else {
            // Run in an independent terminal (the default method).
            const terminal = terminalManager.createTerminal({
              background: options.background,
              logFile: options.logFile,
            });
            const result = await taskExecutor.executeInTerminal(workflowId, inputData, terminal);

            if (options.background) {
              output.newLine();
              output.info("The workflow execution has been started in the background.");
              output.keyValue("Task ID", result.taskId);
              output.keyValue("Process ID", String(terminal.pid));
              output.keyValue("Log file", options.logFile || `logs/task-${result.taskId}.log`);
              output.keyValue("Startup time", result.startTime.toISOString());
              output.newLine();
              output.info(
                `Use 'modular-agent execution status ${result.taskId}' to check task status`,
              );
            } else {
              output.newLine();
              output.info("The workflow execution has been started in a separate terminal.");
              output.keyValue("Task ID", result.taskId);
              output.keyValue("Terminal ID", result.sessionId);
              output.keyValue("Process ID", String(terminal.pid));
              output.keyValue("Startup time", result.startTime.toISOString());
              output.newLine();
              output.info(
                `Use 'modular-agent execution status ${result.taskId}' to check task status`,
              );
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

  // Addition: Command to view task status
  workflowExecutionCmd
    .command("status <task-id>")
    .description("Check the task status.")
    .action(async taskId => {
      try {
        const status = await taskExecutor.monitorTask(taskId);
        output.newLine();
        output.subsection("Task Status:");
        output.keyValue("Task ID", status.taskId);
        output.keyValue("Status", status.status);
        output.keyValue("Progress", `${status.progress || 0}%`);
        output.keyValue("Message", status.message || "No message available");
        output.keyValue("Last update", status.lastUpdate.toISOString());
      } catch (error) {
        handleError(error, {
          operation: "getTaskStatus",
          additionalInfo: { taskId },
        });
      }
    });

  // Addition: Command to stop a task
  workflowExecutionCmd
    .command("cancel <task-id>")
    .description("Cancel task execution")
    .action(async taskId => {
      try {
        await taskExecutor.stopTask(taskId);
        output.info(`Task cancelled: ${taskId}`);
      } catch (error) {
        handleError(error, {
          operation: "cancelTask",
          additionalInfo: { taskId },
        });
      }
    });

  // Addition: List active terminal commands
  workflowExecutionCmd
    .command("terminals")
    .description("List all active terminals")
    .action(() => {
      const terminals = terminalManager.getActiveTerminals();
      if (terminals.length === 0) {
        output.newLine();
        output.output("There are no active terminals currently.");
        return;
      }

      output.newLine();
      output.subsection("Active Terminal:");
      terminals.forEach((terminal, index) => {
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
