/**
 * Unified Export of SDK Resource Modules
 *
 * Resources are divided into two categories:
 *
 * ## 1. Predefined Resources (static, file-based)
 * - Prompt templates
 * - Built-in tools (agent, workflow, skill, etc.)
 * - Triggers and workflows
 *
 * ## 2. Dynamic Resources (generated at runtime)
 * - System context injection (time, tools, environment)
 * - User context injection (TODOs, state)
 * - See DynamicPromptInjection for two-layer design
 *
 * ## Usage
 *
 * ```typescript
 * // Predefined resources
 * import { createPredefinedTools, createBuiltinTools } from "@wf-agent/sdk/resources";
 *
 * // Dynamic prompt injection (main API)
 * import { buildDynamicPromptInjection } from "@wf-agent/sdk/resources";
 *
 * const injection = await buildDynamicPromptInjection(context, config);
 * ```
 */

// Predefined content (static templates, tools, triggers, workflows)
export * from "./predefined/index.js";

// Dynamic content (runtime-generated context injection)
export * from "./dynamic/index.js";

