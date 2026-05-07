/**
 * Variable Update Types
 * Types related to workflow variable updates and message configuration
 */

import type { VariableScope } from "../workflow-execution/scopes.js";

/**
 * Variable Update Configuration
 */
export interface VariableUpdateConfig {
  /** variable name */
  variableName: string;
  /** Variable update expression (may contain {{input}} placeholders) */
  expression: string;
  /** variable scope */
  scope: VariableScope;
}

/**
 * Message Configuration
 */
export interface MessageConfig {
  /** Message role (fixed to 'user') */
  role: "user";
  /** Message content template (may contain {{input}} placeholders) */
  contentTemplate: string;
}
