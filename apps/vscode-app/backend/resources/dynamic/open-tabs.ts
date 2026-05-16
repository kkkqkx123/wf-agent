/**
 * Open tab data source
 * 
 * Get information about open tabs in the current VSCode
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
 * Get a list of open tabs
 * 
 * @param maxTabs Maximum number of tabs, -1 means unlimited
 * @param ignorePatterns Ignore file patterns
 * @returns List of tab paths
 */
export function getOpenTabs(maxTabs: number = -1, ignorePatterns: string[] = []): string[] {
  const tabs: string[] = [];

  for (const tabGroup of vscode.window.tabGroups.all) {
    for (const tab of tabGroup.tabs) {
      if (tab.input instanceof vscode.TabInputText) {
        const uri = tab.input.uri;
        const workspaceFolder = vscode.workspace.getWorkspaceFolder(uri);

        if (workspaceFolder) {
          const relativePath = vscode.workspace.asRelativePath(uri, false);
          if (!shouldIgnorePath(relativePath, ignorePatterns)) {
            tabs.push(relativePath);
          }
        }
      }
    }
  }

  // De-weighting and limiting the number
  const uniqueTabs = [...new Set(tabs)];
  return maxTabs === -1 ? uniqueTabs : uniqueTabs.slice(0, maxTabs);
}

/**
 * Generate the content of open tabs
 */
export function generateOpenTabsContent(tabs?: string[], maxTabs?: number): string {
  if (!tabs || tabs.length === 0) return "";

  const effectiveMaxTabs = maxTabs === undefined || maxTabs === -1 ? tabs.length : maxTabs;
  const limitedTabs = tabs.slice(0, effectiveMaxTabs);

  let result = "Currently open files in editor:\n";
  for (const tab of limitedTabs) {
    result += `  - ${tab}\n`;
  }

  if (tabs.length > limitedTabs.length) {
    result += `  ... and ${tabs.length - limitedTabs.length} more files`;
  }

  return `<open_tabs>\n${result}\n</open_tabs>`;
}
