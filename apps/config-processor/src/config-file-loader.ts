/**
 * Configuration File Loader
 *
 * File I/O operations for reading config files from disk.
 * Lives in the application layer so the SDK remains I/O-free.
 */

import * as fs from "fs/promises";
import { ConfigFormat, getConfigFormatFromPath } from "@wf-agent/sdk/api";

/**
 * Read configuration file content.
 * @param filePath - Configuration file path.
 * @returns File content as string.
 * @throws Error if file cannot be read.
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
 * Load configuration file content and detect format.
 * Convenience function that combines readConfigFile and getConfigFormatFromPath.
 * @param filePath - Configuration file path.
 * @returns Object containing file content and detected format.
 * @throws Error if file cannot be read or format is not recognised.
 */
export async function loadConfigFile(
  filePath: string,
): Promise<{ content: string; format: ConfigFormat }> {
  const content = await readConfigFile(filePath);
  const format = getConfigFormatFromPath(filePath);
  return { content, format };
}

/**
 * Safely load configuration file content and detect format.
 * Returns null if the file does not exist; throws on other errors.
 * @param filePath - Configuration file path.
 * @returns Object containing file content and detected format, or null if file not found.
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