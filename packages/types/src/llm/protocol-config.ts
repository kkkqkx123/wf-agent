/**
 * Tool Call Protocol Configuration Types
 *
 * Defines the policy for handling protocol violations and the global
 * configuration for tool call protocol locking and cross-boundary conversion.
 */

/**
 * Policy for handling tool call protocol violations.
 *
 * A protocol violation occurs when the locked tool call format (set at execution start)
 * differs from the format requested by the current LLM profile or request.
 */
export type ToolCallProtocolViolationPolicy =
  /** Silently use the locked protocol (no warning) */
  | "ignore"
  /** Log a warning, then use the locked protocol */
  | "warn"
  /** Throw an error, interrupting the execution */
  | "fail"
  /** Convert history between formats automatically */
  | "auto_convert";

/**
 * Global tool call protocol configuration.
 *
 * Controls how protocol locking, violation handling, and cross-boundary
 * conversion behave across the entire SDK.
 */
export interface ToolCallProtocolConfig {
  /** Global default policy for protocol violation (default: "warn") */
  violationPolicy: ToolCallProtocolViolationPolicy;

  /** Whether to lock protocol at execution start (default: true) */
  lockProtocol: boolean;

  /** Whether to enable cross-boundary protocol conversion (default: true) */
  enableCrossBoundaryConversion: boolean;
}

/**
 * Default tool call protocol configuration
 */
export const DEFAULT_TOOL_CALL_PROTOCOL_CONFIG: ToolCallProtocolConfig = {
  violationPolicy: "warn",
  lockProtocol: true,
  enableCrossBoundaryConversion: true,
};