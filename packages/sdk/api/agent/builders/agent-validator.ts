/**
 * AgentValidator - Agent Loop Definition Validator
 *
 * Validates agent loop definitions during the builder phase.
 * Follows the same pattern as WorkflowValidator for consistency across the SDK.
 * Extracted from inline validation in AgentDefinitionBuilder.build().
 */

import type { AgentLoopDefinition } from "@wf-agent/types";

/**
 * Validation input data for agent loop definitions
 */
export interface AgentValidationInput {
  /** Agent loop definition to validate */
  definition: AgentLoopDefinition;
}

/**
 * AgentValidator - validates agent loop definitions during build phase
 */
export class AgentValidator {
  private input: AgentValidationInput;

  constructor(input: AgentValidationInput) {
    this.input = input;
  }

  /**
   * Run validation against the agent loop definition.
   * Throws an Error with all validation errors concatenated.
   */
  validate(): void {
    const errors: string[] = [];

    this.validateBasic(errors);
    this.validateHooks(errors);
    this.validateTriggers(errors);
    this.validateCheckpoint(errors);

    if (errors.length > 0) {
      throw new Error(`Agent validation failed:\n${errors.join("\n")}`);
    }
  }

  /**
   * Run validation and return errors array without throwing.
   * Useful when the caller wants to collect errors without an exception.
   */
  validateSilently(): string[] {
    const errors: string[] = [];
    this.validateBasic(errors);
    this.validateHooks(errors);
    this.validateTriggers(errors);
    this.validateCheckpoint(errors);
    return errors;
  }

  private validateBasic(errors: string[]): void {
    const { definition } = this.input;

    // Name is required
    if (!definition.name || typeof definition.name !== "string" || definition.name.trim().length === 0) {
      errors.push("Agent name is required and must be a non-empty string.");
    }

    // Profile ID is recommended
    if (!definition.profileId) {
      errors.push("Agent profileId is required. An LLM profile must be specified for execution.");
    }

    // Max iterations validation
    if (definition.maxIterations !== undefined) {
      if (typeof definition.maxIterations !== "number" || definition.maxIterations < 1) {
        errors.push("maxIterations must be a positive number when specified.");
      }
      if (definition.maxIterations > 1000) {
        errors.push("maxIterations exceeds 1000. Consider if this is intentional to avoid excessive iterations.");
      }
    }

    // Check for conflicting streaming config
    if (definition.stream !== undefined && typeof definition.stream !== "boolean") {
      errors.push("stream must be a boolean value.");
    }
  }

  private validateHooks(errors: string[]): void {
    const { definition } = this.input;

    if (!definition.hooks || definition.hooks.length === 0) {
      return;
    }

    // Check for duplicate hook types (each hook type should be unique)
    const hookTypes = definition.hooks.map((h) => h.hookType);
    const uniqueHookTypes = new Set(hookTypes);
    if (uniqueHookTypes.size !== hookTypes.length) {
      errors.push("Hook types must be unique within the agent definition.");
    }

    // Validate each hook has required fields
    for (const hook of definition.hooks) {
      if (!hook.hookType || typeof hook.hookType !== "string") {
        errors.push("Each hook must have a 'hookType' specified.");
      }
      if (!hook.eventName || typeof hook.eventName !== "string") {
        errors.push("Each hook must have an 'eventName' specified.");
      }
    }
  }

  private validateTriggers(errors: string[]): void {
    const { definition } = this.input;

    if (!definition.triggers || definition.triggers.length === 0) {
      return;
    }

    // Check for duplicate trigger IDs
    const triggerIds = definition.triggers.map((t) => t.id);
    const uniqueTriggerIds = new Set(triggerIds);
    if (uniqueTriggerIds.size !== triggerIds.length) {
      errors.push("Trigger IDs must be unique within the agent definition.");
    }

    // Validate each trigger has required fields
    for (const trigger of definition.triggers) {
      if (!trigger.id || typeof trigger.id !== "string") {
        errors.push("Each trigger must have an 'id' specified.");
      }
      if (!trigger.type) {
        errors.push("Each trigger must have a 'type' specified.");
      }
      if (!trigger.action) {
        errors.push("Each trigger must have an 'action' specified.");
      }
    }
  }

  private validateCheckpoint(errors: string[]): void {
    const { definition } = this.input;

    if (!definition.checkpoint) {
      return;
    }

    // At least one checkpoint config should be enabled
    const { checkpoint } = definition;
    if (
      checkpoint.createOnEnd === undefined &&
      checkpoint.createOnError === undefined &&
      checkpoint.createOnIteration === undefined
    ) {
      errors.push("Checkpoint configuration is specified but no checkpoint strategy is enabled.");
    }
  }
}