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
    .command("enable <executionId> <triggerId>")
    .description("Enable the trigger")
    .action(async (executionId, triggerId) => {
      try {
        output.infoLog(`Enabling trigger: ${triggerId}`);

        const adapter = new TriggerAdapter();
        await adapter.enableTrigger(executionId, triggerId);
      } catch (error) {
        handleError(error, {
          operation: "enable-trigger",
          additionalInfo: { executionId, triggerId },
        });
      }
    });

  // Disable trigger command
  triggerCmd
    .command("disable <executionId> <triggerId>")
    .description("Disable the trigger.")
    .action(async (executionId, triggerId) => {
      try {
        output.infoLog(`Disabling trigger: ${triggerId}`);

        const adapter = new TriggerAdapter();
        await adapter.disableTrigger(executionId, triggerId);
      } catch (error) {
        handleError(error, {
          operation: "disable-trigger",
          additionalInfo: { executionId, triggerId },
        });
      }
    });

  // List trigger commands by execution entity
  triggerCmd
    .command("list-by-execution <execution-id>")
    .description("List triggers by workflow execution ID")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .action(async (executionId, options: CommandOptions) => {
      try {
        const adapter = new TriggerAdapter();
        const triggers = await adapter.listTriggersByWorkflowExecution(executionId);

        output.output(formatTriggerList(triggers, { table: options.table }));
      } catch (error) {
        handleError(error, {
          operation: "list-triggers-by-execution",
          additionalInfo: { executionId },
        });
      }
    });

  return triggerCmd;
}
