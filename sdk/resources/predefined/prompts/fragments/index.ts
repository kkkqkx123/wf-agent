/**
 * System prompt fragments are exported uniformly.
 *
 * All fragments are defined at the SDK level and contain the actual business content.
 */

// Type definitions
export * from "./types.js";

// Role Definition Segment
export * from "./role/index.js";

// Ability Description Section
export * from "./capability/index.js";

// Constraint segment
export * from "./constraint/index.js";

// Tool Usage Guidelines Section
export * from "./tool-usage/index.js";

// Fragment registration and combination tool
export * from "./registry.js";
export * from "./composer.js";
