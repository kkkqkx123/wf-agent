/**
 * Query Command Group
 * Provides a unified query interface for execution records
 */

import { Command } from "commander";
import { getRouter } from "../../utils/output-router.js";
import { getFormatter } from "../../utils/formatter.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const router = getRouter();

interface QueryOptions extends CommandOptions {
  status?: string;
  workflowId?: string;
  profileId?: string;
  execId?: string;
  limit?: string;
  offset?: string;
  sort?: string;
  order?: string;
  json?: boolean;
  table?: boolean;
}

/**
 * Internal query builder state
 */
interface QueryState {
  resourceType: string;
  filters: Record<string, unknown>;
  sortField?: string;
  sortOrder: "asc" | "desc";
  maxResults: number;
  offset: number;
}

/**
 * Create Query Command Group
 */
export function createQueryCommands(): Command {
  const queryCmd = new Command("query").description("Query execution records with filters");

  // Query executions
  queryCmd
    .command("executions")
    .description("Query workflow execution records")
    .option("--status <status>", "Filter by status (running/completed/failed/cancelled)")
    .option("--workflow-id <workflowId>", "Filter by workflow ID")
    .option("--profile-id <profileId>", "Filter by LLM profile ID")
    .option("--limit <number>", "Maximum results to return", "20")
    .option("--offset <number>", "Number of results to skip", "0")
    .option("--sort <field>", "Sort field (createdAt, status, duration)")
    .option("--order <order>", "Sort order (asc/desc)", "desc")
    .option("--json", "Output as JSON")
    .option("--table", "Output as table")
    .action(async (options: QueryOptions) => {
      try {
        const state: QueryState = {
          resourceType: "execution",
          filters: {},
          maxResults: parseInt(options.limit || "20", 10),
          offset: parseInt(options.offset || "0", 10),
          sortOrder: (options.order as "asc" | "desc") || "desc",
        };

        if (options.status) state.filters["status"] = options.status;
        if (options.workflowId) state.filters["workflowId"] = options.workflowId;
        if (options.profileId) state.filters["profileId"] = options.profileId;
        if (options.sort) state.sortField = options.sort;

        // Use the SDK execution registry API
        const { getSDKInstance } = await import("../../services/sdk-globals.js");
        const sdk = getSDKInstance();
        if (!sdk) {
          throw new Error("SDK instance not available");
        }

        const api = sdk.executions;
        const executions = await api.getAll(state.filters as any);

        // Apply limit and offset
        const total = executions.length;
        const paged = executions.slice(state.offset, state.offset + state.maxResults);

        if (options.json) {
          router.render(paged, {
            type: "list",
            entity: "execution",
            format: () => getFormatter().json(paged),
            metadata: { total, returned: paged.length, offset: state.offset },
          });
          return;
        }

        if (options.table) {
          const headers = ["Execution ID", "Workflow ID", "Status", "Created"];
          const rows = paged.map((e: any) => [
            (e.id || "").substring(0, 8),
            (e.workflowId || "").substring(0, 8),
            e.status || "N/A",
            e.createdAt ? new Date(e.createdAt).toISOString().substring(0, 19) : "N/A",
          ]);
          router.render(paged, {
            type: "list",
            entity: "execution",
            format: () => getFormatter().table(headers, rows),
            metadata: { total, returned: paged.length },
          });
          return;
        }

        // Default text output
        const lines: string[] = [];
        lines.push(getFormatter().subsection(`Execution Records (${paged.length}/${total}):`));
        paged.forEach((e: any, i: number) => {
          const id = (e.id || "N/A").substring(0, 12);
          const wf = (e.workflowId || "N/A").substring(0, 12);
          lines.push(`  ${i + 1}. ${id} | ${wf} | ${e.status || "N/A"}`);
        });
        if (paged.length < total) {
          lines.push(`\n  ... and ${total - paged.length} more results`);
        }
        lines.push(`\n  Total: ${total} execution(s)`);

        router.render(paged, {
          type: "list",
          entity: "execution",
          format: () => lines.join("\n"),
          metadata: { total, returned: paged.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "query-executions",
          additionalInfo: { filters: options },
        });
      }
    });

  // Query agent loops
  queryCmd
    .command("agent-loops")
    .description("Query agent loop records")
    .option("--status <status>", "Filter by status (running/paused/completed/failed)")
    .option("--profile-id <profileId>", "Filter by LLM profile ID")
    .option("--limit <number>", "Maximum results to return", "20")
    .option("--offset <number>", "Number of results to skip", "0")
    .option("--json", "Output as JSON")
    .option("--table", "Output as table")
    .action(async (options: QueryOptions) => {
      try {
        const maxResults = parseInt(options.limit || "20", 10);
        const offset = parseInt(options.offset || "0", 10);

        const { getSDKInstance } = await import("../../services/sdk-globals.js");
        const sdk = getSDKInstance();
        if (!sdk) {
          throw new Error("SDK instance not available");
        }

        const api = sdk.getFactory().createAgentLoopRegistryAPI();
        const filter: Record<string, unknown> = {};
        if (options.status) filter["status"] = options.status;
        if (options.profileId) filter["profileId"] = options.profileId;

        const loops = await api.getAll(filter as any);
        const total = loops.length;
        const paged = loops.slice(offset, offset + maxResults);

        if (options.json) {
          router.render(paged, {
            type: "list",
            entity: "agent-loop",
            format: () => getFormatter().json(paged),
            metadata: { total, returned: paged.length },
          });
          return;
        }

        if (options.table) {
          const headers = ["ID", "Profile", "Status", "Iterations"];
          const rows = paged.map((l: any) => [
            (l.id || "").substring(0, 8),
            l.profileId || "N/A",
            l.status || "N/A",
            String(l.currentIteration || l.iterationCount || 0),
          ]);
          router.render(paged, {
            type: "list",
            entity: "agent-loop",
            format: () => getFormatter().table(headers, rows),
            metadata: { total, returned: paged.length },
          });
          return;
        }

        const lines: string[] = [];
        lines.push(getFormatter().subsection(`Agent Loops (${paged.length}/${total}):`));
        paged.forEach((l: any, i: number) => {
          lines.push(`  ${i + 1}. ${(l.id || "N/A").substring(0, 12)} | ${l.profileId || "N/A"} | ${l.status || "N/A"}`);
        });
        if (paged.length < total) {
          lines.push(`\n  ... and ${total - paged.length} more results`);
        }

        router.render(paged, {
          type: "list",
          entity: "agent-loop",
          format: () => lines.join("\n"),
          metadata: { total, returned: paged.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "query-agent-loops",
          additionalInfo: { filters: options },
        });
      }
    });

  // Query tasks
  queryCmd
    .command("tasks")
    .description("Query task records")
    .option("--status <status>", "Filter by status (queued/running/completed/failed/cancelled/timeout)")
    .option("--execution-id <executionId>", "Filter by execution ID")
    .option("--limit <number>", "Maximum results to return", "20")
    .option("--offset <number>", "Number of results to skip", "0")
    .option("--json", "Output as JSON")
    .option("--table", "Output as table")
    .action(async (options: QueryOptions) => {
      try {
        const maxResults = parseInt(options.limit || "20", 10);
        const offset = parseInt(options.offset || "0", 10);

        const { TaskAdapter } = await import("../../adapters/task-adapter.js");
        const adapter = new TaskAdapter();

        const filter: Record<string, unknown> = {};
        if (options.status) filter["status"] = options.status;
        if (options.execId) filter["executionId"] = options.execId;

        const tasks = await adapter.listTasks(
          Object.keys(filter).length > 0 ? (filter as any) : undefined,
        );
        const total = tasks.length;
        const paged = tasks.slice(offset, offset + maxResults);

        if (options.json) {
          router.render(paged, {
            type: "list",
            entity: "task",
            format: () => getFormatter().json(paged),
            metadata: { total, returned: paged.length },
          });
          return;
        }

        if (options.table) {
          const headers = ["Task ID", "Status", "Type", "Execution ID"];
          const rows = paged.map((t: any) => [
            (t.id || "").substring(0, 8),
            t.status || "N/A",
            t.instanceType || "N/A",
            (t.instance?.id || "").substring(0, 8),
          ]);
          router.render(paged, {
            type: "list",
            entity: "task",
            format: () => getFormatter().table(headers, rows),
            metadata: { total, returned: paged.length },
          });
          return;
        }

        const lines: string[] = [];
        lines.push(getFormatter().subsection(`Tasks (${paged.length}/${total}):`));
        paged.forEach((t: any, i: number) => {
          lines.push(`  ${i + 1}. ${(t.id || "N/A").substring(0, 12)} | ${t.status || "N/A"} | ${t.instanceType || "N/A"}`);
        });
        if (paged.length < total) {
          lines.push(`\n  ... and ${total - paged.length} more results`);
        }

        router.render(paged, {
          type: "list",
          entity: "task",
          format: () => lines.join("\n"),
          metadata: { total, returned: paged.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "query-tasks",
          additionalInfo: { filters: options },
        });
      }
    });

  return queryCmd;
}