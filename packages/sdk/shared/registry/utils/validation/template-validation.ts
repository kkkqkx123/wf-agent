/**
 * Template Validators
 *
 * Unified validation framework for all template types.
 * Provides consistent validation schemas and methods for:
 * - PromptTemplate
 * - SystemPromptFragment
 * - HookTemplate
 * - NodeTemplate
 * - TriggerTemplate
 */

import type {
  PromptTemplate,
  SystemPromptFragment,
  HookTemplate,
  NodeTemplate,
  TriggerTemplate,
} from "@wf-agent/types";
import {
  validateEntityOrThrow,
  type EntityValidationSchema,
} from "./entity-validation.js";
import { RegistryValidationError } from "../../types.js";

/**
 * Validation schema for PromptTemplate
 */
export const PROMPT_TEMPLATE_SCHEMA: EntityValidationSchema<PromptTemplate>[] = [
  {
    field: "id",
    required: true,
    type: "string",
    message: "Template ID is required and must be a non-empty string",
  },
  {
    field: "content",
    required: true,
    type: "string",
    message: "Template content is required and must be a non-empty string",
  },
  {
    field: "name",
    required: true,
    type: "string",
  },
  {
    field: "category",
    required: true,
    type: "string",
  },
];

/**
 * Validation schema for SystemPromptFragment
 */
export const FRAGMENT_SCHEMA: EntityValidationSchema<SystemPromptFragment>[] = [
  {
    field: "id",
    required: true,
    type: "string",
    message: "Fragment ID is required and must be a non-empty string",
  },
  {
    field: "content",
    required: true,
    type: "string",
    message: "Fragment content is required and must be a non-empty string",
  },
];

/**
 * Validation schema for HookTemplate
 */
export const HOOK_TEMPLATE_SCHEMA: EntityValidationSchema<HookTemplate>[] = [
  {
    field: "name",
    required: true,
    type: "string",
    message: "Hook template name is required",
  },
  {
    field: "hook",
    required: true,
    type: "object",
    message: "Hook configuration is required",
  },
];

/**
 * Validation schema for NodeTemplate
 */
export const NODE_TEMPLATE_SCHEMA: EntityValidationSchema<NodeTemplate>[] = [
  {
    field: "name",
    required: true,
    type: "string",
    message: "Node template name is required",
  },
  {
    field: "type",
    required: true,
    type: "string",
    message: "Node template type is required",
  },
  {
    field: "config",
    required: true,
    type: "object",
    message: "Node template config is required",
  },
];

/**
 * Validation schema for TriggerTemplate
 */
export const TRIGGER_TEMPLATE_SCHEMA: EntityValidationSchema<TriggerTemplate>[] = [
  {
    field: "name",
    required: true,
    type: "string",
    message: "Trigger template name is required",
  },
  {
    field: "condition",
    required: true,
    type: "object",
    message: "Trigger configuration is required",
  },
  {
    field: "action",
    required: true,
    type: "object",
    message: "Trigger action is required",
  },
];

/**
 * Validates a PromptTemplate
 */
export function validatePromptTemplate(template: PromptTemplate): void {
  validateEntityOrThrow(template, PROMPT_TEMPLATE_SCHEMA, "PromptTemplate");
}

/**
 * Validates a SystemPromptFragment with variable usage checking
 */
export function validateFragment(
  fragment: SystemPromptFragment,
  logger?: { warn: (msg: string) => void },
): void {
  validateEntityOrThrow(fragment, FRAGMENT_SCHEMA, "SystemPromptFragment");

  const variables = fragment.variables as
    | Array<{ name: string; required?: boolean }>
    | undefined;
  if (variables && variables.length > 0) {
    const content = fragment.content as string;
    for (const variable of variables) {
      const placeholder = `{{${variable.name}}}`;
      const isUsed = content.includes(placeholder);
      if (!isUsed && variable.required && logger) {
        logger.warn(
          `Fragment '${fragment.id}' declares required variable '${variable.name}' ` +
            `but it is not used in the content`,
        );
      }
    }
  }
}

/**
 * Validates a HookTemplate
 */
export function validateHookTemplate(template: HookTemplate): void {
  validateEntityOrThrow(template, HOOK_TEMPLATE_SCHEMA, "HookTemplate");

  const hook = template.hook;
  if (!hook.hookType || typeof hook.hookType !== "string") {
    throw new RegistryValidationError("Hook template hook.hookType is required", "hook.hookType");
  }

  if (!hook.eventName || typeof hook.eventName !== "string") {
    throw new RegistryValidationError("Hook template hook.eventName is required", "hook.eventName");
  }
}

/**
 * Validates a NodeTemplate
 */
export function validateNodeTemplate(template: NodeTemplate): void {
  validateEntityOrThrow(template, NODE_TEMPLATE_SCHEMA, "NodeTemplate");
}

/**
 * Validates a TriggerTemplate
 */
export function validateTriggerTemplate(template: TriggerTemplate): void {
  validateEntityOrThrow(template, TRIGGER_TEMPLATE_SCHEMA, "TriggerTemplate");
}
