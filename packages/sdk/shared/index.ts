/**
 * Shared Module Exports
 *
 * ARCHITECTURE NOTE: This module exports only components defined directly in the shared directory.
 * Submodules (persistence, checkpoint, etc.) are NOT re-exported here to maintain clear dependency graphs.
 *
 * Import directly from submodules:
 *   - import { Component } from "@wf-agent/sdk/shared/persistence/core"
 *   - import { Component } from "@wf-agent/sdk/shared/checkpoint"
 *   - import { Component } from "@wf-agent/sdk/shared/coordinators"
 *
 * This approach:
 * ✓ Makes dependencies explicit
 * ✓ Enables better tree-shaking
 * ✓ Prevents circular dependency issues
 * ✓ Clarifies what is public API vs internal
 */

// Only export components defined directly in this directory
export * from "./global-context.js";

