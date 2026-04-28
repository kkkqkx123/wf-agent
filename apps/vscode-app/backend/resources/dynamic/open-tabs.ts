/**
 * 打开的标签页数据源
 *
 * 获取当前 VSCode 中打开的标签页信息
 */

import * as vscode from "vscode";

/**
 * 检查路径是否应该被忽略
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
 * 获取打开的标签页列表
 *
 * @param maxTabs 最大标签页数量，-1 表示不限制
 * @param ignorePatterns 忽略的文件模式
 * @returns 标签页路径列表
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
 * 生成打开的标签页内容
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
