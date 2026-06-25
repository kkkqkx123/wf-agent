/**
 * Dynamic Context Module
 *
 * Generates dynamic system context and user context at runtime
 * to enable KV cache optimization (Anthropic Prompt Caching).
 *
 * ## Architecture
 *
 * Two-layer context design:
 *
 * 1. **System Context** (stable, cacheable)
 *    - Current time and timezone
 *    - Available tools documentation
 *    - Environment information
 *    - Merged into: system message (stays constant)
 *
 * 2. **User Context** (variable, not cached)
 *    - TODO lists, task status
 *    - Pinned files content
 *    - Real-time state changes
 *    - Appended to: last user message (changes frequently)
 *
 * ## Usage
 *
 * Applications combine context builders with messaging injection:
 *
 * ```typescript
 * import { buildSystemContextPrompt } from "@wf-agent/sdk/resources";
 * import { injectDynamicPrompts } from "@wf-agent/sdk/shared/messaging";
 *
 * const staticSystem = await buildSystemContextPrompt(config);
 * const messages = injectDynamicPrompts(messages, staticSystem, undefined);
 * ```
 */

// System context builders (stable, for system message)
export { buildSystemContextPrompt } from "./system-context/index.js";

// User context builders (variable, for last user message)
export { buildUserContextContent } from "./user-context/index.js";


