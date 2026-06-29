/**
 * Adapter Requirements Definition
 *
 * Manages validation of storage adapters provided by users.
 *
 * SCOPE: This module ONLY handles user-provided storage adapters
 * - NOT SDK-created config objects (those are managed by RegistriesConfig)
 * - NOT non-adapter components (like MetricsConfig, which is config)
 *
 * REQUIRED: Missing adapter will cause SDK initialization to fail
 * OPTIONAL: Missing adapter is skipped with a warning log
 *
 * This module ensures consistent adapter validation across the SDK
 * and makes it easy to add/modify adapter requirements in the future.
 */

/**
 * Adapter Requirement Levels
 */
export enum AdapterRequirement {
  REQUIRED = "required",
  OPTIONAL = "optional",
}

/**
 * User-provided storage adapter requirements mapping
 *
 * This is the single source of truth for which STORAGE ADAPTERS are required vs optional.
 *
 * INCLUDES: 11 user-provided storage adapters
 * EXCLUDES: SDK-created managers, configs, non-adapter components
 *
 * When adding new storage adapters:
 * 1. Add to this map with REQUIRED or OPTIONAL level
 * 2. Update the adapter interfaces in @wf-agent/storage
 * 3. Update SDKOptions to accept the adapter parameter
 * 4. Update RegistriesConfig for registry-specific configuration
 *
 * Update this when adding new storage adapters to the SDK.
 */
export const ADAPTER_REQUIREMENTS_MAP = {
  // Core persistence (REQUIRED)
  checkpoint: AdapterRequirement.REQUIRED,

  // User-provided storage adapters (OPTIONAL but recommended for production)
  workflow: AdapterRequirement.OPTIONAL,
  workflowExecution: AdapterRequirement.OPTIONAL,
  trigger: AdapterRequirement.OPTIONAL,
  task: AdapterRequirement.OPTIONAL,
  agentLoop: AdapterRequirement.OPTIONAL,
  tool: AdapterRequirement.OPTIONAL,
  script: AdapterRequirement.OPTIONAL,
  nodeTemplate: AdapterRequirement.OPTIONAL,
  hookTemplate: AdapterRequirement.OPTIONAL,
  agentProfile: AdapterRequirement.OPTIONAL,
} as const;

/** List of required adapter names */
export const REQUIRED_ADAPTERS = Object.entries(ADAPTER_REQUIREMENTS_MAP)
  .filter(([_, level]) => level === AdapterRequirement.REQUIRED)
  .map(([name]) => name);

/** List of optional adapter names */
export const OPTIONAL_ADAPTERS = Object.entries(ADAPTER_REQUIREMENTS_MAP)
  .filter(([_, level]) => level === AdapterRequirement.OPTIONAL)
  .map(([name]) => name);

/**
 * Validate that all required adapters are configured
 *
 * @param config Adapter configuration object containing storage adapter instances
 * @throws {Error} If any required adapters are missing
 *
 * @example
 * ```typescript
 * validateAdapterRequirements({
 *   checkpoint: checkpointAdapter,
 *   workflow: workflowAdapter,
 * });
 * // Throws if checkpoint adapter is not configured
 * ```
 */
export function validateAdapterRequirements(config: Record<string, any>): void {
  const missingRequired: string[] = [];

  for (const adapterName of REQUIRED_ADAPTERS) {
    if (!config[adapterName]) {
      missingRequired.push(adapterName);
    }
  }

  if (missingRequired.length > 0) {
    throw new Error(
      `REQUIRED storage adapter(s) missing: ${missingRequired.join(", ")}. ` +
        `These adapters must be provided to create an SDK instance.`,
    );
  }
}

/**
 * Get warning messages for missing optional adapters
 *
 * @param config Adapter configuration object containing storage adapter instances
 * @returns Array of warning messages for missing optional adapters
 *
 * @example
 * ```typescript
 * const warnings = getOptionalAdapterWarnings({
 *   checkpoint: checkpointAdapter,
 * });
 * warnings.forEach(warning => logger.warn(warning));
 * ```
 */
export function getOptionalAdapterWarnings(config: Record<string, any>): string[] {
  const warnings: string[] = [];

  for (const adapterName of OPTIONAL_ADAPTERS) {
    if (!config[adapterName]) {
      // Provide context-aware warning messages
      const contextMessage = getAdapterContextMessage(adapterName);
      warnings.push(
        `Optional storage adapter '${adapterName}' is not configured. ` +
          `${contextMessage} For production use, please provide this adapter.`,
      );
    }
  }

  return warnings;
}

/**
 * Get context-aware messages for specific adapters
 *
 * @param adapterName Name of the adapter
 * @returns Context message explaining the impact of missing this adapter
 */
function getAdapterContextMessage(adapterName: string): string {
  const contextMap: Record<string, string> = {
    checkpoint: "Checkpoints will not be persisted.",
    workflow: "Workflows will not be persisted.",
    workflowExecution: "Execution history will not be persisted.",
    task: "Tasks will not be persisted.",
    trigger: "Trigger templates will not be persisted.",
    agentLoop: "Agent loop state will not be persisted.",
    tool: "Tools will not be persisted.",
    script: "Scripts will not be persisted.",
    nodeTemplate: "Node templates will not be persisted.",
    hookTemplate: "Hook templates will not be persisted.",
    agentProfile: "Agent profiles will not be persisted.",
  };

  return contextMap[adapterName] || "Data will not be persisted.";
}
