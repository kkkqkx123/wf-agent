/**
 * File system utility functions
 *
 * Consolidates common file-related utilities that are duplicated across
 * multiple tool handlers, including path resolution, file size formatting,
 * and text file detection.
 */

import * as path from "path";

/**
 * Resolve a file path to an absolute path.
 *
 * - Absolute paths (starting with "/" or "X:\") are returned as-is
 * - Relative paths are resolved against the workspace directory (or cwd)
 * - Backslashes are normalized to forward slashes
 *
 * @param filePath - The file path to resolve
 * @param workspaceDir - Optional workspace directory (defaults to process.cwd())
 * @returns Resolved absolute path
 */
export function resolveFilePath(filePath: string, workspaceDir?: string): string {
  if (filePath.startsWith("/") || filePath.match(/^[A-Za-z]:\\/)) {
    return filePath;
  }
  const baseDir = workspaceDir ?? process.cwd();
  return `${baseDir}/${filePath}`.replace(/\\/g, "/");
}

/**
 * Format file size in bytes to a human-readable string.
 *
 * @param bytes - File size in bytes
 * @returns Formatted string like "1.5 MB"
 */
export function formatFileSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${Math.round((bytes / Math.pow(k, i)) * 100) / 100} ${sizes[i]}`;
}

/**
 * Common text and code file extensions set.
 */
const TEXT_EXTENSIONS = new Set([
  // Plain text
  ".txt",
  ".md",
  ".markdown",
  ".rst",
  ".text",
  // Code files
  ".js",
  ".ts",
  ".jsx",
  ".tsx",
  ".py",
  ".java",
  ".cpp",
  ".c",
  ".h",
  ".hpp",
  ".cs",
  ".go",
  ".rs",
  ".rb",
  ".php",
  ".swift",
  ".kt",
  ".scala",
  // Web files
  ".html",
  ".htm",
  ".css",
  ".scss",
  ".sass",
  ".less",
  // Config files
  ".json",
  ".yaml",
  ".yml",
  ".toml",
  ".xml",
  ".ini",
  ".cfg",
  ".conf",
  // Shell scripts
  ".sh",
  ".bash",
  ".zsh",
  ".fish",
  ".ps1",
  ".bat",
  ".cmd",
  // Data files
  ".csv",
  ".tsv",
  ".sql",
  // Other text formats
  ".log",
  ".env",
  ".gitignore",
  ".dockerignore",
  ".vue",
  ".svelte",
  ".astro",
]);

/**
 * Check if a file is likely a plain text file based on its extension.
 *
 * Files without an extension are assumed to be text files.
 *
 * @param filePath - The file path to check
 * @returns True if the file appears to be a text file
 */
export function isLikelyTextFile(filePath: string): boolean {
  const ext = path.extname(filePath).toLowerCase();

  // Files without extension are often text files (scripts, configs)
  if (!ext) {
    return true;
  }

  return TEXT_EXTENSIONS.has(ext);
}
