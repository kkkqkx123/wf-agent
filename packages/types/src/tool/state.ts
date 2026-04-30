/**
 * Tool Status Type Definition
 */

/**
 * Tool type
 */
export type ToolType =
  /** Stateless tools (pure functions provided by the application layer) */
  | "STATELESS"
  /** Stateful tools (classes/objects provided by the application layer, isolated via ExecutionContext) */
  | "STATEFUL"
  /** REST API Tool */
  | "REST"
  /** Built-in tools (SDK internal tools like execute_workflow) */
  | "BUILTIN";
