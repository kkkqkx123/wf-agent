/**
 * Activity Editor Data Source
 * 
 * Get information about the active editor in the current VSCode
 */

import * as vscode from "vscode";

/**
 * Check if the path should be ignored
 */
function shouldIgnorePath(path: string, patterns: string[]): boolean {
  for (const pattern of patterns) {
    // Simple glob pattern matching
    if (pattern.includes("*")) {
      const regex = new RegExp("^" + pattern.replace(/\*/g, ".*").replace(/\?/g, ".") + "$");
      if (regex.test(path)) return true;
    } else {
      if (path.includes(pattern)) return true;
    }
  }
  return false;
}

/**
 * Get event editor path
 * 
 * @param ignorePatterns file patterns to ignore
 * @returns Relative path or null
 */
export function getActiveEditorPath(ignorePatterns: string[] = []): string | null {
  const activeEditor = vscode.window.activeTextEditor;
  if (!activeEditor) return null;

  const uri = activeEditor.document.uri;
  const workspaceFolder = vscode.workspace.getWorkspaceFolder(uri);

  if (!workspaceFolder) return null;

  const relativePath = vscode.workspace.asRelativePath(uri, false);

  if (shouldIgnorePath(relativePath, ignorePatterns)) {
    return null;
  }

  return relativePath;
}

/**
 * Generating Event Editor Content
 */
export function generateActiveEditorContent(activeEditor?: string): string {
  if (!activeEditor) return "";
  return `<active_editor>\nCurrently active file: ${activeEditor}\n</active_editor>`;
}
