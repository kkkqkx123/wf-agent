/**
 * Trigger Command Group
 */

import { Command } from "commander";
import { TriggerAdapter } from "../../adapters/trigger-adapter.js";
import { getOutput } from "../../utils/output.js";
import { formatTrigger, formatTriggerList } from "../../utils/cli-formatters.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const output = getOutput();

/**
 * Create Trigger Command Group
 */
export function createTriggerCommands(): Command {
  const triggerCmd = new Command("trigger").description("Manage Triggers");

  // List trigger commands
  triggerCmd
    .command("list")
    .description("List all triggers")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .action(async (options: CommandOptions) => {
      try {
        const adapter = new TriggerAdapter();
        const triggers = await adapter.listTriggers();

        output.output(formatTriggerList(triggers, { table: options.table }));
      } catch (error) {
        handleError(error, {
          operation: "list-triggers",
        });
      }
    });

  // View Trigger Details Command
  triggerCmd
    .command("show <id>")
    .description("View trigger details")
    .option("-v, --verbose", "Detailed output")
    .action(async (id, options: CommandOptions) => {
      try {
        const adapter = new TriggerAdapter();
        const trigger = await adapter.getTrigger(id);

        output.output(formatTrigger(trigger, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "show-trigger",
          additionalInfo: { id },
        });
      }
    });

  // Enable trigger command
  triggerCmd
    .command("enable <threadId> <triggerId>")
    .description("Enable the trigger")
    .action(async (threadId, triggerId) => {
      try {
        output.infoLog(`Enabling trigger: ${triggerId}`);

        const adapter = new TriggerAdapter();
        await adapter.enableTrigger(threadId, triggerId);
      } catch (error) {
        handleError(error, {
          operation: "enable-trigger",
          additionalInfo: { threadId, triggerId },
        });
      }
    });

  // Disable trigger command
  triggerCmd
    .command("disable <threadId> <triggerId>")
    .description("Disable the trigger.")
    .action(async (threadId, triggerId) => {
      try {
        output.infoLog(`Disabling trigger: ${triggerId}`);

        const adapter = new TriggerAdapter();
        await adapter.disableTrigger(threadId, triggerId);
      } catch (error) {
        handleError(error, {
          operation: "disable-trigger",
          additionalInfo: { threadId, triggerId },
        });
      }
    });

  // List trigger commands by thread
  triggerCmd
    .command("list-by-thread <thread-id>")
    .description("List triggers by thread ID")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .action(async (threadId, options: CommandOptions) => {
      try {
        const adapter = new TriggerAdapter();
        const triggers = await adapter.listTriggersByThread(threadId);

        output.output(formatTriggerList(triggers, { table: options.table }));
      } catch (error) {
        handleError(error, {
          operation: "list-triggers-by-thread",
          additionalInfo: { threadId },
        });
      }
    });

  return triggerCmd;
}
