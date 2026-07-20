/**
 * Search Adapter
 * Cross-resource search across workflows, executions, tasks, checkpoints, and events.
 */

import { BaseAdapter } from "./base-adapter.js";
import type { SearchOptions, SearchResult } from "@wf-agent/sdk/api";

export class SearchAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "Search";
  }

  async search(params: { query: string; types?: string[]; limit?: number }): Promise<SearchResult> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("search", { query: params.query });
      const api = this.sdk.getFactory().createSearchAPI();
      const options: SearchOptions = {
        types: params.types as any,
        limitPerType: params.limit || 20,
        maxTotal: (params.limit || 20) * 5,
      };
      const result = await api.search(params.query, options);
      return result;
    }, "Search");
  }
}