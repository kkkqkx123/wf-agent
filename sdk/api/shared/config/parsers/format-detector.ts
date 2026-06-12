/**
 * Format Detector
 *
 * A pure utility to detect the config format (TOML / JSON) from a file path.
 * No file I/O — only string/path logic.
 */

import { ConfigFormat } from "../types.js";

/**
 * Detect configuration format from file extension.
 *
 * @param filePath - File path (or file name) with a recognised extension.
 * @returns The detected ConfigFormat.
 * @throws Error if the extension is not recognised.
 */
export function getConfigFormatFromPath(filePath: string): ConfigFormat {
  // Use lastIndexOf so we don't need the `path` module (pure string operation).
  const dotIdx = filePath.lastIndexOf(".");
  if (dotIdx === -1) {
    throw new Error(`Configuration file has no extension: ${filePath}`);
  }
  const ext = filePath.slice(dotIdx).toLowerCase();

  switch (ext) {
    case ".toml":
      return "toml";
    case ".json":
      return "json";
    default:
      throw new Error(`Unrecognised configuration file extension: ${ext}`);
  }
}
