/**
 * Ripgrep Executor
 *
 * Provides ripgrep-specific functionality for content search and file listing.
 */

import * as path from "path";
import * as readline from "readline";
import * as childProcess from "child_process";
import { BaseExecutor } from "../../BaseExecutor.js";
import type { ExecutionOptions } from "../../types.js";
import type {
  RipgrepSearchOptions,
  RipgrepListFilesOptions,
  SearchFileResult,
  SearchResult,
  SearchLineResult,
  FileResult,
} from "./types.js";

/**
 * Constants
 */
const MAX_RESULTS = 300;
const MAX_LINE_LENGTH = 500;
const isWindows = process.platform.startsWith("win");

/**
 * Truncates a line if it exceeds the maximum length
 */
export function truncateLine(line: string, maxLength: number = MAX_LINE_LENGTH): string {
  return line.length > maxLength ? line.substring(0, maxLength) + " [truncated...]" : line;
}

/**
 * Ripgrep Executor class
 */
export class RipgrepExecutor extends BaseExecutor {
  constructor(customPath?: string) {
    super({
      name: "ripgrep",
      binaryName: isWindows ? "rg.exe" : "rg",
      customPath,
    });
  }

  /**
   * Get default search paths for ripgrep binary
   */
  protected getDefaultPaths(): string[] {
    if (isWindows) {
      return [
        "C:\\Program Files\\ripgrep\\rg.exe",
        "C:\\Program Files (x86)\\ripgrep\\rg.exe",
        path.join(process.env["LOCALAPPDATA"] ?? "", "ripgrep", "rg.exe"),
      ];
    } else {
      return [
        "/usr/local/bin/rg",
        "/usr/bin/rg",
        "/opt/homebrew/bin/rg",
        path.join(process.env["HOME"] ?? "", ".local", "bin", "rg"),
      ];
    }
  }

  /**
   * Perform content search using ripgrep
   */
  async searchContent(options: RipgrepSearchOptions): Promise<string> {
    const bin = await this.ensureInitialized();

    const {
      cwd,
      directoryPath,
      pattern,
      filePattern,
      contextLines = 1,
      maxResults = MAX_RESULTS,
      maxLineLength = MAX_LINE_LENGTH,
    } = options;

    const args = ["--json", "-e", pattern];

    if (filePattern) {
      args.push("--glob", filePattern);
    }

    args.push("--context", String(contextLines), "--no-messages", directoryPath);

    const execOptions: ExecutionOptions = {
      args,
      maxLines: maxResults * 5,
    };

    const result = await this.execute(execOptions);

    if (!result.success || !result.stdout) {
      return "No results found";
    }

    const results: SearchFileResult[] = [];
    let currentFile: SearchFileResult | null = null;

    result.stdout.split("\n").forEach((line) => {
      if (line) {
        try {
          const parsed = JSON.parse(line);
          if (parsed.type === "begin") {
            currentFile = {
              file: parsed.data.path.text.toString(),
              searchResults: [],
            };
          } else if (parsed.type === "end") {
            if (currentFile) {
              results.push(currentFile);
            }
            currentFile = null;
          } else if ((parsed.type === "match" || parsed.type === "context") && currentFile) {
            const searchLine: SearchLineResult = {
              line: parsed.data.line_number,
              text: truncateLine(parsed.data.lines.text, maxLineLength),
              isMatch: parsed.type === "match",
              ...(parsed.type === "match" && { column: parsed.data.absolute_offset }),
            };

            const lastResult = currentFile.searchResults[currentFile.searchResults.length - 1];
            if (lastResult && lastResult.lines.length > 0) {
              const lastLine = lastResult.lines[lastResult.lines.length - 1];

              if (lastLine && parsed.data.line_number <= lastLine.line + 1) {
                lastResult.lines.push(searchLine);
              } else {
                currentFile.searchResults.push({
                  lines: [searchLine],
                });
              }
            } else {
              currentFile.searchResults.push({
                lines: [searchLine],
              });
            }
          }
        } catch {
          // Ignore parse errors
        }
      }
    });

    return this.formatResults(results, cwd, maxResults);
  }

