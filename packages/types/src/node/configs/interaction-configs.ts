/**
 * User Interaction Node Configuration Type Definition
 * 
 * Workflow-specific configuration for USER_INTERACTION nodes.
 * This is NOT part of the general interaction protocol - it's workflow state management.
 */

/**
 * Variable Update Configuration (Workflow-specific)
 * Used to update workflow variables based on user input
 */
export interface WorkflowVariableUpdateConfig {
  /** Variable name */
  variableName: string;
  /** Variable update expression (may contain {{input}} placeholders) */
  expression: string;
}

/**
 * Message Configuration (Workflow-specific)
 * Used to add user messages to workflow conversation context
 */
export interface WorkflowMessageConfig {
  /** Message role (fixed to 'user') */
  role: 'user';
  /** Message content template (may contain {{input}} placeholders) */
  contentTemplate: string;
}

/**
 * User Interaction Node Output
 * - operationType: 'UPDATE_VARIABLES' | 'ADD_MESSAGE' - The type of interaction performed
 * - userInput: unknown - The raw input received from the user
 * - updatedVariables?: Array<{ variableName: string, newValue: unknown }> - Variables updated (when UPDATE_VARIABLES)
 * - addedMessages?: number - Number of messages added to context (when ADD_MESSAGE)
 */
export interface UserInteractionNodeOutput {
  operationType: 'UPDATE_VARIABLES' | 'ADD_MESSAGE';
  userInput: unknown;
  updatedVariables?: Array<{
    variableName: string;
    newValue: unknown;
  }>;
  addedMessages?: number;
}

/**
 * User Interaction Node Configuration
 * 
 * Defines workflow-level operations that require user input.
 * Currently supports:
 * - UPDATE_VARIABLES: Update workflow variables based on user input
 * - ADD_MESSAGE: Add user message to conversation context
 * 
 * Note: This is different from the general UserInteractionRequest protocol.
 * The general protocol (in packages/types/src/interaction/) is for app-level
 * UI interactions like tool approval and follow-up questions.
 */
export interface UserInteractionNodeConfig {
  /** Type of operation */
  operationType: 'UPDATE_VARIABLES' | 'ADD_MESSAGE';
  
  /** Variable update configuration (when operationType = UPDATE_VARIABLES) */
  variables?: WorkflowVariableUpdateConfig[];
  
  /** Message configuration (when operationType = ADD_MESSAGE) */
  message?: WorkflowMessageConfig;
  
  /** Prompt message to the user (used by the application layer for display) */
  prompt: string;
  
  /** Interaction timeout (milliseconds) */
  timeout?: number;
  
  /** Additional operational information */
  metadata?: Record<string, unknown>;
}
