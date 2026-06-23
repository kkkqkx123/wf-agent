/**
 * Execution Service Module
 * 
 * This module provides the unified execution layer for workflow executions.
 * All workflow executions go through SDK. Terminals are used ONLY for display.
 */

export { ExecutionService } from "./execution-service.js";
export type { ExecutionMode, ExecutionResult } from "./execution-service.js";
