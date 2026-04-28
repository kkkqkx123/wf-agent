/**
 * Type Conversion Tool
 *
 * Provides data normalization and conversion functions to convert UNKNOWN or raw data to the target type
 */

import type { TodoItem, TodoStatus, TodoPriority, TodoStats } from "../todo.js";
import type { EnvironmentInfo, WorkspaceInfo } from "../environment.js";
import type { PinnedFileItem, SkillConfigItem } from "../user-config.js";
import { isTodoStatus, isValidTodoItem, isTodoPriority } from "./validators.js";

// ============================================================================
// TODO Related Conversions
// ============================================================================

/**
 * Normalizing TODO Status
 */
export function normalizeTodoStatus(
  value: unknown,
  defaultValue: TodoStatus = "pending",
): TodoStatus {
  return isTodoStatus(value) ? value : defaultValue;
}

/**
 * Normalizing TODO Priorities
 */
export function normalizeTodoPriority(
  value: unknown,
  defaultValue: TodoPriority = "medium",
): TodoPriority {
  return isTodoPriority(value) ? value : defaultValue;
}

/**
 * Normalized TODO List
 */
export function normalizeTodoList(raw: unknown): TodoItem[] {
  if (!Array.isArray(raw)) return [];
  const out: TodoItem[] = [];
  for (const item of raw) {
    if (isValidTodoItem(item)) {
      out.push({
        id: item.id.trim(),
        content: item.content,
        status: normalizeTodoStatus(item.status),
        priority: item.priority ? normalizeTodoPriority(item.priority) : undefined,
        createdAt: item.createdAt,
        updatedAt: item.updatedAt,
        metadata: item.metadata,
      });
    }
  }
  return out;
}

/**
 * Normalize individual TODO entries
 */
export function normalizeTodoItem(raw: unknown): TodoItem | null {
  if (!isValidTodoItem(raw)) return null;
  return {
    id: raw.id.trim(),
    content: raw.content,
    status: normalizeTodoStatus(raw.status),
    priority: raw.priority ? normalizeTodoPriority(raw.priority) : undefined,
    createdAt: raw.createdAt,
    updatedAt: raw.updatedAt,
    metadata: raw.metadata,
  };
}

/**
 * Calculating TODO Statistics
 */
export function calculateTodoStats(todos: TodoItem[]): TodoStats {
  const counts: Record<TodoStatus, number> = {
    pending: 0,
    in_progress: 0,
    completed: 0,
    cancelled: 0,
  };
  const priorityCounts: Record<TodoPriority, number> = {
    high: 0,
    medium: 0,
    low: 0,
  };

  for (const todo of todos) {
    counts[todo.status]++;
    if (todo.priority) {
      priorityCounts[todo.priority]++;
    }
  }

  return {
    total: todos.length,
    counts,
    priorityCounts,
  };
}

// ============================================================================
// Environmental information related conversions
// ============================================================================

/**
 * Normalized workspace information
 */
export function normalizeWorkspaceInfo(raw: unknown): WorkspaceInfo | null {
  if (!raw || typeof raw !== "object") return null;
  const obj = raw as Record<string, unknown>;

  return {
    name: typeof obj["name"] === "string" ? obj["name"] : "unknown",
    path: typeof obj["path"] === "string" ? obj["path"] : "",
    uri: typeof obj["uri"] === "string" ? obj["uri"] : undefined,
    isActive: typeof obj["isActive"] === "boolean" ? obj["isActive"] : false,
    metadata: obj["metadata"] && typeof obj["metadata"] === "object" 
      ? (obj["metadata"] as Record<string, unknown>) 
      : undefined,
  };
}

/**
 * Regularized environmental information
 */
export function normalizeEnvironmentInfo(raw: unknown): EnvironmentInfo | null {
  if (!raw || typeof raw !== "object") return null;
  const obj = raw as Record<string, unknown>;

  const workspacesRaw = obj["workspaces"];
  let workspaces: WorkspaceInfo[] = [];
  if (Array.isArray(workspacesRaw)) {
    workspaces = workspacesRaw
      .map(normalizeWorkspaceInfo)
      .filter((w): w is WorkspaceInfo => w !== null);
  }

  return {
    os: typeof obj["os"] === "string" ? obj["os"] : "unknown",
    arch: typeof obj["arch"] === "string" ? obj["arch"] : undefined,
    osVersion: typeof obj["osVersion"] === "string" ? obj["osVersion"] : undefined,
    timezone: typeof obj["timezone"] === "string" ? obj["timezone"] : "UTC",
    userLanguage: typeof obj["userLanguage"] === "string" ? obj["userLanguage"] : "en",
    locale: typeof obj["locale"] === "string" ? obj["locale"] : undefined,
    workspaces,
    timestamp: typeof obj["timestamp"] === "number" ? obj["timestamp"] : Date.now(),
    nodeVersion: typeof obj["nodeVersion"] === "string" ? obj["nodeVersion"] : undefined,
    metadata: obj["metadata"] && typeof obj["metadata"] === "object" 
      ? (obj["metadata"] as Record<string, unknown>) 
      : undefined,
  };
}

// ============================================================================
// User Configuration Related Conversions
// ============================================================================

/**
 * Regularized fixed document line
 */
