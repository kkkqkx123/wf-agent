/**
 * Modular Agent SDK - Main Entry Point
 * 
 * This is the main entry point for the SDK.
 * 
 * Usage patterns:
 * 
 * 1. Import from specific submodules (recommended):
 *    import { EventRegistry } from "@wf-agent/sdk/core";
 *    import { AgentLoopExecutor } from "@wf-agent/sdk/agent";
 *    import { GracefulShutdownManager } from "@wf-agent/sdk/services";
 * 
 * 2. Import from main entry (limited exports to avoid conflicts):
 *    import { SDKInstance, createSDK } from "@wf-agent/sdk";
 */

// Main SDK instance and factory (primary exports)
export { SDKInstance } from "./api/index.js";
export { createSDK } from "./api/shared/core/sdk.js";

// Re-export module namespaces for organized access
export * as api from "./api/index.js";
export * as core from "./core/index.js";
export * as services from "./services/index.js";
export * as agent from "./agent/index.js";
export * as utils from "./utils/index.js";
export * as resources from "./resources/index.js";
