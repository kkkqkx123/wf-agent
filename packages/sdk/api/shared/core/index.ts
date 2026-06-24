/**
 * Core API Layer - Persistence Exports
 * Provides unified access to persistence layer interfaces and implementations
 */

export type { PersistenceLayer, EventQueryFilter, TimeRange, PersistenceLayerHealth } from "./persistence-interfaces.js";
export { NoOpPersistenceLayer } from "./__tests__/no-op-persistence.js";
export { BufferedPersistenceLayer } from "./buffered-persistence.js";
