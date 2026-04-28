/**
 * Built-in ignore patterns for file system operations
 *
 * These patterns represent directories that are typically large or contain
 * generated/dependency files that should be excluded from file listings
 * and searches for performance and security reasons.
 */

/**
 * List of directories that should be ignored when listing files recursively
 * or scanning for code indexing.
 */
export const BUILTIN_IGNORE_DIRS = [
  // OS
  ".DS_Store",
  ".DS_Store",
  ".DS_Store?",
  "*.swp",

  // Dependencies
  "node_modules",
  "__pycache__",
  "env",
  "venv",
  ".venv",
  "vendor",
  "deps",
  "Pods",

  // Build outputs
  "target/dependency",
  "build/dependencies",
  "dist",
  "out",
  "bundle",
  "target",
  ".next",
  ".nuxt",

  // Temporary files
  "tmp",
  "temp",
  ".tmp",
  ".temp",

  // Version control
  ".git",
  ".svn",
  ".hg",

  // IDE/Editor
  ".idea",
  ".vscode",
  ".zed",

  // Hidden directories (pattern)
  ".*",

  // Repomix
  ".repomix-output.xml",

  // env
  ".env",

  // 
] as const;

/**
 * Critical directories that should always be ignored,
 * even inside explicitly targeted hidden directories
 */
export const CRITICAL_IGNORE_DIRS = new Set([
  "node_modules",
  ".git",
  "__pycache__",
  "venv",
  "env",
  ".venv",
]);

/**
 * Maximum number of results to return from file listing operations
 */
export const MAX_FILE_RESULTS = 1000;

/**
 * Maximum line length for search results
 */
export const MAX_LINE_LENGTH = 500;
