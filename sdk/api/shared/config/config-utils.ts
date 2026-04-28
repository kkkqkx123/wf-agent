/**
 * Configuration Tool Helper Functions
 * Provide auxiliary functions related to configuration files
 */

import * as fs from "fs/promises";
import * as path from "path";
import { ConfigFormat } from "./types.js";
import { isError } from "@wf-agent/common-utils";

/**
 * Detect the configuration format based on the file extension.
 * @param filePath File path
 * @returns Configuration format
 * @throws {Error} Throws an error when the file extension cannot be recognized.
 */
export function detectConfigFormat(filePath: string): ConfigFormat {
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
 * Read the content of the configuration file
 * @param filePath File path
 * @returns String containing the file content
 * @throws {Error} Throws an error if the file cannot be read
 */
export async function readConfigFile(filePath: string): Promise<string> {
  try {
    return await fs.readFile(filePath, "utf-8");
  } catch (error) {
    if (isError(error)) {
      throw new Error(`Unable to read configuration file：${error.message}`, { cause: error });
    }
    throw new Error("Failed to read the configuration file: Unknown error.", { cause: error });
  }
}

/**
 * Load configuration content from the file path and check its format.
 * @param filePath File path
 * @returns An object containing the content and format
 */
export async function loadConfigContent(filePath: string): Promise<{
  content: string;
  format: ConfigFormat;
}> {
  const content = await readConfigFile(filePath);
  const format = detectConfigFormat(filePath);
  return { content, format };
}
