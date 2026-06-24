/**
 * Registry Core Types
 *
 * Provides base registry interfaces and error classes for all registry implementations.
 */

/**
 * Base registry interface for read-only operations.
 * All registries should implement this interface.
 */
export interface Registry<T> {
  get(key: string): T | undefined;
  has(key: string): boolean;
  list(): T[];
  keys(): string[];
  readonly size: number;
  clear(): void;
}

/**
 * Mutable registry interface for write operations.
 * Extends Registry<T> with mutation methods.
 */
export interface MutableRegistry<T> extends Registry<T> {
  set(key: string, value: T): void;
  delete(key: string): boolean;
}

/**
 * Registry error types for consistent error handling.
 */
export class RegistryError extends Error {
  readonly operation: string;
  readonly key?: string;
  override readonly cause?: Error;

  constructor(
    message: string,
    operation: string,
    key?: string,
    cause?: Error,
  ) {
    super(message);
    this.operation = operation;
    this.key = key;
    this.cause = cause;
    this.name = "RegistryError";
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

export class RegistryNotFoundError extends RegistryError {
  constructor(key: string, entityName: string = "item") {
    super(`${entityName} with key '${key}' not found`, "get", key);
    this.name = "RegistryNotFoundError";
  }
}

export class RegistryAlreadyExistsError extends RegistryError {
  constructor(key: string, entityName: string = "item") {
    super(`${entityName} with key '${key}' already exists`, "register", key);
    this.name = "RegistryAlreadyExistsError";
  }
}

export class RegistryValidationError extends RegistryError {
  constructor(message: string, key?: string) {
    super(message, "validate", key);
    this.name = "RegistryValidationError";
  }
}

/**
 * Interface for registries that support storage persistence.
 */
export interface PersistableRegistry<T> {
  readonly registry: MutableRegistry<T>;
  persist(item: T): Promise<void>;
  removeFromStorage(key: string): Promise<void>;
  loadFromStorage(key: string): Promise<T | null>;
  initializeFromStorage(): Promise<void>;
}

/**
 * Interface for registries that support batch operations.
 */
export interface BatchOperations<T> {
  registerBatch(items: T[]): Promise<void>;
  unregisterBatch(keys: string[]): Promise<void>;
}

/**
 * Interface for registries that support search operations.
 */
export interface SearchableRegistry<T> {
  search(query: string): T[];
  listByCategory(category: string): T[];
  listByTags(tags: string[]): T[];
}

/**
 * Interface for registries that support export/import operations.
 * @template T The type of items that can be exported/imported
 */
export interface ExportableRegistry<T> {
  /** Export an item as JSON string */
  export(key: string): string;
  /** Import an item from JSON string, returns the key */
  import(json: string): string;
  /** Type parameter marker for TypeScript */
  __typeMarker?: T;
}

/**
 * Options for registry operations.
 */
export interface RegistryOptions {
  skipIfExists?: boolean;
}

/**
 * Result of a registry operation.
 */
export interface RegistryOperationResult<T> {
  success: boolean;
  item?: T;
  error?: Error;
}

/**
 * Delete check result for safe-delete verification.
 */
export interface DeleteCheckResult {
  /** Whether the resource can be safely deleted */
  canDelete: boolean;
  /** Human-readable details about the check result */
  details: string;
  /** List of references pointing to this resource */
  references: Array<{
    /** Reference type: workflow, trigger, node, template, etc. */
    type: string;
    /** Source resource ID */
    sourceId: string;
    /** Source resource name */
    sourceName?: string;
    /** Additional details */
    details?: string;
  }>;
}

/**
 * Optional interface for registries that support reference checking.
 * Registries can implement this to provide efficient local reference queries.
 */
export interface IReferenceCheckable {
  /** Check if the resource has any references */
  checkReferences(id: string): Promise<DeleteCheckResult> | DeleteCheckResult;
}