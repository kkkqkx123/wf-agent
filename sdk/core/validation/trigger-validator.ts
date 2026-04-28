/**
 * Trigger Validation Function
 * Provides static validation logic for Trigger configurations, using Zod schemas from types package.
 */

import type {
  WorkflowTrigger,
  TriggerCondition,
  TriggerAction,
  ExecuteTriggeredSubgraphActionConfig,
  ExecuteScriptActionConfig,
  TriggerReference,
} from "@wf-agent/types";
import {
  TriggerConditionSchema,
  TriggerActionSchema,
  WorkflowTriggerSchema,
  TriggerReferenceSchema,
  ExecuteTriggeredSubgraphActionConfigSchema,
  ExecuteScriptActionConfigSchema,
} from "@wf-agent/types";
import { ConfigurationValidationError } from "@wf-agent/types";
import type { Result } from "@wf-agent/types";
import { ok, err } from "@wf-agent/common-utils";
import { validateConfig } from "./utils.js";

/**
 * Verify the trigger condition
 * @param condition: The trigger condition
 * @param path: The field path (used for error paths)
 * @throws ValidationError: Throws a ValidationError when the configuration is invalid
 */
export function validateTriggerCondition(
  condition: TriggerCondition,
  path: string = "condition",
): Result<TriggerCondition, ConfigurationValidationError[]> {
  return validateConfig(condition, TriggerConditionSchema, path, "trigger");
}

/**
 * Verify the trigger workflow action configuration
 * @param config: Trigger workflow action configuration
 * @param path: Field path (used for error paths)
 * @throws ValidationError: Throws a ValidationError when the configuration is invalid
 */
export function validateExecuteTriggeredSubgraphActionConfig(
  config: ExecuteTriggeredSubgraphActionConfig,
  path: string = "action.parameters",
): Result<ExecuteTriggeredSubgraphActionConfig, ConfigurationValidationError[]> {
  return validateConfig(config, ExecuteTriggeredSubgraphActionConfigSchema, path, "trigger");
}

/**
 * Verify the execution script action configuration
 * @param config: Execution script action configuration
 * @param path: Field path (used for error paths)
 * @throws ValidationError: Throws a ValidationError when the configuration is invalid
 */
export function validateExecuteScriptActionConfig(
  config: ExecuteScriptActionConfig,
  path: string = "action.parameters",
): Result<ExecuteScriptActionConfig, ConfigurationValidationError[]> {
  return validateConfig(config, ExecuteScriptActionConfigSchema, path, "trigger");
}

/**
 * Verify the trigger action
 * @param action The trigger action
 * @param path The field path (used for error paths)
 * @throws ValidationError Throws a ValidationError when the configuration is invalid
 */
export function validateTriggerAction(
  action: TriggerAction,
  path: string = "action",
): Result<TriggerAction, ConfigurationValidationError[]> {
  // Using the `TriggerActionSchema` with `discriminatedUnion` will automatically validate each type of parameter.
  return validateConfig(action, TriggerActionSchema, path, "trigger") as Result<
    TriggerAction,
    ConfigurationValidationError[]
  >;
}

/**
 * Verify WorkflowTrigger
 * @param trigger WorkflowTrigger object
 * @param path field path (used for error paths)
 * @throws ValidationError Throws a ValidationError when the configuration is invalid
 */
export function validateWorkflowTrigger(
  trigger: WorkflowTrigger,
  path: string = "triggers",
): Result<WorkflowTrigger, ConfigurationValidationError[]> {
  return validateConfig(trigger, WorkflowTriggerSchema, path, "trigger") as Result<
    WorkflowTrigger,
    ConfigurationValidationError[]
  >;
}

/**
 * Verify TriggerReference
 * @param reference: TriggerReference object
 * @param path: field path (used for error paths)
 * @throws ValidationError: Throws a ValidationError when the configuration is invalid
 */
export function validateTriggerReference(
  reference: TriggerReference,
  path: string = "triggers",
): Result<TriggerReference, ConfigurationValidationError[]> {
  return validateConfig(reference, TriggerReferenceSchema, path, "trigger") as Result<
    TriggerReference,
    ConfigurationValidationError[]
  >;
}

/**
 * 验证触发器数组（包含 WorkflowTrigger 和 TriggerReference）
 * @param triggers 触发器数组
 * @param path 字段路径（用于错误路径）
 * @returns Result<(WorkflowTrigger | TriggerReference)[], ConfigurationValidationError[]>
 */
export function validateTriggers(
  triggers: (WorkflowTrigger | TriggerReference)[],
  path: string = "triggers",
): Result<(WorkflowTrigger | TriggerReference)[], ConfigurationValidationError[]> {
  if (!triggers || !Array.isArray(triggers)) {
    return err([
      new ConfigurationValidationError("Triggers must be an array", {
        configType: "trigger",
        configPath: path,
      }),
    ]);
  }

  // Check the uniqueness of the trigger ID.
  const triggerIds = new Set<string>();
  const errors: ConfigurationValidationError[] = [];
  for (let i = 0; i < triggers.length; i++) {
    const trigger = triggers[i];
    if (!trigger) continue;

    const itemPath = `${path}[${i}]`;

    // Check the uniqueness of the ID.
    const triggerId = "id" in trigger ? trigger.id : trigger.triggerId;
    if (triggerIds.has(triggerId)) {
      errors.push(
        new ConfigurationValidationError(`Trigger ID must be unique: ${triggerId}`, {
          configType: "trigger",
          configPath: `${itemPath}.id`,
        }),
      );
      continue;
    }
    triggerIds.add(triggerId);

    // Type validation based on the type
    let result: Result<WorkflowTrigger | TriggerReference, ConfigurationValidationError[]>;
    if ("templateName" in trigger) {
      // TriggerReference
      result = validateTriggerReference(trigger, itemPath);
    } else {
      // WorkflowTrigger
      result = validateWorkflowTrigger(trigger, itemPath);
    }

    if (result.isErr()) {
      errors.push(...result.error);
    }
  }

  if (errors.length === 0) {
    return ok(triggers);
  }
  return err(errors);
}
