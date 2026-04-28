/**
 * Common Type Definitions
 * Define general basic types, including IDs, timestamps, versions, metadata, etc.
 *
 * Design Principles:
 * - Use type aliases instead of classes to maintain simplicity.
 * - Facilitate serialization and deserialization.
 * - Suitable for use in SDK scenarios.
 * - Place utility functions in the sdk/utils/ directory.
 */

/**
 * ID Type (Type Alias)
 * Uses strings as IDs, supporting UUID or other formats.
 */
export type ID = string;

/**
 * Timestamp types (type aliases)
 * Using millisecond timestamps
 */
export type Timestamp = number;

/**
 * Version types (type aliases)
 * Follows semanticized versioning specifications (e.g., "1.0.0")
 */
export type Version = string;

/**
 * Metadata types (type aliases)
 * Supports any key-value pair
 */
export type Metadata = Record<string, unknown>;
