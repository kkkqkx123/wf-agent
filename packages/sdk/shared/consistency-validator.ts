/**
 * Consistency Validator Framework
 *
 * Provides mechanisms to validate execution state integrity across modules.
 * Ensures data consistency between related components (messages, state, tools, etc).
 */

import { createHash } from "crypto";
import { createContextualLogger } from "../utils/contextual-logger.js";
import type { Tool } from "@wf-agent/types";

const logger = createContextualLogger({ component: "ConsistencyValidator" });

/**
 * Consistency error details
 */
export interface ConsistencyError {
  /** Error type identifier */
  type: string;
  /** Error message */
  message: string;
  /** Optional details */
  details?: Record<string, unknown>;
}

/**
 * Validation result
 */
export interface ValidationResult {
  /** Whether validation passed */
  valid: boolean;
  /** List of errors found */
  errors: ConsistencyError[];
  /** List of warnings (non-fatal issues) */
  warnings: string[];
  /** Validation timestamp */
  timestamp: number;
}

/**
 * Validator interface
 */
export interface Validator {
  validate(entity: unknown): Promise<ValidationResult>;
}

/**
 * Base consistency validator
 */
export abstract class BaseConsistencyValidator implements Validator {
  abstract validate(entity: unknown): Promise<ValidationResult>;

  protected createError(type: string, message: string, details?: Record<string, unknown>): ConsistencyError {
    return { type, message, details };
  }

  protected createResult(
    errors: ConsistencyError[],
    warnings: string[] = [],
  ): ValidationResult {
    return {
      valid: errors.length === 0,
      errors,
      warnings,
      timestamp: Date.now(),
    };
  }
}

/**
 * Message-State Consistency Validator
 *
 * Validates that:
 * - Message count matches expected count from state
 * - Message history fingerprint matches checkpoint metadata
 * - No orphaned messages or unmatched state entries
 */
export class MessageStateConsistencyValidator extends BaseConsistencyValidator {
  async validate(execution: unknown): Promise<ValidationResult> {
    const errors: ConsistencyError[] = [];
    const warnings: string[] = [];

    try {
      const ex = execution as {
        getConversationManager(): { getMessages(): { role: string; toolCallId?: string; toolCalls?: { id: string }[] }[] };
        state: {
          messageCount?: number;
          messageMetadata?: { fingerprint?: string };
        };
      } | null;

      // Check if execution has required properties
      if (!ex?.getConversationManager || !ex?.state) {
        return this.createResult([
          this.createError(
            "MISSING_COMPONENTS",
            "Execution missing ConversationManager or state",
          ),
        ]);
      }

      const conversationManager = ex.getConversationManager();
      const state = ex.state;
      const messages = conversationManager.getMessages();

      // Validate message count consistency
      if (state.messageCount !== undefined) {
        const expectedCount = state.messageCount;
        const actualCount = messages.length;

        if (Math.abs(actualCount - expectedCount) > 2) {
          errors.push(
            this.createError(
              "MESSAGE_COUNT_MISMATCH",
              `Message count mismatch: expected ${expectedCount}, got ${actualCount}`,
              {
                expected: expectedCount,
                actual: actualCount,
              },
            ),
          );
        }
      }

      // Validate message fingerprint if available
      if (state.messageMetadata?.fingerprint) {
        const currentFingerprint = this.calculateFingerprint(messages);
        if (currentFingerprint !== state.messageMetadata.fingerprint) {
          errors.push(
            this.createError(
              "MESSAGE_FINGERPRINT_MISMATCH",
              "Message fingerprint does not match checkpoint metadata",
              {
                expected: state.messageMetadata.fingerprint,
                actual: currentFingerprint,
              },
            ),
          );
        }
      }

      // Check for tool call response completeness
      const respondedToolCallIds = new Set<string>();
      for (const msg of messages) {
        if (msg.role === "tool" && msg.toolCallId) {
          respondedToolCallIds.add(msg.toolCallId);
        }
      }

      for (const msg of messages) {
        if (msg.role === "assistant" && msg.toolCalls) {
          for (const call of msg.toolCalls) {
            if (!respondedToolCallIds.has(call.id)) {
              warnings.push(
                `Tool call ${call.id} in message at index ${messages.indexOf(msg)} was not responded to`,
              );
            }
          }
        }
      }

      return this.createResult(errors, warnings);
    } catch (error) {
      logger.error("Message-state consistency validation failed", {
        error: error instanceof Error ? error.message : String(error),
      });

      return this.createResult([
        this.createError(
          "VALIDATION_ERROR",
          `Validation failed: ${error instanceof Error ? error.message : String(error)}`,
        ),
      ]);
    }
  }

  private calculateFingerprint(messages: unknown[]): string {
    const messageString = JSON.stringify(messages);
    return createHash("sha256").update(messageString).digest("hex");
  }
}

/**
 * Tool Registry Consistency Validator
 *
 * Validates that:
 * - Tools referenced in state still exist in registry
 * - Tool metadata is consistent with registry
 * - No broken tool references
 */
export class ToolRegistryConsistencyValidator extends BaseConsistencyValidator {
  constructor(private toolRegistry: unknown) {
    super();
  }

