/**
 * Search Service
 *
 * Provides high-level file and content search functionality.
 * Uses ripgrep executor for performance and fuzzy matching for file search.
 */

import * as path from "path";
import * as fs from "fs";
import { RipgrepExecutor } from "../executors/implementations/ripgrep/index.js";
import { sortByFuzzyMatch } from "./fuzzy/index.js";
import type {
  FileSearchOptions,
  ListAllFilesOptions,
  FileSearchResult,
} from "./types.js";

/**
 * Search Service class
 */
export class SearchService {
  private ripgrepExecutor: RipgrepExecutor;

  constructor() {
    this.ripgrepExecutor = new RipgrepExecutor();
  }

  /**
   * Initialize the service
   */
  async initialize(): Promise<void> {
    await this.ripgrepExecutor.initialize();
  }

  /**
   * List all files in workspace
   */
  async listAllFiles(options: ListAllFilesOptions): Promise<FileSearchResult[]> {
    const { workspacePath, limit = 10000 } = options;

    try {
      const files = await this.ripgrepExecutor.listFiles({
        workspacePath,
        limit,
      });

      return files.map((file) => ({
        path: file.path,
        type: file.type,
        label: file.label,
      }));
    } catch (error) {
      console.error("Error listing files:", error);
      return [];
    }
  }

  /**
   * Search for files matching a query using fuzzy matching
   */
  async searchFiles(options: FileSearchOptions): Promise<FileSearchResult[]> {
    const { query, workspacePath, limit = 20 } = options;

    try {
      // Get all files
      const allFiles = await this.listAllFiles({ workspacePath });

      // If no query, return top items
      if (!query.trim()) {
        return allFiles.slice(0, limit);
      }

      // Perform fuzzy search
      const matched = sortByFuzzyMatch(
        allFiles,
        query,
        (item) => `${item.path} ${item.label || ""}`,
      );

      // Take top results
      const topResults = matched.slice(0, limit).map(({ item }) => item);

      // Verify types of results
      const verifiedResults = await Promise.all(
        topResults.map(async (result) => {
          const fullPath = path.join(workspacePath, result.path);
          try {
            if (fs.existsSync(fullPath)) {
              const stats = fs.lstatSync(fullPath);
              return {
                ...result,
                type: stats.isDirectory() ? ("folder" as const) : ("file" as const),
              };
            }
          } catch {
            // If path doesn't exist, keep original type
          }
          return result;
        }),
      );

      return verifiedResults;
    } catch (error) {
      console.error("Error in searchFiles:", error);
      return [];
    }
  }

  /**
   * Search content in files using ripgrep
   */
  async searchContent(options: {
    cwd: string;
    directoryPath: string;
    pattern: string;
    filePattern?: string;
    contextLines?: number;
    maxResults?: number;
  }): Promise<string> {
    return this.ripgrepExecutor.searchContent(options);
  }
}
