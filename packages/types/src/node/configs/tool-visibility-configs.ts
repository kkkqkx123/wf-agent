/**
 * Tool Visibility Node Configuration Type Definition
 *
 * Manages tool permissions at runtime by blocking/unblocking tools.
 * This allows phased workflows where different tools are available at different stages.
 */

/**
 * Tool Visibility Node Output
 * - action: 'block' | 'unblock' - The action performed
 * - toolIds: string[] - The tools affected
 */
export interface ToolVisibilityNodeOutput {
  action: 'block' | 'unblock';
  toolIds: string[];
}

/**
 * Tool Visibility Node Configuration
 *
 * Manages tool permissions at runtime by blocking/unblocking tools.
 * This allows phased workflows where different tools are available at different stages.
 *
 * @example
 * ```typescript
 * // Phase 1: Disable editing during exploration
 * const config: ToolVisibilityNodeConfig = {
 *   action: 'block',
 *   toolIds: ['write_file', 'edit_file', 'delete_file'],
 *   reason: 'Complete code exploration first'
 * };
 *
 * // Phase 2: Enable editing after exploration
 * const config: ToolVisibilityNodeConfig = {
 *   action: 'unblock',
 *   toolIds: ['write_file', 'edit_file']
 * };
 * ```
 */
export interface ToolVisibilityNodeConfig {
  /** Action to perform */
  action: 'block' | 'unblock';

  /** Tool IDs to block/unblock */
  toolIds: string[];

  /** Optional reason for blocking (used in rejection message) */
  reason?: string;
}