export function normalizePinnedFileItem(raw: unknown): PinnedFileItem | null {
  if (!raw || typeof raw !== "object") return null;
  const obj = raw as Record<string, unknown>;

  return {
    id: typeof obj["id"] === "string" ? obj["id"] : Date.now().toString(),
    path: typeof obj["path"] === "string" ? obj["path"] : "",
    workspaceUri: typeof obj["workspaceUri"] === "string" ? obj["workspaceUri"] : undefined,
    filename: typeof obj["filename"] === "string" ? obj["filename"] : undefined,
    enabled: typeof obj["enabled"] === "boolean" ? obj["enabled"] : true,
    addedAt: typeof obj["addedAt"] === "number" ? obj["addedAt"] : Date.now(),
    size: typeof obj["size"] === "number" ? obj["size"] : undefined,
    fileType: typeof obj["fileType"] === "string" ? obj["fileType"] : undefined,
    metadata: obj["metadata"] && typeof obj["metadata"] === "object" 
      ? (obj["metadata"] as Record<string, unknown>) 
      : undefined,
  };
}

/**
 * List of normalized fixed documents
 */
export function normalizePinnedFileList(raw: unknown): PinnedFileItem[] {
  if (!Array.isArray(raw)) return [];
  return raw.map(normalizePinnedFileItem).filter((f): f is PinnedFileItem => f !== null);
}

/**
 * Normalized skill set items
 */
export function normalizeSkillConfigItem(raw: unknown): SkillConfigItem | null {
  if (!raw || typeof raw !== "object") return null;
  const obj = raw as Record<string, unknown>;

  return {
    id: typeof obj["id"] === "string" ? obj["id"] : Date.now().toString(),
    name: typeof obj["name"] === "string" ? obj["name"] : "",
    description: typeof obj["description"] === "string" ? obj["description"] : "",
    enabled: typeof obj["enabled"] === "boolean" ? obj["enabled"] : true,
    sendContent: typeof obj["sendContent"] === "boolean" ? obj["sendContent"] : true,
    path: typeof obj["path"] === "string" ? obj["path"] : undefined,
    version: typeof obj["version"] === "string" ? obj["version"] : undefined,
    allowedTools:
      Array.isArray(obj["allowedTools"]) && obj["allowedTools"].every(t => typeof t === "string")
        ? (obj["allowedTools"] as string[])
        : undefined,
    metadata: obj["metadata"] && typeof obj["metadata"] === "object" 
      ? (obj["metadata"] as Record<string, unknown>) 
      : undefined,
  };
}

/**
 * Normalized Skills Configuration List
 */
export function normalizeSkillConfigList(raw: unknown): SkillConfigItem[] {
  if (!Array.isArray(raw)) return [];
  return raw.map(normalizeSkillConfigItem).filter((s): s is SkillConfigItem => s !== null);
}

// ============================================================================
// Universal conversion tool
// ============================================================================

/**
 * Normalized strings
 */
export function normalizeString(value: unknown, defaultValue: string = ""): string {
  if (typeof value === "string") return value.trim();
  return defaultValue;
}

/**
 * Normalized figures
 */
export function normalizeNumber(value: unknown, defaultValue: number = 0): number {
  if (typeof value === "number" && !isNaN(value)) return value;
  if (typeof value === "string") {
    const parsed = parseFloat(value);
    return isNaN(parsed) ? defaultValue : parsed;
  }
  return defaultValue;
}

/**
 * Normalized Boolean
 */
export function normalizeBoolean(value: unknown, defaultValue: boolean = false): boolean {
  if (typeof value === "boolean") return value;
  if (typeof value === "string") {
    const lower = value.toLowerCase().trim();
    if (lower === "true" || lower === "1" || lower === "yes") return true;
    if (lower === "false" || lower === "0" || lower === "no") return false;
  }
  if (typeof value === "number") return value !== 0;
  return defaultValue;
}

/**
 * Normalized arrays
 */
export function normalizeArray<T>(value: unknown, defaultValue: T[] = []): T[] {
  if (Array.isArray(value)) return value as T[];
  return defaultValue;
}

/**
 * Normalized objects
 */
export function normalizeObject<T extends Record<string, unknown>>(
  value: unknown,
  defaultValue: T = {} as T,
): T {
  if (value !== null && typeof value === "object" && !Array.isArray(value)) return value as T;
  return defaultValue;
}

/**
 * Normalized timestamps
 */
export function normalizeTimestamp(value: unknown, defaultValue: number = Date.now()): number {
  if (typeof value === "number" && !isNaN(value) && value > 0) return value;
  if (typeof value === "string") {
    const parsed = Date.parse(value);
    return isNaN(parsed) ? defaultValue : parsed;
  }
  return defaultValue;
}

/**
 * truncate a string
 */
export function truncateString(text: string, maxLength: number, suffix: string = "..."): string {
  if (text.length <= maxLength) return text;
  const truncateLength = maxLength - suffix.length;
  return text.slice(0, truncateLength) + suffix;
}

/**
 * Cleaning up blank characters
 */
export function trimWhitespace(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

/**
 * Normalized Line Breaks
 */
export function normalizeLineEndings(text: string, lineEnding: "\n" | "\r\n" = "\n"): string {
  return text.replace(/\r\n|\r/g, lineEnding);
}
