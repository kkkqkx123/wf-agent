/**
 * Fragment Type Definitions
 *
 * Reusing the basic type definitions from @wf-agent/prompt-templates
 * This file only defines extension types specific to the SDK layer.
 */

/**
 * Predefined system prompt word configuration
 * Used to define a complete single system prompt word
 */
export interface SystemPromptDefinition {
  /** System prompt ID */
  id: string;
  /** Name */
  name: string;
  /** Description */
  description: string;
  /** List of included fragment IDs (in order) */
  fragmentIds: string[];
  /** Optional: Additional static content (append after the snippet) */
  additionalContent?: string;
}
