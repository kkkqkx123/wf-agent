/**
 * Tool Risk Level Type Definitions
 * Used for auto-approval decision making
 */

/**
 * Tool operation risk level
 * Used to categorize tools for auto-approval decisions
 */
export type ToolRiskLevel =
  /** Read operations, no side effects - safe to auto-approve by default */
  | "READ_ONLY"
  /** Write operations, has side effects - requires explicit configuration */
  | "WRITE"
  /** Command execution, high risk - requires command whitelist */
  | "EXECUTE"
  /** MCP protocol operations - independent category with fine-grained control */
  | "MCP"
  /** HTTP network requests - requires domain whitelist */
  | "NETWORK"
  /** System-level operations - never auto-approve */
  | "SYSTEM"
  /** User interaction - special handling (timeout auto-response) */
  | "INTERACTION";

/**
 * Auto-approval category
 * Maps to risk levels for configuration
 */
export type AutoApprovalCategory =
  | "alwaysAllowReadOnly"
  | "alwaysAllowWrite"
  | "alwaysAllowExecute"
  | "alwaysAllowMcp"
  | "alwaysAllowNetwork"
  | "alwaysAllowInteraction";

/**
 * Risk level to approval category mapping
 * Used to determine which category setting applies to a given risk level
 */
export const RISK_TO_CATEGORY: Record<ToolRiskLevel, AutoApprovalCategory | null> = {
  READ_ONLY: "alwaysAllowReadOnly",
  WRITE: "alwaysAllowWrite",
  EXECUTE: "alwaysAllowExecute",
  MCP: "alwaysAllowMcp",
  NETWORK: "alwaysAllowNetwork",
  INTERACTION: "alwaysAllowInteraction",
  SYSTEM: null, // SYSTEM level never auto-approves
};

/**
 * Get the approval category for a risk level
 * @param riskLevel The risk level
 * @returns The corresponding approval category, or null if never auto-approves
 */
export function getApprovalCategory(riskLevel: ToolRiskLevel): AutoApprovalCategory | null {
  return RISK_TO_CATEGORY[riskLevel];
}

/**
 * Check if a risk level can be auto-approved
 * @param riskLevel The risk level
 * @returns True if the risk level can potentially be auto-approved
 */
export function canAutoApprove(riskLevel: ToolRiskLevel): boolean {
  return RISK_TO_CATEGORY[riskLevel] !== null;
}
