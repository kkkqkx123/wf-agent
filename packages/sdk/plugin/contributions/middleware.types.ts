/**
 * Middleware Types - Execution middleware pipeline type definitions.
 */

export type MiddlewarePhase =
  | 'before-workflow-execution'
  | 'after-workflow-execution'
  | 'before-node-execution'
  | 'after-node-execution'
  | 'before-llm-invocation'
  | 'after-llm-invocation'
  | 'before-tool-execution'
  | 'after-tool-execution'
  | 'on-error'
  | 'on-checkpoint'
  | 'on-resume';

export interface ExecutionMiddleware {
  phase: MiddlewarePhase;
  handler: (context: Record<string, unknown>, next: () => Promise<void>) => Promise<void>;
  /** Lower priority = executed earlier (default: 100) */
  priority: number;
}