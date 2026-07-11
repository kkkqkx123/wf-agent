/**
 * Search Adapter
 * Wraps SearchAPI for cross-resource search
 */

import { BaseAdapter } from "./base-adapter.js";
import type {
  SearchResourceType,
  SearchOptions,
  SearchResultItem,
} from "@wf-agent/sdk/api";

export interface SearchParams {
  query: string;
  types?: SearchResourceType[];
  limit?: number;
  offset?: number;
}

export type SearchResults = Record<SearchResourceType, SearchResultItem[]>;

export class SearchAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  /**
   * Cross-resource search
   */
  async search(params: SearchParams): Promise<SearchResults> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.getFactory().createSearchAPI();

      const options: SearchOptions = {
        types: params.types,
        limitPerType: params.limit || 20,
        maxTotal: params.offset || 100,
      };

      const result = await api.search(params.query, options);

      // Transform SearchResult to SearchResults (Record type)
      const searchResults: Partial<SearchResults> = {};
      for (const item of result.items) {
        const type = item.type as SearchResourceType;
        if (!searchResults[type]) {
          searchResults[type] = [];
        }
        searchResults[type]!.push(item);
      }

      this.logOperation(`Search completed: found ${result.total} results`);
      return searchResults as SearchResults;
    }, "Search cross-resource");
  }

  /**
   * Search specific resource type
   */
  async searchByType(
    query: string,
    type: SearchResourceType,
    limit?: number
  ): Promise<SearchResults> {
    return this.search({
      query,
      types: [type],
      limit: limit || 20,
    });
  }

  /**
   * Format search results by type
   */
  formatResults(results: SearchResults): Record<string, unknown[]> {
    const formatted: Record<string, unknown[]> = {};

    for (const [type, items] of Object.entries(results)) {
      if (!items) continue;
      formatted[type] = items.map(item => ({
        id: item.id,
        name: item.label,
        score: item.score || 0,
        type: item.type,
      }));
    }

    return formatted;
  }
}
