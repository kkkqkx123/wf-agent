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

// ============================================================================
// Cross-Boundary Protocol Handling
// ============================================================================

/**
 * Strategy for handling protocol mismatches across execution boundaries.
 *
 * Determines how the system behaves when a parent execution and child execution
 * (sub-agent, workflow fork, triggered sub-workflow) use different tool call protocols.
 */
export type CrossBoundaryMismatchStrategy =
  /**
   * Automatically convert messages between protocols (default).
   * The parent's message history is converted to the child's protocol format.
   */
  | "convert"
  /**
   * Force child to inherit parent's protocol ignoring child's own config.
   * The child effectively uses the same protocol as the parent.
   */
  | "inherit"
  /**
   * Reject if protocols differ (throw error).
   * Ensures all executions in a hierarchy use the same protocol.
   */
  | "strict"
  /**
   * Log a warning, then accept the mismatch.
   * Only safe when both protocols are text-based and compatible.
   */
  | "warn_and_continue";

/**
 * Cross-boundary protocol configuration.
 *
 * Controls how protocol mismatches are handled when execution crosses
 * boundaries between different execution contexts (e.g., sub-agent spawn,
 * workflow fork, triggered sub-workflow).
 */
export interface CrossBoundaryConfig {
  /**
   * How to handle protocol mismatches across execution boundaries.
   * @default "convert"
   */
  mismatchStrategy: CrossBoundaryMismatchStrategy;

  /**
   * Whether to lock the child's protocol at creation time.
   * When true, the child execution's protocol is locked immediately
   * (before any LLM calls), preventing mid-execution changes.
   * @default true
   */
  lockChildProtocol: boolean;
}

/**
 * Default cross-boundary configuration
 */
export const DEFAULT_CROSS_BOUNDARY_CONFIG: CrossBoundaryConfig = {
  mismatchStrategy: "convert",
  lockChildProtocol: true,
};