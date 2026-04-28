/**
 * File Permission Type Definitions
 * Used for fine-grained file access control with highest priority
 */

/**
 * File permission level
 * Defines the level of access allowed for a file
 */
export type FilePermissionLevel =
  /** No restrictions - full access */
  | "none"
  /** Read-only access */
  | "read"
  /** Read and write access (no delete) */
  | "write"
  /** Full access including delete */
  | "full"
  /** Explicitly denied - no access */
  | "denied";

/**
 * File operation type
 */
export type FileOperationType = "read" | "write" | "delete";

/**
 * File permission rule
 * Defines a pattern-based permission rule
 */
export interface FilePermissionRule {
  /** File path pattern (glob pattern or exact path) */
  pattern: string;
  /** Permission level for matching files */
  permission: FilePermissionLevel;
  /** Rule description for documentation */
  description?: string;
}

/**
 * File permission settings
 * Contains all file permission rules and default behavior
 */
export interface FilePermissionSettings {
  /** Permission rules (evaluated in order, first match wins) */
  rules: FilePermissionRule[];
  /** Default permission for files not matching any rule */
  defaultPermission?: FilePermissionLevel;
}

/**
 * File permission check result
 */
export interface FilePermissionResult {
  /** Whether the operation is allowed */
  allowed: boolean;
  /** Reason for denial (if not allowed) */
  reason?: string;
  /** The rule that matched (if any) */
  matchedRule?: FilePermissionRule;
}

/**
 * Check if a permission level allows an operation
 * @param permission The permission level
 * @param operation The operation type
 * @returns True if the operation is allowed
 */
export function isOperationAllowed(
  permission: FilePermissionLevel,
  operation: FileOperationType
): boolean {
  switch (permission) {
    case "denied":
      return false;
    case "none":
      return true;
    case "read":
      return operation === "read";
    case "write":
      return operation === "read" || operation === "write";
    case "full":
      return true;
    default:
      return false;
  }
}

/**
 * Get denial reason for an operation
 * @param permission The permission level
 * @param operation The operation type
 * @param filePath The file path (for error message)
 * @returns The denial reason, or undefined if allowed
 */
export function getDenialReason(
  permission: FilePermissionLevel,
  operation: FileOperationType,
  filePath: string
): string | undefined {
  if (isOperationAllowed(permission, operation)) {
    return undefined;
  }

  switch (permission) {
    case "denied":
      return `Access to file "${filePath}" is explicitly denied`;
    case "read":
      return `File "${filePath}" is read-only, ${operation} operation not allowed`;
    case "write":
      return `File "${filePath}" does not allow delete operation`;
    default:
      return `Operation ${operation} not allowed on file "${filePath}"`;
  }
}
