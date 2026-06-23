/**
 * Tool utility functions
 * Only contains utility functions specific to the app layer
 */

/**
 * Resolve path (supports relative paths)
 * File path resolution logic specific to the app layer
 */
export function resolvePath(path: string, workspaceDir: string): string {
  if (path.startsWith("/") || path.match(/^[A-Za-z]:\\/)) {
    return path;
  }
  return `${workspaceDir}/${path}`.replace(/\\/g, "/");
}
