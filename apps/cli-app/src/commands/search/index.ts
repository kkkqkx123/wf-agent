/**
 * Search Commands
 * Cross-resource search functionality
 */

import { Command } from "commander";
import { SearchAdapter } from "../../adapters/search-adapter.js";
import { getRouter } from "../../utils/output-router.js";
import { handleError } from "../../utils/error-handler.js";
import { formatSearchResults } from "../../utils/formatters/search-formatters.js";
import type { SearchResourceType } from "@wf-agent/sdk/api";

export function createSearchCommand(): Command {
  const search = new Command("search")
    .argument("<query>", "Search query string")
    .description("Search across all resources (workflows, executions, tasks, etc.)")
    .option("-t, --type <type>", "Filter by resource type (workflow|execution|task|checkpoint|event|agent_loop)")
    .option("-l, --limit <number>", "Maximum results per type", "20")
    .action(async (query: string, options) => {
      try {
        const adapter = new SearchAdapter();

        const params = {
          query,
          types: options.type ? [options.type as SearchResourceType] : undefined,
          limit: parseInt(options.limit, 10),
        };

        const results = await adapter.search(params);

        getRouter().render(results, {
          type: "list",
          entity: "search_results",
          format: () => formatSearchResults(results),
          message: `Search found results for "${query}"`,
          metadata: {
            query,
            resultCount: Object.values(results).reduce((sum: number, items: any) => sum + (items?.length || 0), 0),
          },
        });
      } catch (error) {
        handleError(error, {
          operation: "search",
          additionalInfo: { query, type: options.type },
        });
      }
    });

  return search;
}
