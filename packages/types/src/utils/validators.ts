/**
 * Type validation tools
 *
 * Provides type guards and validation functions to ensure type safety of data
 */

import type { TodoItem, TodoStatus, TodoPriority } from "../todo.js";
import type { EnvironmentInfo, WorkspaceInfo } from "../environment.js";
import type { PinnedFileItem, SkillConfigItem } from "../user-config.js";

// ============================================================================
// TODO Related Validations
// ============================================================================

/**
 * Validating TODO Status
 */
export function isTodoStatus(value: unknown): value is TodoStatus {
  return (
    value === "pending" || value === "in_progress" || value === "completed" || value === "cancelled"
  );
}

/**
 * Validating TODO Priorities
 */
export function isTodoPriority(value: unknown): value is TodoPriority {
  return value === "high" || value === "medium" || value === "low";
}

/**
 * Validating individual TODO entries
 */
export function isValidTodoItem(item: unknown): item is TodoItem {
  if (!item || typeof item !== "object") return false;
  const obj = item as Record<string, unknown>;
  return (
    typeof obj["id"] === "string" &&
    obj["id"].trim() !== "" &&
    typeof obj["content"] === "string" &&
    isTodoStatus(obj["status"])
  );
}

/**
 * Validating an array of TODO items
 */
export function isValidTodoArray(value: unknown): value is TodoItem[] {
  if (!Array.isArray(value)) return false;
  return value.every(isValidTodoItem);
}

// ============================================================================
// Validation of environmental information
// ============================================================================

/**
 * Verify workspace information
 */
export function isValidWorkspaceInfo(item: unknown): item is WorkspaceInfo {
  if (!item || typeof item !== "object") return false;
  const obj = item as Record<string, unknown>;
  return (
    typeof obj["name"] === "string" &&
    typeof obj["path"] === "string" &&
    Array.isArray(obj["workspaces"]) === false
  );
}

/**
 * Validating environmental information
 */
export function isValidEnvironmentInfo(item: unknown): item is EnvironmentInfo {
  if (!item || typeof item !== "object") return false;
  const obj = item as Record<string, unknown>;
  return (
    typeof obj["os"] === "string" &&
    typeof obj["timezone"] === "string" &&
    typeof obj["userLanguage"] === "string" &&
    Array.isArray(obj["workspaces"]) &&
    obj["workspaces"].every(isValidWorkspaceInfo)
  );
}

// ============================================================================
// User Configuration Related Authentication
// ============================================================================

/**
 * Validating fixed file entries
 */
export function isValidPinnedFileItem(item: unknown): item is PinnedFileItem {
  if (!item || typeof item !== "object") return false;
  const obj = item as Record<string, unknown>;
  return (
    typeof obj["id"] === "string" &&
    obj["id"].trim() !== "" &&
    typeof obj["path"] === "string" &&
    typeof obj["enabled"] === "boolean" &&
    typeof obj["addedAt"] === "number"
  );
}

/**
 * Validating Skill Configuration Items
 */
export function isValidSkillConfigItem(item: unknown): item is SkillConfigItem {
  if (!item || typeof item !== "object") return false;
  const obj = item as Record<string, unknown>;
  return (
    typeof obj["id"] === "string" &&
    obj["id"].trim() !== "" &&
    typeof obj["name"] === "string" &&
    typeof obj["description"] === "string" &&
    typeof obj["enabled"] === "boolean" &&
    typeof obj["sendContent"] === "boolean"
  );
}

// ============================================================================
// Generic validation tools
// ============================================================================

/**
 * Validating String Arrays
 */
export function isStringArray(value: unknown): value is string[] {
  if (!Array.isArray(value)) return false;
  return value.every(item => typeof item === "string");
}

/**
 * Validating arrays of numbers
 */
export function isNumberArray(value: unknown): value is number[] {
  if (!Array.isArray(value)) return false;
  return value.every(item => typeof item === "number" && !isNaN(item));
}

/**
 * Array of validation objects
 */
export function isObjectArray(value: unknown): value is Record<string, unknown>[] {
  if (!Array.isArray(value)) return false;
  return value.every(item => item !== null && typeof item === "object");
}

/**
 * Validate URL string
 */
export function isValidUrl(value: unknown): value is string {
  if (typeof value !== "string") return false;
  try {
    new URL(value);
    return true;
  } catch {
    return false;
  }
}

/**
 * Verify Email Address
 */
export function isValidEmail(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
  return emailRegex.test(value);
}

/**
 * Validating file paths (simple validation)
 */
export function isValidFilePath(value: unknown): value is string {
  if (typeof value !== "string") return false;
  return value.length > 0 && value.indexOf("\0") === -1;
}

/**
 * Verify timestamp
 */
export function isValidTimestamp(value: unknown): value is number {
  return typeof value === "number" && !isNaN(value) && value > 0;
}

/**
 * Verify UUID
 */
export function isValidUuid(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const uuidRegex = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
  return uuidRegex.test(value);
}
