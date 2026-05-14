/**
 * Common Types Test
 * 
 * Tests for basic common type definitions including ID, Timestamp, Version, and Metadata.
 * These types are foundational building blocks used throughout the SDK.
 * 
 * Priority: 🟢 LOW (Stage 3)
 */

import { expectType, expectAssignable } from "tsd";
import type { ID, Timestamp, Version, Metadata } from "../../../src/index.js";

// ============================================================================
// Test 1: ID Type - Basic Usage
// ============================================================================

/**
 * Test that ID is a string type alias
 */
declare const id: ID;
expectType<string>(id);

/**
 * Test that ID can be assigned string literals
 */
const uuid: ID = "550e8400-e29b-41d4-a716-446655440000";
expectType<ID>(uuid);

/**
 * Test that ID supports various string formats
 */
const numericId: ID = "12345";
const prefixedId: ID = "user_abc123";
const complexId: ID = "workflow-2024-01-15-v1.0.0";

expectType<ID>(numericId);
expectType<ID>(prefixedId);
expectType<ID>(complexId);

/**
 * Test that ID is strongly typed (these would fail in regular TypeScript)
 * Note: In tsd tests, we focus on positive type assertions
 */

// ============================================================================
// Test 2: Timestamp Type - Millisecond Precision
// ============================================================================

/**
 * Test that Timestamp is a number type alias
 */
declare const timestamp: Timestamp;
expectType<number>(timestamp);

/**
 * Test that Timestamp can represent current time
 */
const now: Timestamp = Date.now();
expectType<Timestamp>(now);

/**
 * Test that Timestamp supports millisecond precision
 */
const preciseTimestamp: Timestamp = 1705334400000; // 2024-01-15T12:00:00.000Z
expectType<Timestamp>(preciseTimestamp);

/**
 * Test that Timestamp can be used in arithmetic operations
 */
const futureTimestamp: Timestamp = now + 3600000; // 1 hour later
expectType<Timestamp>(futureTimestamp);

/**
 * Test that Timestamp is strongly typed for millisecond values
 * Note: Negative tests are documented but not enforced in tsd
 */

// ============================================================================
// Test 3: Version Type - Semantic Versioning
// ============================================================================

/**
 * Test that Version is a string type alias
 */
declare const version: Version;
expectType<string>(version);

/**
 * Test that Version supports semantic versioning format
 */
const semver: Version = "1.0.0";
const preRelease: Version = "2.0.0-beta.1";
const buildMetadata: Version = "1.2.3+build.123";

expectType<Version>(semver);
expectType<Version>(preRelease);
expectType<Version>(buildMetadata);

/**
 * Test that Version can represent various version formats
 */
const simpleVersion: Version = "1";
const twoPartVersion: Version = "1.0";
const fullVersion: Version = "1.2.3-alpha+metadata";

expectType<Version>(simpleVersion);
expectType<Version>(twoPartVersion);
expectType<Version>(fullVersion);

/**
 * Test that Version follows semantic versioning conventions
 * Note: Runtime validation should enforce format constraints
 */

// ============================================================================
// Test 4: Metadata Type - Flexible Key-Value Pairs
// ============================================================================

/**
 * Test that Metadata is Record<string, unknown>
 */
declare const metadata: Metadata;
expectType<Record<string, unknown>>(metadata);

/**
 * Test that Metadata can hold various value types
 */
const meta1: Metadata = {
  author: "John Doe",
  createdAt: 1705334400000,
  tags: ["important", "review"],
  nested: {
    key: "value",
  },
};

expectType<Metadata>(meta1);

/**
 * Test that Metadata supports optional fields
 */
const meta2: Metadata = {
  required: "value",
  optional: undefined,
  nullable: null,
};

expectType<Metadata>(meta2);

/**
 * Test that Metadata can be empty
 */
const emptyMeta: Metadata = {};
expectType<Metadata>(emptyMeta);

/**
 * Test that Metadata values can be accessed with unknown type
 */
declare const meta: Metadata;
const value: unknown = meta["key"];
expectType<unknown>(value);

/**
 * Test that Metadata requires object structure
 * Note: Type system enforces Record<string, unknown> constraint
 */

// ============================================================================
// Test 5: Combined Usage - Real-world Scenarios
// ============================================================================

/**
 * Test using multiple common types together
 */
interface Entity {
  id: ID;
  version: Version;
  createdAt: Timestamp;
  updatedAt: Timestamp;
  metadata: Metadata;
}

const entity: Entity = {
  id: "entity-001",
  version: "1.0.0",
  createdAt: Date.now(),
  updatedAt: Date.now(),
  metadata: {
    description: "Test entity",
    status: "active",
  },
};

expectType<Entity>(entity);

/**
 * Test that Entity fields have correct types
 */
expectType<ID>(entity.id);
expectType<Version>(entity.version);
expectType<Timestamp>(entity.createdAt);
expectType<Timestamp>(entity.updatedAt);
expectType<Metadata>(entity.metadata);

/**
 * Test function signatures using common types
 */
function createEntity(
  id: ID,
  version: Version,
  metadata?: Metadata,
): { id: ID; version: Version; timestamp: Timestamp; metadata: Metadata } {
  return {
    id,
    version,
    timestamp: Date.now(),
    metadata: metadata || {},
  };
}

const newEntity = createEntity("test-id", "1.0.0", { test: true });
expectType<ID>(newEntity.id);
expectType<Version>(newEntity.version);
expectType<Timestamp>(newEntity.timestamp);
expectType<Metadata>(newEntity.metadata);

// ============================================================================
// Test 6: Type Assignability
// ============================================================================

/**
 * Test that string is assignable to ID
 */
const str: string = "test";
expectAssignable<ID>(str);

/**
 * Test that number is assignable to Timestamp
 */
const num: number = 1234567890;
expectAssignable<Timestamp>(num);

/**
 * Test that string is assignable to Version
 */
const ver: string = "1.0.0";
expectAssignable<Version>(ver);

/**
 * Test that Record<string, any> is assignable to Metadata
 */
const record: Record<string, any> = { key: "value" };
expectAssignable<Metadata>(record);

/**
 * Test that ID is assignable to string (bidirectional)
 */
const idValue: ID = "test";
expectAssignable<string>(idValue);

// ============================================================================
// Test 7: Array and Collection Usage
// ============================================================================

/**
 * Test arrays of common types
 */
const ids: ID[] = ["id1", "id2", "id3"];
expectType<ID[]>(ids);

const timestamps: Timestamp[] = [Date.now(), Date.now() + 1000];
expectType<Timestamp[]>(timestamps);

const versions: Version[] = ["1.0.0", "2.0.0", "3.0.0"];
expectType<Version[]>(versions);

const metadatas: Metadata[] = [{}, { key: "value" }];
expectType<Metadata[]>(metadatas);

/**
 * Test Map with common types
 */
const idToMetadata: Map<ID, Metadata> = new Map([
  ["id1", { name: "Item 1" }],
  ["id2", { name: "Item 2" }],
]);

expectType<Map<ID, Metadata>>(idToMetadata);

/**
 * Test Set with common types
 */
const idSet: Set<ID> = new Set(["id1", "id2", "id3"]);
expectType<Set<ID>>(idSet);
