/**
 * Query API - Simplified execution queries
 */

import { QueryAPIImpl } from '../executors/query.executor.js';
import type { QueryBuilder } from '../types/query.types.js';

/**
 * Query API interface
 */
export interface QueryAPI {
  executions(): QueryBuilder;
}

// Re-export QueryAPIImpl as the implementation
export { QueryAPIImpl };

// Re-export types
export type { QueryBuilder };