  async validate(execution: unknown): Promise<ValidationResult> {
    const errors: ConsistencyError[] = [];
    const warnings: string[] = [];

    try {
      const ex = execution as { state?: { usedTools?: string[] } } | null;
      const registry = this.toolRegistry as { keys(): Iterable<string>; get(id: string): Tool | undefined } | null;

      if (!ex?.state) {
        return this.createResult([
          this.createError("MISSING_STATE", "Execution missing state"),
        ]);
      }

      const usedTools = ex.state.usedTools || [];
      const registryTools = new Set(registry?.keys());

      for (const toolId of usedTools) {
        if (!registryTools.has(toolId)) {
          warnings.push(
            `Tool "${toolId}" was used in execution but no longer exists in registry`,
          );
        } else {
          const tool = registry?.get(toolId) as Tool | undefined;
          if (!tool) {
            errors.push(
              this.createError(
                "TOOL_NOT_FOUND",
                `Tool "${toolId}" exists in registry but cannot be retrieved`,
              ),
            );
          }
        }
      }

      return this.createResult(errors, warnings);
    } catch (error) {
      logger.error("Tool registry consistency validation failed", {
        error: error instanceof Error ? error.message : String(error),
      });

      return this.createResult([
        this.createError(
          "VALIDATION_ERROR",
          `Validation failed: ${error instanceof Error ? error.message : String(error)}`,
        ),
      ]);
    }
  }
}

/**
 * Execution Hierarchy Consistency Validator
 *
 * Validates that:
 * - Parent-child execution relationships are consistent
 * - No circular dependencies in execution hierarchy
 * - All referenced parent/child executions exist
 */
export class ExecutionHierarchyConsistencyValidator extends BaseConsistencyValidator {
  constructor(
    private executionRegistry: unknown,
  ) {
    super();
  }

  async validate(execution: unknown): Promise<ValidationResult> {
    const errors: ConsistencyError[] = [];
    const warnings: string[] = [];

    try {
      const ex = execution as {
        id?: string;
        parentId?: string;
        childExecutionIds?: string[];
      } | null;
      const registry = this.executionRegistry as { get?(id: string): { parentId?: string } | undefined } | null;

      if (!ex?.id) {
        return this.createResult([
          this.createError("MISSING_ID", "Execution missing ID"),
        ]);
      }

      // Check parent execution
      if (ex.parentId) {
        const parent = registry?.get?.(ex.parentId);
        if (!parent) {
          warnings.push(
            `Parent execution "${ex.parentId}" referenced but not found`,
          );
        }
      }

      // Check child executions
      const children = ex.childExecutionIds || [];
      for (const childId of children) {
        const child = registry?.get?.(childId);
        if (!child) {
          warnings.push(
            `Child execution "${childId}" referenced but not found`,
          );
        }
      }

      // Check for circular dependencies (basic check)
      if (this.hasCircularDependency(ex.parentId)) {
        errors.push(
          this.createError(
            "CIRCULAR_DEPENDENCY",
            "Circular dependency detected in execution hierarchy",
          ),
        );
      }

      return this.createResult(errors, warnings);
    } catch (error) {
      logger.error("Execution hierarchy consistency validation failed", {
        error: error instanceof Error ? error.message : String(error),
      });

      return this.createResult([
        this.createError(
          "VALIDATION_ERROR",
          `Validation failed: ${error instanceof Error ? error.message : String(error)}`,
        ),
      ]);
    }
  }

  private hasCircularDependency(parentId?: string, visited = new Set<string>()): boolean {
    if (!parentId) return false;
    if (visited.has(parentId)) return true;

    visited.add(parentId);
    const registry = this.executionRegistry as { get?(id: string): { parentId?: string } | undefined } | null;
    const parent = registry?.get?.(parentId);
    if (!parent) return false;

    return this.hasCircularDependency(parent.parentId, visited);
  }
}

/**
 * Composite Validator
 *
 * Runs multiple validators and combines results
 */
export class CompositeValidator extends BaseConsistencyValidator {
  constructor(private validators: Validator[]) {
    super();
  }

  async validate(entity: unknown): Promise<ValidationResult> {
    const allErrors: ConsistencyError[] = [];
    const allWarnings: string[] = [];

    for (const validator of this.validators) {
      const result = await validator.validate(entity);
      allErrors.push(...result.errors);
      allWarnings.push(...result.warnings);
    }

    return this.createResult(allErrors, allWarnings);
  }
}

/**
 * Consistency Check Utilities
 */
export class ConsistencyChecker {
  /**
   * Create a default validator for execution entities
   */
  static createDefaultValidator(
    toolRegistry?: unknown,
    hierarchyRegistry?: unknown,
    executionRegistry?: unknown,
  ): Validator {
    const validators: Validator[] = [
      new MessageStateConsistencyValidator(),
    ];

    if (toolRegistry) {
      validators.push(new ToolRegistryConsistencyValidator(toolRegistry));
    }

    if (hierarchyRegistry && executionRegistry) {
      validators.push(
        new ExecutionHierarchyConsistencyValidator(executionRegistry),
      );
    }

    return new CompositeValidator(validators);
  }

  /**
   * Validate and throw on critical errors
   */
  static async validateOrThrow(
    entity: unknown,
    validator: Validator,
  ): Promise<ValidationResult> {
    const result = await validator.validate(entity);

    if (!result.valid) {
      const errorMessages = result.errors
        .map(e => `${e.type}: ${e.message}`)
        .join("; ");

      throw new Error(
        `Consistency validation failed: ${errorMessages}`,
      );
    }

    if (result.warnings.length > 0) {
      logger.warn("Consistency validation warnings", {
        warnings: result.warnings,
      });
    }

    return result;
  }
}
