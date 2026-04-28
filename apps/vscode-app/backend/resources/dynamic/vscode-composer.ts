/**
 * VSCode 扩展组合器
 *
 * 扩展 SDK 的基础组合器，添加 VSCode 特有功能
 */

import { generateDynamicContextContent } from "@wf-agent/sdk";
import type {
  VSCodeDynamicContextConfig,
  VSCodeDynamicRuntimeContext,
  DiagnosticsConfig,
} from "./types.js";
import { getActiveEditorPath, generateActiveEditorContent } from "./active-editor.js";
import { getOpenTabs, generateOpenTabsContent } from "./open-tabs.js";
import { getWorkspaceDiagnostics, generateDiagnosticsContent } from "./diagnostics.js";

/**
 * 默认诊断配置
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
 * 生成 VSCode 扩展的动态上下文
 *
 * @param config VSCode 扩展的动态上下文配置
 * @param runtime 运行时上下文数据
 * @param diagnosticsConfig 诊断配置（可选）
 * @returns 动态上下文内容字符串
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
 * 检查是否有 VSCode 特有的动态内容
 */
export function hasVSCodeDynamicContent(config: VSCodeDynamicContextConfig): boolean {
  return (
    config.includeOpenTabs === true ||
    config.includeActiveEditor === true ||
    config.includeDiagnostics === true
  );
}
