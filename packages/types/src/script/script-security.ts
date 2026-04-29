/**
 * Code Security Related Type Definitions
 * Define a type system for code execution risk management
 */

/**
 * Code Risk Level
 */
export type ScriptRiskLevel =
  /** No risk - no security checks */
  | "none"
  /** Low Risk - Basic Pathway Check */
  | "low"
  /** Medium Risk - Hazardous Command Check */
  | "medium"
  /** High Risk - Warning logs should be recorded, additional security measures such as sandboxing should be implemented at the application layer */
  | "high";

/**
 * Verification results
 */
export interface ValidationResult {
  /** Whether or not it passes the validation */
  valid: boolean;
  /** Error message (if validation fails) */
  message?: string;
  /** Additional information */
  metadata?: Record<string, unknown>;
}

/**
 * Results of security inspections
 */
export interface SecurityCheckResult {
  /** Is it safe? */
  secure: boolean;
  /** List of Violations */
  violations: SecurityViolation[];
  /** suggestion */
  recommendations?: string[];
}

/**
 * security violation
 */
export interface SecurityViolation {
  /** Type of violation */
  type: "risk_level" | "forbidden_command" | "forbidden_path" | "size_exceeded" | "blacklisted";
  /** error message */
  message: string;
  /** severity */
  severity: "error" | "warning";
  /** Detailed information */
  details?: Record<string, unknown>;
}

/**
 * Code Security Policy
 */
export interface ScriptSecurityPolicy {
  /** Permissible risk levels */
  allowedRiskLevels: ScriptRiskLevel[];
  /** Script Whitelisting */
  whitelist?: string[];
  /** Script Blacklist */
  blacklist?: string[];
  /** prohibited command */
  forbiddenCommands?: string[];
  /** Prohibited Path Patterns */
  forbiddenPathPatterns?: RegExp[];
  /** Maximum script size (bytes) */
  maxScriptSize?: number;
  /** Whether to allow dynamic scripting */
  allowDynamicScripts?: boolean;
}

/**
 * Audit events
 */
export interface AuditEvent {
  /** Event Type */
  eventType: string;
  /** timestamp */
  timestamp: Date;
  /** Execution ID */
  executionId: string;
  /** Node ID */
  nodeId: string;
  /** Node Name */
  nodeName?: string;
  /** user ID */
  userId?: string;
  /** screenplay title */
  scriptName?: string;
  /** risk level */
  riskLevel?: ScriptRiskLevel;
  /** Additional data */
  data?: Record<string, unknown>;
}
