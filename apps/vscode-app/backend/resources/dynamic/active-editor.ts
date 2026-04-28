/**
 * 活动编辑器数据源
 *
 * 获取当前 VSCode 中活动的编辑器信息
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
 * 获取活动编辑器路径
 *
 * @param ignorePatterns 忽略的文件模式
 * @returns 相对路径或 null
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
 * 生成活动编辑器内容
 */
export function generateActiveEditorContent(activeEditor?: string): string {
  if (!activeEditor) return "";
  return `<active_editor>\nCurrently active file: ${activeEditor}\n</active_editor>`;
}
