/**
 * Registration Result Types
 *
 * Defines types for aggregated registration results across all three pipelines:
 * - Predefined resources (SDK built-in)
 * - Custom resources (user-provided via config files)
 * - Application resources (runtime-defined, reserved for future use)
 */

/**
 * Registration result for a specific resource type
 */
export interface ResourceRegistrationResult {
  success: string[];
  failures: Array<{ id: string; error: string }>;
}

/**
 * Predefined Resources Registration Result
 *
 * Results from registering predefined (SDK built-in) resources.
 */
export interface PredefinedRegistrationResult {
  prompts: ResourceRegistrationResult;
  fragments: ResourceRegistrationResult;
  toolDescriptions: ResourceRegistrationResult;
  tools: ResourceRegistrationResult;
  triggers: ResourceRegistrationResult;
  workflows: ResourceRegistrationResult;
}

/**
 * Custom Resources Registration Result
 *
 * Results from registering custom (user-provided) resources.
 */
export interface CustomResourcesRegistrationResult {
  tools: ResourceRegistrationResult;
  triggers: ResourceRegistrationResult;
  prompts: ResourceRegistrationResult;
}

/**
 * Application Resources Registration Result (reserved for future use)
 *
 * Results from registering application-level (runtime-defined) resources.
 */
export interface ApplicationResourcesRegistrationResult {
  tools: ResourceRegistrationResult;
}

/**
 * Registration Result
 *
 * Aggregates registration results from all three pipelines.
 * Provides a complete view of what was successfully registered
 * and what failed across all resource types and sources.
 */
export interface RegistrationResult {
  predefined: PredefinedRegistrationResult;
  custom: CustomResourcesRegistrationResult;
  application?: ApplicationResourcesRegistrationResult;
}

/**
 * Registries required for resource registration
 */
export interface ResourceRegistries {
  triggerRegistry: import("@sdk/shared/registry/trigger-template-registry.js").TriggerTemplateRegistry;
  workflowRegistry: import("@sdk/workflow/stores/workflow-registry.js").WorkflowRegistry;
  toolRegistry: import("@sdk/shared/registry/tool-registry.js").ToolRegistry;
  promptTemplateRegistry: import("../../shared/registry/prompt-template-registry.js").PromptTemplateRegistry;
  fragmentRegistry: import("../../shared/registry/fragment-registry.js").FragmentRegistry;
  toolDescriptionRegistry: import("../../shared/utils/tools/tool-description-registry.js").ToolDescriptionRegistry;
}

/**
 * Options for resource registration
 */
export interface ResourceRegistrationOptions {
  skipIfExists: boolean;
  promptsEnabled: boolean;
  toolDescriptionsEnabled: boolean;
}
