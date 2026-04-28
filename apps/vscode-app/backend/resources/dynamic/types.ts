/**
 * VSCode 扩展的动态上下文类型定义
 *
 * 扩展 SDK 的基础类型，添加 VSCode 特有功能
 */

import type { DynamicContextConfig, DynamicRuntimeContext } from "@wf-agent/types";

/**
 * VSCode 扩展的动态上下文配置
 */
export interface VSCodeDynamicContextConfig extends DynamicContextConfig {
  /** Does it contain any open tabs? */
  includeOpenTabs?: boolean;
  /** Maximum number of tabs */
  maxOpenTabs?: number;
  /** Does it include an active editor? */
  includeActiveEditor?: boolean;
  /** Does it contain diagnostic information? */
  includeDiagnostics?: boolean;
}

/**
 * VSCode 扩展的运行时上下文
 */
export interface VSCodeDynamicRuntimeContext extends DynamicRuntimeContext {
  /** Open tab page */
  openTabs?: string[];
  /** Activity Editor */
  activeEditor?: string;
  /** Diagnostic information */
  diagnostics?: string;
}

/**
 * 诊断信息配置
 */
export interface DiagnosticsConfig {
  /** Whether to enable */
  enabled: boolean;
  /** The severity levels included are: */
  includeSeverities: ("error" | "warning" | "information" | "hint")[];
  /** Only workspace files */
  workspaceOnly: boolean;
  /** Only the files that are open are displayed. */
  openFilesOnly: boolean;
  /** Maximum number of files */
  maxFiles: number;
  /** The maximum number of diagnoses per file */
  maxDiagnosticsPerFile: number;
}
