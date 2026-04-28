/**
 * File Permission Checker
 * Implements file-level permission control with highest priority
 */

import type {
  FilePermissionSettings,
  FilePermissionRule,
  FilePermissionLevel,
  FileOperationType,
  FilePermissionResult,
} from "@wf-agent/types";
import { isOperationAllowed, getDenialReason } from "@wf-agent/types";
import { minimatch } from "minimatch";

/**
 * Check file permission for an operation
 *
 * @param filePath - The file path to check
 * @param operation - The operation type (read, write, delete)
 * @param settings - File permission settings
 * @returns Permission check result
 */
export function checkFilePermission(
  filePath: string,
  operation: FileOperationType,
  settings: FilePermissionSettings,
): FilePermissionResult {
  // 1. Find matching rule (first match wins)
  const matchedRule = findMatchingRule(filePath, settings.rules);

  // 2. Get permission level
  const permission = matchedRule?.permission ?? settings.defaultPermission ?? "write";

  // 3. Check if operation is allowed
  const allowed = isOperationAllowed(permission, operation);

  if (allowed) {
    return { allowed: true, matchedRule };
  }

  // 4. Get denial reason
  const reason = getDenialReason(permission, operation, filePath);

  return { allowed: false, reason, matchedRule };
}

/**
 * Find the first matching rule for a file path
 *
 * @param filePath - The file path to match
 * @param rules - List of permission rules
 * @returns The first matching rule, or undefined if no match
 */
function findMatchingRule(
  filePath: string,
  rules: FilePermissionRule[],
): FilePermissionRule | undefined {
  // Normalize path separators
  const normalizedPath = filePath.replace(/\\/g, "/");

  for (const rule of rules) {
    if (minimatch(normalizedPath, rule.pattern, { nocase: true })) {
      return rule;
    }
  }

  return undefined;
}

/**
 * Check if a file path matches a pattern
 *
 * @param filePath - The file path to check
 * @param pattern - The glob pattern
 * @returns True if the path matches the pattern
 */
export function matchesPattern(filePath: string, pattern: string): boolean {
  const normalizedPath = filePath.replace(/\\/g, "/");
  return minimatch(normalizedPath, pattern, { nocase: true });
}

/**
 * Get the effective permission level for a file
 *
 * @param filePath - The file path
 * @param settings - File permission settings
 * @returns The effective permission level
 */
export function getEffectivePermission(
  filePath: string,
  settings: FilePermissionSettings,
): FilePermissionLevel {
  const matchedRule = findMatchingRule(filePath, settings.rules);
  return matchedRule?.permission ?? settings.defaultPermission ?? "write";
}

/**
 * Batch check file permissions
 *
 * @param files - List of file paths and operations
 * @param settings - File permission settings
 * @returns Map of file paths to permission results
 */
export function batchCheckFilePermissions(
  files: Array<{ path: string; operation: FileOperationType }>,
  settings: FilePermissionSettings,
): Map<string, FilePermissionResult> {
  const results = new Map<string, FilePermissionResult>();

  for (const { path, operation } of files) {
    results.set(path, checkFilePermission(path, operation, settings));
  }

  return results;
}

/**
 * Create default file permission settings
 * Provides a sensible default configuration
 *
 * @param workspaceDir - The workspace directory
 * @returns Default file permission settings
 */
export function createDefaultFilePermissionSettings(workspaceDir?: string): FilePermissionSettings {
  const rules: FilePermissionRule[] = [
    // Deny access to sensitive files
    { pattern: "**/.env", permission: "denied", description: "Environment files" },
    { pattern: "**/.env.*", permission: "denied", description: "Environment files" },
    { pattern: "**/credentials.json", permission: "denied", description: "Credentials file" },
    { pattern: "**/secrets/**", permission: "denied", description: "Secrets directory" },
    { pattern: "**/*.pem", permission: "denied", description: "Certificate files" },
    { pattern: "**/*.key", permission: "denied", description: "Key files" },

    // Read-only for important config files
    { pattern: "**/package.json", permission: "read", description: "Package config" },
    { pattern: "**/package-lock.json", permission: "read", description: "Package lock" },
    { pattern: "**/tsconfig.json", permission: "read", description: "TypeScript config" },
    { pattern: "**/.git/**", permission: "read", description: "Git directory" },
  ];

  // If workspace is specified, allow full access to source files
  if (workspaceDir) {
    const normalizedWorkspace = workspaceDir.replace(/\\/g, "/");
    rules.push(
      { pattern: `${normalizedWorkspace}/src/**`, permission: "full", description: "Source files" },
      {
        pattern: `${normalizedWorkspace}/lib/**`,
        permission: "full",
        description: "Library files",
      },
      { pattern: `${normalizedWorkspace}/test/**`, permission: "full", description: "Test files" },
    );
  }

  return {
    rules,
    defaultPermission: "write", // Default allow read and write
  };
}
