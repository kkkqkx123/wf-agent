/**
 * Persistence Utilities
 *
 * Helper functions for persistence operations
 */

/**
 * Default serializer for JSON-serializable entities
 */
export class DefaultEntitySerializer<T> {
  serialize(entity: T): Uint8Array {
    const encoder = new TextEncoder();
    return encoder.encode(JSON.stringify(entity));
  }

  deserialize(data: Uint8Array): T {
    const decoder = new TextDecoder();
    const json = decoder.decode(data);
    return JSON.parse(json) as T;
  }
}

/**
 * Metadata builder helper
 */
export class DefaultMetadataBuilder {
  static create(
    entityId: string,
    entityType: string,
    additionalFields?: Record<string, unknown>,
  ): Record<string, unknown> {
    return {
      entityId,
      entityType,
      timestamp: Date.now(),
      ...additionalFields,
    };
  }
}
