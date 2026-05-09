/**
 * Modular Agent SDK - Main Entry Point
 */

import { registerAllSerializers } from "./core/serialization/entities/index.js";

// Register all entity serializers with the global registry
registerAllSerializers();

// Re-export API layer
export * from "./api/index.js";

// Re-export utilities
export * from "./utils/index.js";

// Re-export resources
export * from "./resources/index.js";
