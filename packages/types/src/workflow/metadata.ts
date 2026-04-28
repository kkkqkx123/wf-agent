/**
 * Workflow Metadata Type Definitions
 */

/**
 * Workflow Metadata Types
 * Used to store extended information
 */
export interface WorkflowMetadata {
  /** Author Information */
  author?: string;
  /** tagged array */
  tags?: string[];
  /** categorization */
  category?: string;
}
