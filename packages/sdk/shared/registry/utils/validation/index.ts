/**
 * Validation Index
 *
 * Re-export all validation functions and schemas for convenient access.
 */

export type {
  RequiredFieldRule,
  ValidationResult,
  EntityValidationSchema,
} from "./entity-validation.js";

export {
  validateRequiredFields,
  validateRequiredString,
  validateIdentifier,
  validateBoolean,
  validatePositiveNumber,
  validateEnum,
  validateAtLeastOne,
  validateMetadata,
  combineValidationResults,
  isRegistryValidationError,
  validateEntityBySchema,
  validateEntityOrThrow,
} from "./entity-validation.js";

export {
  PROMPT_TEMPLATE_SCHEMA,
  FRAGMENT_SCHEMA,
  HOOK_TEMPLATE_SCHEMA,
  NODE_TEMPLATE_SCHEMA,
  TRIGGER_TEMPLATE_SCHEMA,
  validatePromptTemplate,
  validateFragment,
  validateHookTemplate,
  validateNodeTemplate,
  validateTriggerTemplate,
} from "./template-validation.js";

export {
  TRIGGER_TEMPLATE_EVENT_TYPES,
  TRIGGER_TEMPLATE_ACTION_TYPES,
  validateTriggerTemplate as validateTriggerTemplateRegistry,
} from "./trigger-template-validation.js";
