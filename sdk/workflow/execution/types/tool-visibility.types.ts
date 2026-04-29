/**
 * Tool Visibility Type Definitions
 * Define the context for tool visibility and the related data structures
 */

import type { ToolScope } from "../../stores/tool-context-store.js";

/**
 * Visibility declaration history
 */
export interface VisibilityDeclaration {
  /** timestamp */
  timestamp: number;
  /** Scope Types */
  scope: ToolScope;
  /** Scope ID (Execution ID/Workflow ID) */
  scopeId: string;
  /** List of tool IDs */
  toolIds: string[];
  /** Declare the position of the message within the conversation. */
  messageIndex: number;
  /** Change type */
  changeType: "init" | "enter_scope" | "add_tools" | "exit_scope" | "refresh";
}

/**
 * Tool Visibility Context
 */
export interface ToolVisibilityContext {
  /** Current scope */
  currentScope: ToolScope;

  /** Current scope ID (execution ID/workflow ID) */
  scopeId: string;

  /** The currently visible set of tools */
  visibleTools: Set<string>;

  /** Visibility declaration history */
  declarationHistory: VisibilityDeclaration[];

  /** The message index of the last declaration */
  lastDeclarationIndex: number;

  /** Initialize timestamp */
  initializedAt: number;
}

/**
 * Tool Visibility Change Types
 */
export type VisibilityChangeType =
  | "init" // Initialization
  | "enter_scope" // Enter scope
  | "add_tools" // Add tools
  | "exit_scope" // Leave the scope
  | "refresh"; // Refresh statement
