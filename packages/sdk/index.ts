/**
 * Modular Agent SDK - Main Entry Point
 *
 * This is the main entry point for the SDK. Only exports the core SDK instance.
 * All other components must be imported directly from their respective modules.
 *
 * Usage:
 *
 * Core SDK:
 *    import { SDKInstance, createSDK } from "@wf-agent/sdk";
 *
 * Import from specific modules (do NOT go through this entry point):
 *    import { BasePersistentRegistry } from "@wf-agent/sdk/shared/persistence/core";
 *    import { WorkflowExecutionRegistry } from "@wf-agent/sdk/workflow/stores";
 *    import { AgentLoopExecutor } from "@wf-agent/sdk/agent";
 */

// Main SDK instance and factory (primary exports)
export { SDKInstance } from "./api/index.js";
export { createSDK } from "./api/shared/core/sdk.js";

