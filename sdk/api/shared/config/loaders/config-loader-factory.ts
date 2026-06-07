/**
 * Config Loader Factory
 *
 * Provides a higher-order function that eliminates the repetitive
 * "read → detect → parse → merge" pattern shared by config loaders.
 *
 * Each config-specific loader (metrics, timeout, file-checkpoint) differs
 * only in its merge function and component name — this factory captures
 * the invariant skeleton and lets callers supply the variable parts.
 */

import { tryLoadConfigFile } from "./config-file-loader.js";
import { parseToml } from "../parsers/toml-parser.js";
import { parseJson } from "../parsers/json-parser.js";
import { createContextualLogger } from "../../../../utils/contextual-logger.js";
import { getErrorOrNew } from "@wf-agent/common-utils";

/**
 * Create a standardised config-file loader.
 *
 * The returned function:
 *  1. Resolves & checks the file path
 *  2. Reads the file and detects TOML/JSON format
 *  3. Parses the content
 *  4. Merges the parsed result with application defaults via `mergeWithDefaults`
 *  5. Falls back to defaults when the file is missing or unreadable
 *
 * @typeParam TConfig - The shape of the merged config object.
 * @param mergeWithDefaults - Pure function that merges a partial config with defaults.
 * @param componentName - Component label used in log output.
 * @returns An async loader function: `(filePath, ...extraMergeArgs) => Promise<TConfig>`.
 */
export function createConfigFileLoader<TConfig>(
  mergeWithDefaults: (partial: Partial<TConfig>, ...extraArgs: any[]) => TConfig,
  componentName: string,
): (filePath: string, ...extraArgs: any[]) => Promise<TConfig> {
  const logger = createContextualLogger({ component: componentName });

  return async (filePath: string, ...extraArgs: any[]): Promise<TConfig> => {
    try {
      const { resolve } = await import("path");
      const resolvedPath = resolve(filePath);

      const loaded = await tryLoadConfigFile(resolvedPath);
      if (loaded === null) {
        logger.warn(`${componentName} config file not found`, { filePath: resolvedPath });
        return mergeWithDefaults({}, ...extraArgs);
      }

      const { content, format } = loaded;
      const parsed: unknown =
        format === "toml"
          ? parseToml(content)
          : parseJson(content);

      logger.info(`Loaded ${componentName} config from file`, { filePath: resolvedPath });
      return mergeWithDefaults(parsed as Partial<TConfig>, ...extraArgs);
    } catch (error) {
      logger.warn(`Failed to load ${componentName} config from file, using defaults`, {
        filePath,
        error: getErrorOrNew(error),
      });
      return mergeWithDefaults({}, ...extraArgs);
    }
  };
}