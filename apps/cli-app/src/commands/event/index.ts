/**
 * Event Command Group
 */

import { Command } from "commander";
import { EventAdapter } from "../../adapters/event-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { getFormatter } from "../../utils/formatter.js";
import { formatEvent, formatEventList } from "../../utils/formatters/index.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";
import { CLIValidationError } from "../../types/cli-types.js";

const output = getOutput();
const router = getRouter();

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
    .option("--execution-id <executionId>", "Filter by execution ID")
    .option("--workflow-id <workflowId>", "Filter by workflow ID")
    .option("--limit <limit>", "Limit the number of returned items.")
    .action(
      async (
        options: CommandOptions & {
          type?: string;
          executionId?: string;
          workflowId?: string;
          limit?: string;
        },
      ) => {
        try {
          const adapter = new EventAdapter();
          const filter: Record<string, unknown> = {};
          if (options.type) filter['type'] = options.type;
          if (options.executionId) filter['executionId'] = options.executionId;
          if (options.workflowId) filter['workflowId'] = options.workflowId;
          if (options.limit) filter['limit'] = parseInt(options.limit, 10);

          const events = await adapter.listEvents(filter);

          router.render(events, {
            type: "list",
            entity: "event",
            format: () => formatEventList(events, { table: options.table }),
            metadata: { total: events.length },
          });
        } catch (error) {
          handleError(error, {
            operation: "listEvents",
            additionalInfo: {
              filter: {
                type: options.type,
                executionId: options.executionId,
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

        router.render(event, {
          type: "detail",
          entity: "event",
          format: () => formatEvent(event, { verbose: options.verbose }),
        });
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
    .option("--execution-id <executionId>", "Filter by execution ID")
    .option("--workflow-id <workflowId>", "Filter by workflow ID")
    .action(async (options: { type?: string; executionId?: string; workflowId?: string }) => {
      try {
        const adapter = new EventAdapter();
        const filter: Record<string, unknown> = {};
        if (options.type) filter['type'] = options.type;
        if (options.executionId) filter['executionId'] = options.executionId;
        if (options.workflowId) filter['workflowId'] = options.workflowId;

        const stats = await adapter.getEventStats(filter);

        output.newLine();
        output.output(getFormatter().subsection("Event Statistics:"));
        output.output(getFormatter().keyValue("Total", String(stats.total)));
        output.newLine();
        output.output("By type:");
        Object.entries(stats.byType).forEach(([type, count]) => {
          output.output(`    ${type}: ${count}`);
        });
        output.newLine();
        output.output("By execution:");
        Object.entries(stats.byExecution).forEach(([executionId, count]) => {
          output.output(`    ${executionId.substring(0, 8)}: ${count}`);
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
              executionId: options.executionId,
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

  // Subscribe to real-time events
  eventCmd
    .command("subscribe")
    .description("Subscribe to real-time events (Ctrl+C to stop)")
    .option("--type <type>", "Filter by event type")
    .option("--execution-id <executionId>", "Filter by execution ID")
    .option("--workflow-id <workflowId>", "Filter by workflow ID")
    .action(async (options: { type?: string; executionId?: string; workflowId?: string }) => {
      try {
        output.infoLog("Starting event subscription...");
        output.infoLog("Press Ctrl+C to stop listening.");

        // Use the SDK-Kit EventManager if available, otherwise fall back to polling
        try {
          const { SDKKit } = await import("@wf-agent/sdk-kit");
          const { getSDKInstance } = await import("../../services/sdk-globals.js");
          const sdk = getSDKInstance();
          if (sdk) {
            const kit = new SDKKit(sdk);
            const events = kit.events();

            const { ExecutionEventType } = await import("@wf-agent/sdk-kit");

            // Subscribe to execution events
            const unsubStart = events.subscribe(
              ExecutionEventType.EXECUTION_START,
              (payload) => {
                output.output(`[START] Execution ${payload.executionId} started`);
              },
            );

            const unsubProgress = events.subscribe(
              ExecutionEventType.EXECUTION_PROGRESS,
              (payload) => {
                output.output(`[PROGRESS] Execution ${payload.executionId}: ${JSON.stringify(payload.data)}`);
              },
            );

            const unsubCompleted = events.subscribe(
              ExecutionEventType.EXECUTION_COMPLETED,
              (payload) => {
                output.output(`[COMPLETED] Execution ${payload.executionId} completed`);
              },
            );

            const unsubFailed = events.subscribe(
              ExecutionEventType.EXECUTION_FAILED,
              (payload) => {
                output.output(`[FAILED] Execution ${payload.executionId}: ${payload.error?.message || "Unknown error"}`);
              },
            );

            // Wait for Ctrl+C
            await new Promise<void>((resolve) => {
              process.on("SIGINT", () => {
                output.infoLog("\nStopping event subscription...");
                unsubStart();
                unsubProgress();
                unsubCompleted();
                unsubFailed();
                resolve();
              });
            });
            return;
          }
        } catch {
          // Fallback to polling approach
        }

        // Fallback: Poll events via EventAdapter
        output.infoLog("Using polling fallback (SDK-Kit EventManager not available)");
        const adapter = new EventAdapter();
        const filter: Record<string, unknown> = { limit: 50 };
        if (options.type) filter["type"] = options.type;
        if (options.executionId) filter["executionId"] = options.executionId;
        if (options.workflowId) filter["workflowId"] = options.workflowId;

        let lastEventCount = 0;
        const pollInterval = setInterval(async () => {
          try {
            const events = await adapter.listEvents(filter as any);
            if (events.length > lastEventCount) {
              const newEvents = events.slice(lastEventCount);
              newEvents.forEach((event) => {
                output.output(`[${event.type}] ${event.executionId} - ${new Date(event.timestamp).toISOString()}`);
              });
              lastEventCount = events.length;
            }
          } catch {
            // Silently continue polling
          }
        }, 2000);

        await new Promise<void>((resolve) => {
          process.on("SIGINT", () => {
            clearInterval(pollInterval);
            output.infoLog("\nStopped event subscription.");
            resolve();
          });
        });
      } catch (error) {
        handleError(error, { operation: "event-subscribe" });
      }
    });

  // Event timeline command
  eventCmd
    .command("timeline <execution-id>")
    .description("Show execution event timeline")
    .option("--json", "Output as JSON")
    .option("-v, --verbose", "Detailed output")
    .action(async (executionId, options: { json?: boolean; verbose?: boolean }) => {
      try {
        const adapter = new EventAdapter();
        const events = await adapter.listEvents({ executionId } as any);

        if (events.length === 0) {
          output.info("No events found for this execution");
          return;
        }

        // Sort by timestamp
        events.sort((a, b) => (a.timestamp || 0) - (b.timestamp || 0));

        if (options.json) {
          output.output(getFormatter().json(events));
          return;
        }

        output.newLine();
        output.output(getFormatter().subsection(`Event Timeline for Execution: ${executionId}`));
        output.output("─".repeat(60));

        const startTime = events[0]?.timestamp || 0;
        events.forEach((event, index) => {
          const relative = ((event.timestamp || 0) - startTime).toFixed(0);
          const time = new Date(event.timestamp || 0).toISOString();
          output.output(`  ${index + 1}. [+${relative}ms] ${event.type}`);
          if (options.verbose) {
            output.output(`     Time: ${time}`);
            output.output(`     ID: ${event.id}`);
            if ((event as any).data) {
              output.output(`     Data: ${JSON.stringify((event as any).data)}`);
            }
          }
        });

        output.newLine();
        output.info(`Total events: ${events.length}`);
      } catch (error) {
        handleError(error, {
          operation: "event-timeline",
          additionalInfo: { executionId },
        });
      }
    });

  return eventCmd;
}
