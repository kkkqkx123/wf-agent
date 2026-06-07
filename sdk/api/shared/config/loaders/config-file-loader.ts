/**
 * Configuration File Loader
 * 
 * Responsible for file I/O operations related to configuration files.
 * Separates file system operations from configuration parsing logic.
 * 
 * Design Principles:
 * - Pure file I/O operations, no parsing or validation logic
 * - Clear error messages for file access issues
 * - Reusable across different applications (CLI, Web, etc.)
 */

import * as fs from "fs/promises";
import * as path from "path";
import { ConfigFormat } from "../types.js";

/**
 * Read configuration file content
 * @param filePath Configuration file path
 * @returns File content as string
 * @throws Error if file cannot be read
 */
export async function readConfigFile(filePath: string): Promise<string> {
  try {
    return await fs.readFile(filePath, "utf-8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      throw new Error(`Configuration file not found: ${filePath}`, { cause: error });
    }
    throw error;
  }
}

/**
 * Detect configuration format from file extension
 * @param filePath File path
 * @returns Configuration format
 * @throws Error if file extension is not recognized
 */
export function getConfigFormatFromPath(filePath: string): ConfigFormat {
  const ext = path.extname(filePath).toLowerCase();

  switch (ext) {
    case ".toml":
      return "toml";
    case ".json":
      return "json";
    default:
      throw new Error(`Unrecognized configuration file extension: ${ext}`);
  }
}

/**
 * Safely load configuration file content and detect format.
 * Returns null if the file does not exist, throws on other errors.
 * @param filePath Configuration file path
 * @returns Object containing file content and detected format, or null if file not found
 */
export async function tryLoadConfigFile(
  filePath: string,
): Promise<{ content: string; format: ConfigFormat } | null> {
  try {
    return await loadConfigFile(filePath);
  } catch (error) {
    if ((error as Error).message?.startsWith("Configuration file not found")) {
      return null;
    }
    throw error;
  }
}

/**
 * Load configuration file content and detect format
 * Convenience function that combines readConfigFile and getConfigFormatFromPath
 * @param filePath Configuration file path
 * @returns Object containing file content and detected format
 * @throws Error if file cannot be read or format is not recognized
 */
export async function loadConfigFile(
  filePath: string,
): Promise<{ content: string; format: ConfigFormat }> {
  const content = await readConfigFile(filePath);
  const format = getConfigFormatFromPath(filePath);
  return { content, format };
}
