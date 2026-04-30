/**
 * Message Command Group
 */

import { Command } from "commander";
import { MessageAdapter } from "../../adapters/message-adapter.js";
import { getOutput } from "../../utils/output.js";
import { formatMessage, formatMessageList } from "../../utils/cli-formatters.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";

const output = getOutput();

/**
 * Create message command group
 */
export function createMessageCommands(): Command {
  const messageCmd = new Command("message").description("Manage messages");

  // List message commands
  messageCmd
    .command("list")
    .description("List all messages")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .option("--execution-id <executionId>", "Filter by execution ID")
    .option("--role <role>", "Filter by role")
    .option("--content <content>", "Filter by content keywords")
    .action(
      async (
        options: CommandOptions & {
          executionId?: string;
          role?: string;
          content?: string;
        },
      ) => {
        try {
          const adapter = new MessageAdapter();
          const filter: any = {};
          if (options.executionId) filter.executionId = options.executionId;
          if (options.role) filter.role = options.role;
          if (options.content) filter.content = options.content;

          const messages = await adapter.listMessages(filter);

          output.output(formatMessageList(messages, { table: options.table }));
        } catch (error) {
          handleError(error, {
            operation: "listMessages",
            additionalInfo: {
              filter: { executionId: options.executionId, role: options.role, content: options.content },
            },
          });
        }
      },
    );

  // View message details command
  messageCmd
    .command("show <id>")
    .description("View message details")
    .option("-v, --verbose", "Detailed output")
    .action(async (id, options: CommandOptions) => {
      try {
        const adapter = new MessageAdapter();
        const message = await adapter.getMessage(id);

        output.output(formatMessage(message, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "getMessage",
          additionalInfo: { id },
        });
      }
    });

  // Command to list messages by execution
  messageCmd
    .command("list-by-execution <execution-id>")
    .description("List messages by execution ID")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .action(async (executionId, options: CommandOptions) => {
      try {
        const adapter = new MessageAdapter();
        const messages = await adapter.listMessagesByExecution(executionId);

        output.output(formatMessageList(messages, { table: options.table }));
      } catch (error) {
        handleError(error, {
          operation: "listMessagesByExecution",
          additionalInfo: { executionId },
        });
      }
    });

  // Get message statistics command
  messageCmd
    .command("stats")
    .description("Get message statistics")
    .option("--execution-id <executionId>", "Count by execution ID")
    .action(async (options: { executionId?: string }) => {
      try {
        const adapter = new MessageAdapter();
        const stats = await adapter.getMessageStats(options.executionId);

        output.newLine();
        output.subsection("Message Statistics:");
        output.keyValue("Total", String(stats.total));
        output.newLine();
        output.output("By role:");
        Object.entries(stats.byRole).forEach(([role, count]) => {
          output.output(`    ${role}: ${count}`);
        });
      } catch (error) {
        handleError(error, {
          operation: "getMessageStats",
          additionalInfo: { executionId: options.executionId },
        });
      }
    });

  // Compress context command
  messageCmd
    .command("compress <executionId>")
    .description("Manually trigger context compression")
    .option(
      "-s, --strategy <strategy>",
      "Compression strategies (TRUNCATE/CLEAR, etc.)",
      "TRUNCATE",
    )
    .option("--keep-recent <count>", "Retain the number of recent messages.", "10")
    .action(async (executionId: string, options: any) => {
      try {
        const { EventAdapter } = await import("../../adapters/event-adapter.js");
        const adapter = new EventAdapter();
        const event = {
          type: "CONTEXT_COMPRESSION_REQUESTED",
          timestamp: Date.now(),
          executionId,
          data: {
            strategy: options.strategy,
            keepRecent: parseInt(options.keepRecent, 10),
          },
        };
        await adapter.dispatchEvent(event as any);
        output.newLine();
        output.output(
          `Context compression request sent to execution ${executionId} (strategy: ${options.strategy})`,
        );
        output.output(
          `Note: You need to configure the corresponding trigger in the workflow (trigger event: CONTEXT_COMPRESSION_REQUESTED) to execute.`,
        );
      } catch (error) {
        handleError(error, {
          operation: "dispatchContextCompressionEvent",
          additionalInfo: { executionId, strategy: options.strategy, keepRecent: options.keepRecent },
        });
      }
    });

  return messageCmd;
}
