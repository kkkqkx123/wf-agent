/**
 * Execution API - Simplified workflow execution
 */

import { ExecutionAPIImpl } from '../executors/execution.executor.js';
import type { ExecutionBuilder } from '../types/execution.types.js';

/**
 * Execution API interface
 */
export interface ExecutionAPI {
  workflow(id: string): ExecutionBuilder;
}

// Re-export ExecutionAPIImpl as the implementation
export { ExecutionAPIImpl };

// Re-export types
export type { ExecutionBuilder };

