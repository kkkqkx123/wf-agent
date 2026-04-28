/**
 * Modular Agent SDK - Main Entry Point
 */

import { initializeContainer } from "./core/di/container-config.js";

// Initialize the DI container
initializeContainer();

// Re-export API layer
export * from "./api/index.js";

// Re-export utilities
export * from "./utils/index.js";

// Re-export resources
export * from "./resources/index.js";