  /**
   * Format search results for display
   */
  private formatResults(
    fileResults: SearchFileResult[],
    cwd: string,
    maxResults: number,
  ): string {
    const groupedResults: { [key: string]: SearchResult[] } = {};

    let totalResults = fileResults.reduce(
      (sum, file) => sum + file.searchResults.length,
      0,
    );
    let output = "";

    if (totalResults >= maxResults) {
      output += `Showing first ${maxResults} of ${maxResults}+ results. Use a more specific search if necessary.\n\n`;
    } else {
      output += `Found ${totalResults === 1 ? "1 result" : `${totalResults.toLocaleString()} results`}.\n\n`;
    }

    fileResults.slice(0, maxResults).forEach((file) => {
      const relativeFilePath = path.relative(cwd, file.file).replace(/\\/g, "/");
      if (!groupedResults[relativeFilePath]) {
        groupedResults[relativeFilePath] = [];
        groupedResults[relativeFilePath].push(...file.searchResults);
      }
    });

    for (const [filePath, results] of Object.entries(groupedResults)) {
      output += `# ${filePath}\n`;

      results.forEach((result) => {
        if (result.lines.length > 0) {
          result.lines.forEach((line) => {
            const lineNumber = String(line.line).padStart(3, " ");
            output += `${lineNumber} | ${line.text.trimEnd()}\n`;
          });
          output += "----\n";
        }
      });

      output += "\n";
    }

    return output.trim();
  }

  /**
   * List files using ripgrep
   */
  async listFiles(options: RipgrepListFilesOptions): Promise<FileResult[]> {
    const bin = await this.ensureInitialized();

    const {
      workspacePath,
      limit = 500,
      follow = true,
      hidden = true,
      excludePatterns = ["**/node_modules/**", "**/.git/**", "**/out/**", "**/dist/**"],
    } = options;

    const args = ["--files"];

    if (follow) {
      args.push("--follow");
    }

    if (hidden) {
      args.push("--hidden");
    }

    for (const pattern of excludePatterns) {
      args.push("-g", `!${pattern}`);
    }

    args.push(workspacePath);

    return new Promise((resolve, reject) => {
      const proc = childProcess.spawn(bin, args);
      const rl = readline.createInterface({
        input: proc.stdout,
        crlfDelay: Infinity,
      });

      const fileResults: FileResult[] = [];
      const dirSet = new Set<string>();
      let count = 0;

      rl.on("line", (line) => {
        if (count < limit) {
          try {
            const relativePath = path.relative(workspacePath, line).replace(/\\/g, "/");

            fileResults.push({
              path: relativePath,
              type: "file",
              label: path.basename(relativePath),
            });

            let dirPath = path.dirname(relativePath);

            while (dirPath && dirPath !== "." && dirPath !== "/") {
              dirSet.add(dirPath);
              dirPath = path.dirname(dirPath);
            }

            count++;
          } catch {
            // Ignore errors
          }
        } else {
          rl.close();
          proc.kill();
        }
      });

      let errorOutput = "";
      proc.stderr.on("data", (data) => {
        errorOutput += data.toString();
      });

      rl.on("close", () => {
        if (errorOutput && fileResults.length === 0) {
          reject(new Error(`ripgrep process error: ${errorOutput}`));
        } else {
          const dirResults = Array.from(dirSet).map((dirPath) => ({
            path: dirPath,
            type: "folder" as const,
            label: path.basename(dirPath),
          }));

          resolve([...fileResults, ...dirResults]);
        }
      });

      proc.on("error", (error) => {
        reject(new Error(`ripgrep process error: ${error.message}`));
      });
    });
  }
}
