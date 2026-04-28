/**
 * Interactive Node Configuration Type Definition
 */

import type { VariableScope } from '../../workflow-execution/scopes.js';

/**
 * User Interaction Node Configuration
 * Define the business semantics of user interactions without application layer implementation details
 */
export interface UserInteractionNodeConfig {
  /** Type of operation */
  operationType: 'UPDATE_VARIABLES' | 'ADD_MESSAGE';
  /** Variable update configuration (when operationType = UPDATE_VARIABLES) */
  variables?: Array<{
    /** variable name */
    variableName: string;
    /** Variable update expression (may contain {{input}} placeholders) */
    expression: string;
    /** variable scope */
    scope: VariableScope;
  }>;
  /** Extinguished placement (this operationType = ADD_MESSAGE) */
  message?: {
    /** Message role (fixed to 'user') */
    role: 'user';
    /** Message content template (may contain {{input}} placeholders) */
    contentTemplate: string;
  };
  /** Prompt message to the user (used by the application layer for display) */
  prompt: string;
  /** Interaction timeout (milliseconds) */
  timeout?: number;
  /** Additional operational information */
  metadata?: Record<string, unknown>;
}