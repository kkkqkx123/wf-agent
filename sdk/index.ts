/**
 * Modular Agent SDK - Main Entry Point
 */

import { initializeContainerWithAdapters } from "./core/di/container-config.js";
import { registerAllSerializers } from "./core/serialization/entities/index.js";

// Initialize the DI container with no adapters (adapters will be provided via SDK options)
initializeContainerWithAdapters();

// Register all entity serializers with the global registry
registerAllSerializers();

// Re-export API layer
export * from "./api/index.js";

// Re-export utilities
export * from "./utils/index.js";

// Re-export resources
export * from "./resources/index.js";
