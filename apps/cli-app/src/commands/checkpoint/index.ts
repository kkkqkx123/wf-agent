/**
 * Checkpoint Command Group
 */

import { Command } from "commander";
import { ThreadCheckpointAdapter } from "../../adapters/thread-checkpoint-adapter.js";
import { getOutput } from "../../utils/output.js";
import { formatCheckpoint, formatCheckpointList } from "../../utils/cli-formatters.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const output = getOutput();

/**
 * Create Checkpoint Command Group
 */
export function createCheckpointCommands(): Command {
  const checkpointCmd = new Command("checkpoint").description("Manage checkpoints");

  // Create Checkpoint Command
  checkpointCmd
    .command("create <thread-id>")
    .description("Create checkpoint")
    .option("-n, --name <name>", "Name of checkpoint")
    .option("-v, --verbose", "Detailed output")
    .action(async (threadId, options: CommandOptions & { name?: string }) => {
      try {
        output.infoLog(`Creating checkpoint for thread: ${threadId}`);

        const adapter = new ThreadCheckpointAdapter();
        const checkpoint = await adapter.createCheckpoint(threadId, options.name);

        output.output(formatCheckpoint(checkpoint, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "create-checkpoint",
          additionalInfo: { threadId, name: options.name },
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

        const adapter = new ThreadCheckpointAdapter();
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
        const adapter = new ThreadCheckpointAdapter();
        const checkpoints = await adapter.listCheckpoints();

        output.output(formatCheckpointList(checkpoints, { table: options.table }));
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
        const adapter = new ThreadCheckpointAdapter();
        const checkpoint = await adapter.getCheckpoint(checkpointId);

        output.output(formatCheckpoint(checkpoint, { verbose: options.verbose }));
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

        const adapter = new ThreadCheckpointAdapter();
        await adapter.deleteCheckpoint(checkpointId);
      } catch (error) {
        handleError(error, {
          operation: "delete-checkpoint",
          additionalInfo: { checkpointId },
        });
      }
    });

  return checkpointCmd;
}
