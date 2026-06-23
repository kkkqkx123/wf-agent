/**
 * Dynamic Prompt Injection Module
 *
 * Generates dynamic system context and user context at runtime
 * to enable KV cache optimization (Anthropic Prompt Caching).
 *
 * ## Architecture
 *
 * Two-layer injection design:
 *
 * 1. **System Context** (stable, cacheable)
 *    - Current time and timezone
 *    - Available tools documentation
 *    - Environment information
 *    - Injected into: system message (stays constant)
 *
 * 2. **User Context** (variable, not cached)
 *    - TODO lists, task status
 *    - Pinned files content
 *    - Real-time state changes
 *    - Appended to: last user message (changes frequently)
 *
 * ## Structure
 *
 * ```
 * dynamic/
 * ├── system-context/     ← Stable context builders
 * │   ├── builder.ts
 * │   └── fragments/
 * ├── user-context/       ← Variable context builders
 * │   ├── builder.ts
 * │   └── fragments/
 * ├── injection.ts        ← Unified interface
 * └── index.ts           ← Public exports
 * ```
 *
 * ## Core API
 *
 * Applications should import only the main entry point:
 *
 * ```typescript
 * import { buildDynamicPromptInjection } from "@wf-agent/sdk/resources";
 *
 * const injection = await buildDynamicPromptInjection(context, config);
 * // injection.staticSystem → merge into system message
 * // injection.dynamicUserContext → append to last user message
 * ```
 */

// Core public API - unified entry point
export { buildDynamicPromptInjection } from "./injection.js";

// Module-level APIs (for special cases, prefer buildDynamicPromptInjection)
export { buildSystemContextPrompt } from "./system-context/index.js";
export { buildUserContextContent } from "./user-context/index.js";


