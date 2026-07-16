/**
 * Contribution Type - Type definition for the contribution type enumeration.
 */

/**
 * Recognized contribution types.
 * Only 7 types are currently implemented.
 * The following are removed until implemented:
 * 'evaluator', 'script-executor', 'resource',
 * 'prompt-template', 'fragment', 'skill-loader'
 */
export type ContributionType =
  | 'node-type'
  | 'tool-type'
  | 'llm-provider'
  | 'formatter'
  | 'hook-handler'
  | 'event-handler'
  | 'middleware';