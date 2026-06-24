/**
 * Unified Registration Result Types
 *
 * Defines types for aggregated registration results across all three pipelines:
 * - Predefined resources (SDK built-in)
 * - Custom resources (user-provided via config files)
 * - Application resources (runtime-defined, reserved for future use)
 */

/**
 * Predefined Resources Registration Result
 *
 * Results from registering predefined (SDK built-in) resources.
 */
export interface PredefinedRegistrationResult {
  tools: {
    success: string[];
    failures: Array<{ toolId: string; error: string }>;
  };
  triggers: {
    success: string[];
    failures: Array<{ triggerName: string; error: string }>;
  };
  workflows: {
    success: string[];
    failures: Array<{ workflowId: string; error: string }>;
  };
}

/**
 * Custom Resources Registration Result
 *
 * Results from registering custom (user-provided) resources.
 */
export interface CustomResourcesRegistrationResult {
  tools: {
    success: string[];
    failures: Array<{ toolId: string; error: string }>;
  };
  triggers: {
    success: string[];
    failures: Array<{ triggerName: string; error: string }>;
  };
  prompts: {
    success: string[];
    failures: Array<{ promptId: string; error: string }>;
  };
}

/**
 * Application Resources Registration Result (reserved for future use)
 *
 * Results from registering application-level (runtime-defined) resources.
 */
export interface ApplicationResourcesRegistrationResult {
  tools: {
    success: string[];
    failures: Array<{ toolId: string; error: string }>;
  };
  // Additional resource types can be added as needed
}

/**
 * Unified Registration Result
 *
 * Aggregates registration results from all three pipelines.
 * Provides a complete view of what was successfully registered
 * and what failed across all resource types and sources.
 */
export interface UnifiedRegistrationResult {
  predefined: PredefinedRegistrationResult;
  custom: CustomResourcesRegistrationResult;
  application?: ApplicationResourcesRegistrationResult;
}
