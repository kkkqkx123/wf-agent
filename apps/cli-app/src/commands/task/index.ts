/**
 * Task Command Group
 * Manage execution tasks
 */

import { Command } from "commander";
import { TaskAdapter } from "../../adapters/task-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { formatTask, formatTaskList, formatTaskStats } from "../../utils/formatters/task.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";
import { CLIValidationError } from "../../types/cli-types.js";

const output = getOutput();
const router = getRouter();

/**
 * Create Task Command Group
 */
export function createTaskCommands(): Command {
  const taskCmd = new Command("task").description("Manage execution tasks");

  // List tasks
  taskCmd
    .command("list")
    .description("List all tasks")
    .option("-t, --table", "Output in table format")
    .option("-v, --verbose", "Detailed output")
    .option("--status <status>", "Filter by status (queued/running/completed/failed/cancelled/timeout)")
    .option("--execution-id <executionId>", "Filter by execution ID")
    .action(async (options: CommandOptions & { status?: string; executionId?: string }) => {
      try {
        const adapter = new TaskAdapter();
        const filter: Record<string, unknown> = {};
        if (options.status) filter["status"] = options.status;
        if (options.executionId) filter["executionId"] = options.executionId;

        const tasks = await adapter.listTasks(
          Object.keys(filter).length > 0 ? (filter as any) : undefined,
        );

        router.render(tasks, {
          type: "list",
          entity: "task",
          format: () => formatTaskList(tasks, { table: options.table }),
          metadata: { total: tasks.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "list-tasks",
          additionalInfo: { status: options.status, executionId: options.executionId },
        });
      }
    });

  // Show task details
  taskCmd
    .command("show <id>")
    .description("View task details")
    .option("-v, --verbose", "Detailed output")
    .action(async (id, options: CommandOptions) => {
      try {
        const adapter = new TaskAdapter();
        const task = await adapter.getTask(id);

        router.render(task, {
          type: "detail",
          entity: "task",
          format: () => formatTask(task, { verbose: options.verbose }),
        });
      } catch (error) {
        handleError(error, {
          operation: "show-task",
          additionalInfo: { id },
        });
      }
    });

  // Cancel task
  taskCmd
    .command("cancel <id>")
    .description("Cancel a task")
    .action(async (id) => {
      try {
        output.infoLog(`Cancelling task: ${id}`);

        const adapter = new TaskAdapter();
        await adapter.cancelTask(id);

        router.render(null, {
          type: "action",
          entity: "task",
          message: `Task cancelled: ${id}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "cancel-task",
          additionalInfo: { id },
        });
      }
    });

  // Task statistics
  taskCmd
    .command("stats")
    .description("Get task statistics")
    .action(async () => {
      try {
        const adapter = new TaskAdapter();
        const stats = await adapter.getTaskStats();

        output.newLine();
        output.output(formatTaskStats(stats));
      } catch (error) {
        handleError(error, {
          operation: "task-stats",
        });
      }
    });

  // Clean up tasks
  taskCmd
    .command("cleanup")
    .description("Clean up completed/failed/cancelled/timeout tasks")
    .option("--retention <ms>", "Retention time in milliseconds", "3600000")
    .option("-f, --force", "Skip confirmation")
    .action(async (options: { retention?: string; force?: boolean }) => {
      try {
        const retention = options.retention ? parseInt(options.retention, 10) : 3600000;

        if (isNaN(retention) || retention < 0) {
          handleError(
            new CLIValidationError("Retention time must be a non-negative number"),
            { operation: "cleanup-tasks" },
          );
          return;
        }

        if (!options.force) {
          output.warnLog(`About to clean up tasks with retention time: ${retention}ms`);
          output.infoLog("Use the --force option to skip confirmation.");
          return;
        }

        const adapter = new TaskAdapter();
        const count = await adapter.cleanupTasks(retention);

        router.render(null, {
          type: "action",
          entity: "task",
          message: `Cleaned up ${count} tasks`,
        });
      } catch (error) {
        handleError(error, {
          operation: "cleanup-tasks",
          additionalInfo: { retention: options.retention },
        });
      }
    });

  return taskCmd;
}