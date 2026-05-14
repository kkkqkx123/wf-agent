/**
 * Builtin Executor Types
 */

import type { BuiltinToolExecutionContext } from "@wf-agent/types";

/**
 * Builtin Executor Configuration
 */
export interface BuiltinExecutorConfig {
  /** Default execution context */
  defaultContext?: Partial<BuiltinToolExecutionContext>;
}
