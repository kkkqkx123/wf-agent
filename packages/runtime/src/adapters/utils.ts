/**
 * Runtime Adapter Utilities
 * Shared utility functions for resource adapters across apps.
 *
 * Provides:
 * - findByIdOrThrow: Generic SDK API get + not-found error
 * - scanDirForConfigs: Directory scanning for config (.toml/.json) files
 * - BatchResult type: Standard batch registration result
 * - batchRegisterFromDir: Generic batch register from directory
 * - executeWithErrorHandling: Generic async operation wrapper
 *
 * These utilities eliminate the duplicated patterns found in cli-app adapters
 * (workflow, tool, LLM profile, script, template, etc.) and make them
 * available to both cli-app and server.
 */

import { AdapterError } from "./base-adapter.js";

// ============================================
// Error types
// ============================================

/**
 * Resource not found error
 * Standard error for resource retrieval failures.
 */
export class NotFoundError extends AdapterError {
  constructor(
    resourceName: string,
    id: string,
    message?: string,
  ) {
    super(
      "NOT_FOUND",
      message ?? `${resourceName} not found: ${id}`,
    );
    this.name = "NotFoundError";
  }
}

// ============================================
// Batch result type
// ============================================

/**
 * Standard batch registration result.
 * Used by registerFromDirectory across all adapters.
 */
export interface BatchResult<T> {
  /** Successfully registered items */
  success: T[];
  /** Failed registrations with file path and error message */
  failures: Array<{ filePath: string; error: string }>;
}

// ============================================
// Utility functions
// ============================================

/**
 * Get a resource by ID, throwing NotFoundError if not found.
 *
 * Eliminates the duplicated pattern:
 *   const entity = await api.get(id);
 *   if (!entity) throw new CLINotFoundError(`...`);
 *   return entity;
 *
 * @param api SDK API object with a get method
 * @param id Resource ID
 * @param resourceName Human-readable resource name (e.g. "Workflow", "Tool")
 * @returns The resource entity
 * @throws {NotFoundError} If the resource is not found
 */
export async function findByIdOrThrow<T>(
  api: { get: (id: string) => Promise<T | null | undefined> },
  id: string,
  resourceName: string,
): Promise<T> {
  const entity = await api.get(id);
  if (!entity) {
    throw new NotFoundError(resourceName, id);
  }
  return entity;
}

/**
 * Scan a directory for configuration files (.toml, .json).
 *
 * Eliminates the duplicated directory scanning logic in:
 * - WorkflowAdapter.registerFromDirectory
 * - ToolAdapter.registerFromDirectory
 * - LLMProfileAdapter.registerFromDirectory
 * - ScriptAdapter.registerFromDirectory
 * - TemplateAdapter.registerNodeTemplatesFromDirectory
 * - TemplateAdapter.registerTriggerTemplatesFromDirectory
 *
 * @param dir Directory to scan
 * @param options Scan options
 * @returns Array of matching file paths
 */
export async function scanDirForConfigs(
  dir: string,
  options?: {
    /** Whether to scan subdirectories (default: true) */
    recursive?: boolean;
    /** Optional file name pattern filter */
    filePattern?: RegExp;
  },
): Promise<string[]> {
  const { readdir } = await import("fs/promises");
  const { join, extname } = await import("path");

  const files: string[] = [];

  const scanDir = async (currentDir: string) => {
    const entries = await readdir(currentDir, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = join(currentDir, entry.name);
      if (entry.isDirectory() && options?.recursive !== false) {
        await scanDir(fullPath);
      } else if (entry.isFile()) {
        const ext = extname(entry.name).toLowerCase();
        if (ext === ".toml" || ext === ".json") {
          if (!options?.filePattern || options.filePattern.test(entry.name)) {
            files.push(fullPath);
          }
        }
      }
    }
  };

  await scanDir(dir);
  return files;
}

/**
 * Batch register resources from a directory.
 *
 * Generic version of the registerFromDirectory pattern that is duplicated
 * across multiple adapters. Each adapter provides:
 * - loadAndParse: How to load and parse a single config file
 * - register: How to register the parsed entity with the SDK
 *
 * @param options Registration options
 * @returns Batch registration result
 */
export async function batchRegisterFromDir<T>(
  options: {
    /** Directory to scan for config files */
    configDir: string;
    /** Whether to scan subdirectories (default: true) */
    recursive?: boolean;
    /** Optional file name pattern filter */
    filePattern?: RegExp;
    /** Load and parse a single config file into an entity */
    loadAndParse: (filePath: string) => Promise<T>;
    /** Register the parsed entity with the SDK */
    register: (entity: T) => Promise<void>;
    /** Called on successful registration (for logging) */
    onSuccess?: (entity: T) => void;
    /** Called on failed registration (for logging) */
    onFailure?: (filePath: string, error: Error) => void;
  },
): Promise<BatchResult<T>> {
  const files = await scanDirForConfigs(options.configDir, {
    recursive: options.recursive,
    filePattern: options.filePattern,
  });

  const success: T[] = [];
  const failures: Array<{ filePath: string; error: string }> = [];

  for (const file of files) {
    try {
      const entity = await options.loadAndParse(file);
      await options.register(entity);
      success.push(entity);
      options.onSuccess?.(entity);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      failures.push({ filePath: file, error: errorMessage });
      options.onFailure?.(file, error instanceof Error ? error : new Error(errorMessage));
    }
  }

  return { success, failures };
}

// ============================================
// Error handling wrapper
// ============================================

/**
 * Execute an async operation with error handling.
 *
 * Eliminates the duplicated pattern:
 *   return this.executeWithErrorHandling(async () => { ... }, "Operation name");
 *
 * This is a standalone function version that can be used without
 * extending BaseAppAdapter. For the class method version, see
 * BaseAppAdapter.executeWithErrorHandling.
 *
 * @param operation Async operation to execute
 * @param onError Error handler callback
 * @returns Result of the operation
 */
export async function executeWithErrorHandling<T>(
  operation: () => Promise<T>,
  onError: (error: unknown) => never,
): Promise<T> {
  try {
    return await operation();
  } catch (error) {
    return onError(error);
  }
}

/**
 * Create a typed error handler for a specific adapter.
 * Returns a function that formats errors consistently.
 */
export function createErrorHandler(
  resourceName: string,
  logger?: { errorLog: (message: string, context?: Record<string, unknown>) => void },
) {
  return (error: unknown, context: string): never => {
    const message = error instanceof Error ? error.message : String(error);
    logger?.errorLog(`${resourceName} adapter error in ${context}: ${message}`);
    throw new AdapterError(
      "ADAPTER_ERROR",
      `Failed to ${context} on ${resourceName}`,
      error,
    );
  };
}