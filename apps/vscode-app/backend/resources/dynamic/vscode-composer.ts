/**
 * VSCode Extended Composer
 *
 * Extends the SDK's base combiner with VSCode-specific functionality
 */

// TODO: This module needs to be refactored to use the new buildDynamicPromptInjection API
// For now, import helper functions from user-context
import type {
  VSCodeDynamicContextConfig,
  VSCodeDynamicRuntimeContext,
  DiagnosticsConfig,
} from "./types.js";
import { getActiveEditorPath, generateActiveEditorContent } from "./active-editor.js";
import { getOpenTabs, generateOpenTabsContent } from "./open-tabs.js";
import { getWorkspaceDiagnostics, generateDiagnosticsContent } from "./diagnostics.js";

// Placeholder for compatibility - compose VSCode-specific context
function generateDynamicContextContent(
  _config: VSCodeDynamicContextConfig,
  runtime?: VSCodeDynamicRuntimeContext,
): string {
  const sections: string[] = [];

  if (!runtime) {
    return "";
  }

  // This is a simplified stub that needs to be properly implemented
  // using the new two-layer dynamic prompt injection system
  sections.push("VSCode dynamic context - to be migrated to new API");

  return sections.join("\n\n");
}

/**
 * Default Diagnostic Configuration
 */
const DEFAULT_DIAGNOSTICS_CONFIG: DiagnosticsConfig = {
  enabled: true,
  includeSeverities: ["error", "warning"],
  workspaceOnly: true,
  openFilesOnly: false,
  maxFiles: 50,
  maxDiagnosticsPerFile: 10,
};

/**
 * Generating dynamic contexts for VSCode extensions
 * 
 * @param config Dynamic context configuration for VSCode extensions
 * @param runtime Runtime context data
 * @param diagnosticsConfig Diagnostics configuration (optional)
 * @returns Dynamic context content string
 */
export function generateVSCodeDynamicContext(
  config: VSCodeDynamicContextConfig,
  runtime?: VSCodeDynamicRuntimeContext,
  diagnosticsConfig: DiagnosticsConfig = DEFAULT_DIAGNOSTICS_CONFIG,
): string {
  // First generate basic dynamic content
  const baseContent = generateDynamicContextContent(config, runtime);
  const sections: string[] = [baseContent];

  // Add VSCode-specific content
  if (config.includeOpenTabs) {
    const tabs = runtime?.openTabs ?? getOpenTabs(config.maxOpenTabs);
    if (tabs.length > 0) {
      sections.push(generateOpenTabsContent(tabs, config.maxOpenTabs));
    }
  }

  if (config.includeActiveEditor) {
    const editor = runtime?.activeEditor ?? getActiveEditorPath();
    if (editor) {
      sections.push(generateActiveEditorContent(editor));
    }
  }

  if (config.includeDiagnostics) {
    const diagnostics = runtime?.diagnostics ?? getWorkspaceDiagnostics(diagnosticsConfig);
    if (diagnostics) {
      sections.push(generateDiagnosticsContent(diagnostics));
    }
  }

  return sections.join("\n\n");
}

/**
 * Checking for VSCode-specific dynamic content
 */
export function hasVSCodeDynamicContent(config: VSCodeDynamicContextConfig): boolean {
  return (
    config.includeOpenTabs === true ||
    config.includeActiveEditor === true ||
    config.includeDiagnostics === true
  );
}
