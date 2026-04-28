/**
 * Thread Command Group
 */

import { Command } from "commander";
import { ThreadAdapter } from "../../adapters/thread-adapter.js";
import { getOutput } from "../../utils/output.js";
import { formatThread, formatThreadList } from "../../utils/cli-formatters.js";
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
 * Create thread command group
 */
export function createThreadCommands(): Command {
  const threadCmd = new Command("thread").description("Manage threads");

  // Execute thread commands - The default mode is foreground detached.
  threadCmd
    .command("run <workflow-id>")
    .description("Execute workflow thread")
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
          output.infoLog(`Starting thread: ${workflowId}`);

          let inputData: Record<string, unknown> = {};
          if (options.input) {
            try {
              inputData = JSON.parse(options.input);
            } catch (error) {
              handleError(new CLIValidationError("The input data must be in valid JSON format."), {
                operation: "runThread",
                additionalInfo: { workflowId, input: options.input },
              });
              return;
            }
          }

          if (options.blocking) {
            // Run in the current terminal (in a blocking manner).
            const adapter = new ThreadAdapter();
            const thread = await adapter.executeThread(workflowId, inputData);
            output.output(formatThread(thread, { verbose: options.verbose }));
          } else {
            // Run in an independent terminal (the default method).
            const terminal = terminalManager.createTerminal({
              background: options.background,
              logFile: options.logFile,
            });
            const result = await taskExecutor.executeInTerminal(workflowId, inputData, terminal);

            if (options.background) {
              output.newLine();
              output.info("The thread has been started in the background.");
              output.keyValue("Task ID", result.taskId);
              output.keyValue("Process ID", String(terminal.pid));
              output.keyValue("Log file", options.logFile || `logs/task-${result.taskId}.log`);
              output.keyValue("Startup time", result.startTime.toISOString());
              output.newLine();
              output.info(
                `Use 'modular-agent thread status ${result.taskId}' to check task status`,
              );
            } else {
              output.newLine();
              output.info("The thread has been started in a separate terminal.");
              output.keyValue("Task ID", result.taskId);
              output.keyValue("Terminal ID", result.sessionId);
              output.keyValue("Process ID", String(terminal.pid));
              output.keyValue("Startup time", result.startTime.toISOString());
              output.newLine();
              output.info(
                `Use 'modular-agent thread status ${result.taskId}' to check task status`,
              );
            }
          }
        } catch (error) {
          handleError(error, {
            operation: "runThread",
            additionalInfo: { workflowId },
          });
        }
      },
    );

  // Addition: Command to view task status
  threadCmd
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
  threadCmd
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
  threadCmd
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

  // Pause thread command
  threadCmd
    .command("pause <thread-id>")
    .description("Pause the thread")
    .action(async threadId => {
      try {
        output.infoLog(`Pausing thread: ${threadId}`);

        const adapter = new ThreadAdapter();
        await adapter.pauseThread(threadId);
      } catch (error) {
        handleError(error, {
          operation: "pauseThread",
          additionalInfo: { threadId },
        });
      }
    });

  // Thread recovery command
  threadCmd
    .command("resume <thread-id>")
    .description("Resume the thread")
    .action(async threadId => {
      try {
        output.infoLog(`Resuming thread: ${threadId}`);

        const adapter = new ThreadAdapter();
        await adapter.resumeThread(threadId);
      } catch (error) {
        handleError(error, {
          operation: "resumeThread",
          additionalInfo: { threadId },
        });
      }
    });

  // Stop thread command
  threadCmd
    .command("stop <thread-id>")
    .description("Stop the thread")
    .action(async threadId => {
      try {
        output.infoLog(`Stopping thread: ${threadId}`);

        const adapter = new ThreadAdapter();
        await adapter.stopThread(threadId);
      } catch (error) {
        handleError(error, {
          operation: "stopThread",
          additionalInfo: { threadId },
        });
      }
    });

  // List thread commands
  threadCmd
    .command("list")
    .description("List all threads")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .action(async (options: CommandOptions) => {
      try {
        const adapter = new ThreadAdapter();
        const threads = await adapter.listThreads();

        output.output(formatThreadList(threads, { table: options.table }));
      } catch (error) {
        handleError(error, {
          operation: "listThreads",
        });
      }
    });

  // View thread details command
  threadCmd
    .command("show <thread-id>")
    .description("View thread details")
    .option("-v, --verbose", "Detailed output")
    .action(async (threadId, options: CommandOptions) => {
      try {
        const adapter = new ThreadAdapter();
        const thread = await adapter.getThread(threadId);

        output.output(formatThread(thread, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "getThread",
          additionalInfo: { threadId },
        });
      }
    });

  // Delete thread command
  threadCmd
    .command("delete <thread-id>")
    .description("Delete the thread")
    .option("-f, --force", "Forced deletion, without prompting for confirmation")
    .action(async (threadId, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete thread: ${threadId}`);
          // In practical applications, interactive confirmation can be added here.
          output.infoLog("Use the --force option to skip the confirmation.");
          return;
        }

        const adapter = new ThreadAdapter();
        await adapter.deleteThread(threadId);
      } catch (error) {
        handleError(error, {
          operation: "deleteThread",
          additionalInfo: { threadId },
        });
      }
    });

  return threadCmd;
}
