/**
 * Standard Timeout Tags
 * 
 * Defines standardized tag prefixes and values for timeout management.
 * Using consistent tags enables efficient batch operations and monitoring.
 */

/**
 * Standard timeout tag prefixes
 */
export const TIMEOUT_TAG_PREFIXES = {
  /** LLM-related timeouts */
  LLM: 'llm',
  
  /** Tool execution timeouts */
  TOOL: 'tool',
  
  /** Workflow-related timeouts */
  WORKFLOW: 'workflow',
  
  /** Interruption-related timeouts */
  INTERRUPTION: 'interruption',
  
  /** User interaction timeouts */
  USER: 'user',
} as const;

/**
 * LLM-related timeout tags
 */
export const LLM_TIMEOUT_TAGS = {
  /** Single LLM call */
  CALL: 'llm-call',
  
  /** LLM streaming response */
  STREAM: 'llm-stream',
  
  /** LLM retry attempt */
  RETRY: 'llm-retry',
} as const;

/**
 * Tool execution timeout tags
 */
export const TOOL_TIMEOUT_TAGS = {
  /** Generic tool execution */
  EXECUTION: 'tool-execution',
  
  /** Shell command execution */
  SHELL: 'tool-shell',
  
  /** API call tool */
  API: 'tool-api',
} as const;

/**
 * Workflow-related timeout tags
 */
export const WORKFLOW_TIMEOUT_TAGS = {
  /** Overall workflow execution */
  EXECUTION: 'workflow-execution',
  
  /** Pause state monitoring */
  PAUSE: 'workflow-pause',
  
  /** Node execution */
  NODE: 'workflow-node',
} as const;

/**
 * Interruption-related timeout tags
 */
export const INTERRUPTION_TIMEOUT_TAGS = {
  /** Interruption hook execution */
  HOOK: 'interruption-hook',
  
  /** Interruption cleanup operations */
  CLEANUP: 'interruption-cleanup',
} as const;

/**
 * User interaction timeout tags
 */
export const USER_TIMEOUT_TAGS = {
  /** Waiting for user input */
  INPUT: 'user-input',
  
  /** Waiting for user approval */
  APPROVAL: 'user-approval',
} as const;

/**
 * All standard timeout tags
 */
export const STANDARD_TIMEOUT_TAGS = {
  ...LLM_TIMEOUT_TAGS,
  ...TOOL_TIMEOUT_TAGS,
  ...WORKFLOW_TIMEOUT_TAGS,
  ...INTERRUPTION_TIMEOUT_TAGS,
  ...USER_TIMEOUT_TAGS,
} as const;

/**
 * Type representing all valid standard timeout tags
 */
export type StandardTimeoutTag = typeof STANDARD_TIMEOUT_TAGS[keyof typeof STANDARD_TIMEOUT_TAGS];

/**
 * Validate if a tag follows the standard naming convention
 * @param tag Tag to validate
 * @returns true if tag is valid
 */
export function isValidTimeoutTag(tag: string): boolean {
  // Check if it's a standard tag
  if (Object.values(STANDARD_TIMEOUT_TAGS).includes(tag as StandardTimeoutTag)) {
    return true;
  }
  
  // Check if it follows the prefix pattern (prefix-suffix)
  const parts = tag.split('-');
  if (parts.length < 2) {
    return false;
  }
  
  const prefix = parts[0];
  const validPrefixes = Object.values(TIMEOUT_TAG_PREFIXES);
  
  return validPrefixes.includes(prefix as any);
}

/**
 * Get tag category from a tag string
 * @param tag Tag string
 * @returns Tag category prefix or null if invalid
 */
export function getTagCategory(tag: string): string | null {
  const parts = tag.split('-');
  if (parts.length < 2) {
    return null;
  }
  
  const prefix = parts[0];
  if (!prefix) {
    return null;
  }
  
  const validPrefixes = Object.values(TIMEOUT_TAG_PREFIXES);
  
  return validPrefixes.includes(prefix as any) ? prefix : null;
}
