/**
 * Dynamic Cue Module
 *
 * Manages dynamically generated prompt content at runtime
 * Separated from predefined prompts to focus on dynamic context injection
 *
 * Design principles:
 * - Context information remains unchanged during a single execution, stored using global variables
 * - Pure function design, no class instance management overhead.
 * - Static/dynamic separation: static content can be cached by the API, dynamic content is generated per request.
 *
 * directory structure:
 * - context.ts: global context store + pure functions
 * - fragments/: various fragment generators
 *
 * Type definitions:
 * - Type definitions have been migrated to the @wf-agent/types package.
 * - Please import the relevant types from @wf-agent/types
 */

// Segment Generator
export * from "./fragments/index.js";

// Context Management (Environment Information + Dynamic Context Generation)
export * from "./context.js";
