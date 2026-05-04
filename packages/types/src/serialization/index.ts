/**
 * Serialization Types Index
 *
 * Exports all serialization-related types.
 */

export * from "./base.js";

// Re-export SerializedError for backward compatibility and centralized access
export { type SerializedError } from "../errors/serialized-error.js";
