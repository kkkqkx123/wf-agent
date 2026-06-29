/**
 * Unified Persistence Framework
 *
 * Provides a centralized persistence management framework for all registries.
 *
 * Core Components:
 * 1. BasePersistentRegistry - Abstract base class for all persistent registries
 * 2. PersistenceStrategy - Configurable persistence strategies (ASYNC/BLOCKING)
 *
 * Features:
 * - Unified persistence management with strategy support
 * - Event-driven architecture for monitoring
 * - Data consistency verification and recovery
 * - Automatic failure tracking
 * - Lifecycle hooks
 * - Centralized error handling
 * - Automatic ID extraction via IdExtractor pattern
 *
 * IMPORTANT: This module exports only persistence framework core.
 * For event notifications and validation, import from @wf-agent/sdk/shared/storage
 *
 * Import examples:
 * ```typescript
 * // Persistence framework core
 * import { BasePersistentRegistry, type IdExtractor }
 *   from "@wf-agent/sdk/shared/persistence/core"
 *
 * // Event notifications
 * import { PersistenceEventEmitter }
 *   from "@wf-agent/sdk/shared/storage"
 *
 * // Data validation
 * import { DataConsistencyValidator }
 *   from "@wf-agent/sdk/shared/storage"
 *
 * class MyRegistry extends BasePersistentRegistry<MyEntity> {
 *   protected getIdExtractor(): IdExtractor<MyEntity> {
 *     return { extractId: (e) => e.id }
 *   }
 *
 *   protected async serializeEntity(entity: MyEntity): Promise<Uint8Array> {
 *     const encoder = new TextEncoder();
 *     return encoder.encode(JSON.stringify(entity));
 *   }
 *
 *   protected async buildMetadata(entity: MyEntity): Promise<any> {
 *     return { entityId: entity.id, timestamp: Date.now() };
 *   }
 *
 *   protected getRegistryName(): string {
 *     return "MyRegistry";
 *   }
 * }
 * ```
 */

// Core persistence framework
export type {
  PersistenceConfig,
  RequiredPersistenceConfig,
  RegistryPersistenceConfig,
  MetadataBuilder,
  EntitySerializer,
  PersistenceHooks,
  IdExtractor,
} from "./core/types.js";
export { PersistenceStrategy } from "./core/types.js";
export {
  BasePersistentRegistry,
  DEFAULT_PERSISTENCE_CONFIG,
  mergePersistenceConfig,
} from "./core/base-persistent-registry.js";
