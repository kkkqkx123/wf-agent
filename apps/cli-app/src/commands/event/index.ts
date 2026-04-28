/**
 * Event Command Group
 */

import { Command } from "commander";
import { EventAdapter } from "../../adapters/event-adapter.js";
import { getOutput } from "../../utils/output.js";
import { formatEvent, formatEventList } from "../../utils/cli-formatters.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";
import { CLIValidationError } from "../../types/cli-types.js";

const output = getOutput();

/**
 * Create Event Command Group
 */
export function createEventCommands(): Command {
  const eventCmd = new Command("event").description("Manage Events");

  // List event commands
  eventCmd
    .command("list")
    .description("List all events")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .option("--type <type>", "Filter by event type")
    .option("--thread-id <threadId>", "Filter by thread ID")
    .option("--workflow-id <workflowId>", "Filter by workflow ID")
    .option("--limit <limit>", "Limit the number of returned items.")
    .action(
      async (
        options: CommandOptions & {
          type?: string;
          threadId?: string;
          workflowId?: string;
          limit?: string;
        },
      ) => {
        try {
          const adapter = new EventAdapter();
          const filter: any = {};
          if (options.type) filter.type = options.type;
          if (options.threadId) filter.threadId = options.threadId;
          if (options.workflowId) filter.workflowId = options.workflowId;
          if (options.limit) filter.limit = parseInt(options.limit, 10);

          const events = await adapter.listEvents(filter);

          output.output(formatEventList(events, { table: options.table }));
        } catch (error) {
          handleError(error, {
            operation: "listEvents",
            additionalInfo: {
              filter: {
                type: options.type,
                threadId: options.threadId,
                workflowId: options.workflowId,
                limit: options.limit,
              },
            },
          });
        }
      },
    );

  // View event details command
  eventCmd
    .command("show <id>")
    .description("View event details")
    .option("-v, --verbose", "Detailed output")
    .action(async (id, options: CommandOptions) => {
      try {
        const adapter = new EventAdapter();
        const event = await adapter.getEvent(id);

        output.output(formatEvent(event, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "getEvent",
          additionalInfo: { id },
        });
      }
    });

  // Get event statistics command
  eventCmd
    .command("stats")
    .description("Obtain event statistics information")
    .option("--type <type>", "Filter by event type")
    .option("--thread-id <threadId>", "Filter by thread ID")
    .option("--workflow-id <workflowId>", "Filter by workflow ID")
    .action(async (options: { type?: string; threadId?: string; workflowId?: string }) => {
      try {
        const adapter = new EventAdapter();
        const filter: any = {};
        if (options.type) filter.type = options.type;
        if (options.threadId) filter.threadId = options.threadId;
        if (options.workflowId) filter.workflowId = options.workflowId;

        const stats = await adapter.getEventStats(filter);

        output.newLine();
        output.subsection("Event Statistics:");
        output.keyValue("Total", String(stats.total));
        output.newLine();
        output.output("By type:");
        Object.entries(stats.byType).forEach(([type, count]) => {
          output.output(`    ${type}: ${count}`);
        });
        output.newLine();
        output.output("By thread:");
        Object.entries(stats.byThread).forEach(([threadId, count]) => {
          output.output(`    ${threadId.substring(0, 8)}: ${count}`);
        });
        output.newLine();
        output.output("By workflow:");
        Object.entries(stats.byWorkflow).forEach(([workflowId, count]) => {
          output.output(`    ${workflowId.substring(0, 8)}: ${count}`);
        });
      } catch (error) {
        handleError(error, {
          operation: "getEventStats",
          additionalInfo: {
            filter: {
              type: options.type,
              threadId: options.threadId,
              workflowId: options.workflowId,
            },
          },
        });
      }
    });

  // Trim event history command
  eventCmd
    .command("trim <maxSize>")
    .description("Trim the event history, retaining the latest N events.")
    .option("-f, --force", "Forced trimming, without prompting for confirmation")
    .action(
      async (
        maxSize: string,
        options: {
          force?: boolean;
        },
      ) => {
        try {
          const size = parseInt(maxSize, 10);
          if (isNaN(size) || size < 0) {
            handleError(
              new CLIValidationError("Invalid size parameter; it must be a non-negative integer."),
              {
                operation: "trimEventHistory",
                additionalInfo: { maxSize },
              },
            );
            return;
          }

          if (!options.force) {
            output.warnLog(`About to trim event history to: ${size} events`);
            output.infoLog("Use the --force option to skip the confirmation.");
            return;
          }

          const adapter = new EventAdapter();
          const removed = await adapter.trimEventHistory(size);
          output.infoLog(`Trimmed ${size} events, removed ${removed} events`);
        } catch (error) {
          handleError(error, {
            operation: "trimEventHistory",
            additionalInfo: { maxSize },
          });
        }
      },
    );

  return eventCmd;
}
