/**
 * Checkpoint Command Group
 */

import { Command } from "commander";
import { WorkflowExecutionCheckpointAdapter } from "../../adapters/workflow-execution-checkpoint-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { formatCheckpoint, formatCheckpointList } from "../../utils/formatters/index.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const output = getOutput();
const router = getRouter();

/**
 * Create Checkpoint Command Group
 */
export function createCheckpointCommands(): Command {
  const checkpointCmd = new Command("checkpoint").description("Manage checkpoints");

  // Create Checkpoint Command
  checkpointCmd
    .command("create <execution-id>")
    .description("Create checkpoint")
    .option("-n, --name <name>", "Name of checkpoint")
    .option("-v, --verbose", "Detailed output")
    .action(async (executionId, options: CommandOptions & { name?: string }) => {
      try {
        output.infoLog(`Creating checkpoint for workflow execution: ${executionId}`);

        const adapter = new WorkflowExecutionCheckpointAdapter();
        const checkpoint = await adapter.createCheckpoint(executionId, options.name);

        router.render(checkpoint, {
          type: "detail",
          entity: "checkpoint",
          format: () => formatCheckpoint(checkpoint, { verbose: options.verbose }),
        });
      } catch (error) {
        handleError(error, {
          operation: "create-checkpoint",
          additionalInfo: { executionId, name: options.name },
        });
      }
    });

  // Load Checkpoint Command
  checkpointCmd
    .command("load <checkpoint-id>")
    .description("Load checkpoint")
    .action(async checkpointId => {
      try {
        output.infoLog(`Loading checkpoint: ${checkpointId}`);

        const adapter = new WorkflowExecutionCheckpointAdapter();
        await adapter.loadCheckpoint(checkpointId);
      } catch (error) {
        handleError(error, {
          operation: "load-checkpoint",
          additionalInfo: { checkpointId },
        });
      }
    });

  // List Checkpoints command
  checkpointCmd
    .command("list")
    .description("List all checkpoints")
    .option("-t, --table", "Output in tabular format")
    .option("-v, --verbose", "Detailed output")
    .action(async (options: CommandOptions) => {
      try {
        const adapter = new WorkflowExecutionCheckpointAdapter();
        const checkpoints = await adapter.listCheckpoints();

        router.render(checkpoints, {
          type: "list",
          entity: "checkpoint",
          format: () => formatCheckpointList(checkpoints, { table: options.table }),
          metadata: { total: checkpoints.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "list-checkpoints",
        });
      }
    });

  // View Checkpoint Details command
  checkpointCmd
    .command("show <checkpoint-id>")
    .description("View checkpoint details")
    .option("-v, --verbose", "Detailed output")
    .action(async (checkpointId, options: CommandOptions) => {
      try {
        const adapter = new WorkflowExecutionCheckpointAdapter();
        const checkpoint = await adapter.getCheckpoint(checkpointId);

        router.render(checkpoint, {
          type: "detail",
          entity: "checkpoint",
          format: () => formatCheckpoint(checkpoint, { verbose: options.verbose }),
        });
      } catch (error) {
        handleError(error, {
          operation: "show-checkpoint",
          additionalInfo: { checkpointId },
        });
      }
    });

  // Delete Checkpoint Command
  checkpointCmd
    .command("delete <checkpoint-id>")
    .description("Delete checkpoint")
    .option("-f, --force", "Forced deletion without prompting for confirmation")
    .action(async (checkpointId, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete checkpoint: ${checkpointId}`);
          // In practice, an interactive confirmation can be added here
          output.infoLog("Skip confirmation with the --force option");
          return;
        }

        const adapter = new WorkflowExecutionCheckpointAdapter();
        await adapter.deleteCheckpoint(checkpointId);
      } catch (error) {
        handleError(error, {
          operation: "delete-checkpoint",
          additionalInfo: { checkpointId },
        });
      }
    });

  // --- checkpoint latest <execution-id> ---
  checkpointCmd
    .command("latest <execution-id>")
    .description("Get the latest checkpoint for a workflow execution")
    .action(async (executionId: string) => {
      try {
        const adapter = new WorkflowExecutionCheckpointAdapter();
        const checkpoint = await adapter.getLatestCheckpoint(executionId);

        router.render(checkpoint, {
          type: "detail",
          entity: "checkpoint",
          format: () => formatCheckpoint(checkpoint as any, { verbose: false }),
        });
      } catch (error) {
        handleError(error, {
          operation: "checkpoint-latest",
          additionalInfo: { executionId },
        });
      }
    });

  // --- checkpoint stats ---
  checkpointCmd
    .command("stats")
    .description("Show checkpoint statistics")
    .action(async () => {
      try {
        const adapter = new WorkflowExecutionCheckpointAdapter();
        const stats = await adapter.getStatistics();

        router.render(stats, {
          type: "detail",
          entity: "checkpoint-stats",
          format: () => JSON.stringify(stats, null, 2),
        });
      } catch (error) {
        handleError(error, { operation: "checkpoint-stats" });
      }
    });

  // --- checkpoint query ---
  checkpointCmd
    .command("query")
    .description("Query checkpoints with filters")
    .option("--execution-id <id>", "Filter by execution ID")
    .option("--workflow-id <id>", "Filter by workflow ID")
    .option("--type <type>", "Filter by checkpoint type (FULL, DELTA)")
    .option("--tags <tags>", "Filter by tags (comma-separated)")
    .action(async (options: Record<string, string>) => {
      try {
        const filter: Record<string, unknown> = {};
        if (options["executionId"]) filter["executionId"] = options["executionId"];
        if (options["workflowId"]) filter["workflowId"] = options["workflowId"];
        if (options["type"]) filter["type"] = options["type"];
        if (options["tags"]) filter["tags"] = options["tags"].split(",").map((t: string) => t.trim());

        const adapter = new WorkflowExecutionCheckpointAdapter();
        const checkpoints = await adapter.queryCheckpoints(filter);

        router.render(checkpoints, {
          type: "list",
          entity: "checkpoint",
          format: () => formatCheckpointList(checkpoints, { table: true }),
          metadata: { total: checkpoints.length },
        });
      } catch (error) {
        handleError(error, { operation: "checkpoint-query" });
      }
    });

  // --- checkpoint range <execution-id> <from> <to> ---
  checkpointCmd
    .command("range <execution-id> <from> <to>")
    .description("Get checkpoints within a time range (timestamp in ms)")
    .action(async (executionId: string, from: string, to: string) => {
      try {
        const startTime = parseInt(from, 10);
        const endTime = parseInt(to, 10);

        if (isNaN(startTime) || isNaN(endTime)) {
          output.warnLog("Invalid timestamp. Use Unix timestamps in milliseconds.");
          return;
        }

        const adapter = new WorkflowExecutionCheckpointAdapter();
        const checkpoints = await adapter.getCheckpointsByTimeRange(executionId, startTime, endTime);

        router.render(checkpoints, {
          type: "list",
          entity: "checkpoint",
          format: () => formatCheckpointList(checkpoints, { table: true }),
          metadata: { total: checkpoints.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "checkpoint-range",
          additionalInfo: { executionId, from, to },
        });
      }
    });

  // --- checkpoint by-tags <execution-id> <tags...> ---
  checkpointCmd
    .command("by-tags <execution-id> <tags...>")
    .description("Get checkpoints filtered by tags")
    .action(async (executionId: string, tags: string[]) => {
      try {
        const adapter = new WorkflowExecutionCheckpointAdapter();
        const checkpoints = await adapter.getCheckpointsByTags(executionId, tags);

        router.render(checkpoints, {
          type: "list",
          entity: "checkpoint",
          format: () => formatCheckpointList(checkpoints, { table: true }),
          metadata: { total: checkpoints.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "checkpoint-by-tags",
          additionalInfo: { executionId, tags },
        });
      }
    });

  // --- checkpoint batch <ids...> ---
  checkpointCmd
    .command("batch <ids...>")
    .description("Batch query checkpoints by IDs")
    .action(async (ids: string[]) => {
      try {
        const adapter = new WorkflowExecutionCheckpointAdapter();
        const checkpoints = await adapter.getCheckpointsById(ids);

        router.render(checkpoints, {
          type: "list",
          entity: "checkpoint",
          format: () => formatCheckpointList(checkpoints, { table: true }),
          metadata: { total: checkpoints.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "checkpoint-batch",
          additionalInfo: { ids },
        });
      }
    });

  // --- checkpoint chain <id> ---
  checkpointCmd
    .command("chain <id>")
    .description("Show checkpoint chain (parent chain from a checkpoint)")
    .action(async (id: string) => {
      try {
        const adapter = new WorkflowExecutionCheckpointAdapter();
        const chain = await adapter.getCheckpointChainFrom(id);

        router.render(chain, {
          type: "list",
          entity: "checkpoint-chain",
          format: () =>
            chain.length === 0
              ? "No chain entries found"
              : chain
                  .map((cp: any, i: number) =>
                    [
                      `[${i}] ${cp.id || cp.checkpointId}`,
                      `    Type: ${cp.type || "unknown"}`,
                      `    Timestamp: ${cp.timestamp || cp.createdAt || "unknown"}`,
                      cp.previousCheckpointId
                        ? `    Previous: ${cp.previousCheckpointId}`
                        : null,
                    ]
                      .filter(Boolean)
                      .join("\n"),
                  )
                  .join("\n\n"),
          metadata: { total: chain.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "checkpoint-chain",
          additionalInfo: { id },
        });
      }
    });

  return checkpointCmd;
}
