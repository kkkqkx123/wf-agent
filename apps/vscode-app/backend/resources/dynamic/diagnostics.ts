/**
 * Diagnostic Information Data Sources
 * 
 * Get diagnostic information for the current VSCode workspace
 */

import * as vscode from "vscode";
import type { DiagnosticsConfig } from "./types.js";

/**
 * Diagnostic Severity Level Mapping
 */
const severityMap: Record<vscode.DiagnosticSeverity, string> = {
  [vscode.DiagnosticSeverity.Error]: "error",
  [vscode.DiagnosticSeverity.Warning]: "warning",
  [vscode.DiagnosticSeverity.Information]: "information",
  [vscode.DiagnosticSeverity.Hint]: "hint",
};

/**
 * Get workspace diagnostic information
 * 
 * @param config Diagnostic configuration
 * @returns Formatted diagnostic information string.
 */
export function getWorkspaceDiagnostics(config: DiagnosticsConfig): string {
  if (!config.enabled) return "";

  const allDiagnostics = vscode.languages.getDiagnostics();
  const results: string[] = [];
  let fileCount = 0;

  for (const [uri, diagnostics] of allDiagnostics) {
    if (fileCount >= config.maxFiles) break;

    // Check if it is a workspace file.
    if (config.workspaceOnly) {
      const workspaceFolder = vscode.workspace.getWorkspaceFolder(uri);
      if (!workspaceFolder) continue;
    }

    // Check if it is an open file.
    if (config.openFilesOnly) {
      const document = vscode.workspace.textDocuments.find(
        doc => doc.uri.toString() === uri.toString(),
      );
      if (!document) continue;
    }

    // Filter diagnostic information
    const filteredDiagnostics = diagnostics.filter(diag => {
      const severity = severityMap[diag.severity];
      return config.includeSeverities.includes(severity as "error" | "warning" | "information" | "hint");
    });

    if (filteredDiagnostics.length === 0) continue;

    // Limit the number of diagnostics per file.
    const limitedDiagnostics = filteredDiagnostics.slice(0, config.maxDiagnosticsPerFile);

    const relativePath = vscode.workspace.asRelativePath(uri, false);
    let fileResult = `${relativePath}:\n`;

    for (const diag of limitedDiagnostics) {
      const severity = severityMap[diag.severity];
      const line = diag.range.start.line + 1;
      const message = diag.message.replace(/\n/g, " ").trim();
      fileResult += `  [${severity}] line ${line}: ${message}\n`;
    }

    results.push(fileResult);
    fileCount++;
  }

  return results.join("\n");
}

/**
 * Generate diagnostic information content
 */
export function generateDiagnosticsContent(diagnostics?: string): string {
  if (!diagnostics) return "";
  return `<diagnostics>\n${diagnostics}\n</diagnostics>`;
}
