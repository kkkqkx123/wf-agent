/**
 * Unified Export of SDK Resource Modules
 *
 * Resources are divided into three categories:
 *
 * ## 1. Predefined Resources (static, built-in)
 * - Prompt templates
 * - Built-in tools (agent, workflow, skill, etc.)
 * - Triggers and workflows
 *
 * ## 2. Custom Resources (user-provided via config files)
 * - User-defined tools, triggers, and prompts
 * - Loaded from configuration files
 *
 * ## 3. Dynamic Resources (generated at runtime)
 * - System context injection (time, tools, environment)
 * - User context injection (TODOs, state)
 * - See DynamicPromptInjection for two-layer design
 *
 * ## 4. Unified Registration
 * - Coordinates registration of all three resource types
 * - Orchestrates the three-pipeline registration flow
 *
 * ## Usage
 *
 * ```typescript
 * // Predefined resources
 * import { createPredefinedTools, createBuiltinTools } from "@wf-agent/sdk/resources";
 *
 * // Custom resources
 * import { loadCustomResourcesFromConfig } from "@wf-agent/sdk/resources";
 *
 * // Unified registration
 * import { registerAllResources } from "@wf-agent/sdk/resources";
 *
 * // Dynamic prompt injection
 * import { buildDynamicPromptInjection } from "@wf-agent/sdk/resources";
 * ```
 */

// Predefined content (static templates, tools, triggers, workflows)
export * from "./predefined/index.js";

// Custom resources (user-provided from config files)
export * from "./custom/index.js";

// Unified registration orchestrator
export * from "./registration/index.js";

// Dynamic content (runtime-generated context injection)
export * from "./dynamic/index.js";
