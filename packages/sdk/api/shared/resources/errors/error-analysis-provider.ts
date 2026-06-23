/**
 * Error Analysis Provider Interface
 *
 * Defines the contract for analyzing errors from different execution types.
 * Implementations handle the specific logic for accessing error records
 * from their respective execution entities (Agent, Workflow, etc).
 *
 * This enables a pluggable architecture where new execution types
 * can be supported without modifying existing code.
 */

import type { ID, ExecutionErrorRecord } from "@wf-agent/types";

/**
 * Error Analysis Provider Interface
 *
 * Defines the contract for error analysis operations across different execution types.
 * Each implementation is responsible for:
 * - Accessing the appropriate registry for its execution type
 * - Retrieving error records from the execution entity
 * - Handling errors gracefully with logging
 */
export interface IErrorAnalysisProvider {
  /**
   * Get all error records for an execution
   *
   * @param executionId The ID of the execution to analyze
   * @returns Promise resolving to array of error records
   *          Returns empty array if:
   *          - Execution not found
   *          - No errors occurred
   *          - Error tracking not yet implemented for this type
   */
  getExecutionErrorRecords(executionId: ID): Promise<ExecutionErrorRecord[]>;
}

/**
 * Generic Error Analysis Result
 *
 * Contains information about the analyzed execution and its errors
 */
export interface ErrorAnalysisProviderResult {
  /** The execution ID that was analyzed */
  executionId: ID;

  /** Type of execution (agent, workflow, etc) */
  executionType: "agent" | "workflow";

  /** Error records found during analysis */
  errors: ExecutionErrorRecord[];

  /** Time taken to perform analysis in milliseconds */
  analysisTime: number;
}

/**
 * Execution Type Discriminant
 *
 * Used to identify which provider implementation to use
 */
export type ExecutionTypeDiscriminant = "agent" | "workflow";
