/**
 * Registry Extensions
 *
 * Utility functions for common registry operations that can be composed
 * onto any RegistryImpl-based registry to reduce boilerplate.
 *
 * Each function takes a registry instance (or a list of items) and returns
 * the filtered/transformed result, avoiding the need to duplicate the same
 * filtering logic across multiple registries.
 */

/**
 * Create a search function that filters items by keyword across specified fields.
 * Returns items where any of the specified fields contain the query (case-insensitive).
 *
 * @param listFn - Function that returns the full list of items
 * @param fields - Array of field accessors to search against
 * @returns Search function
 */
export function createSearch<T>(
  listFn: () => T[],
  fields: Array<(item: T) => string | undefined>,
): (query: string) => T[] {
  return (query: string): T[] => {
    const lowerQuery = query.toLowerCase();
    return listFn().filter(item =>
      fields.some(field => {
        const value = field(item);
        return value != null && value.toLowerCase().includes(lowerQuery);
      }),
    );
  };
}

/**
 * Create a filter-by-category function.
 *
 * @param listFn - Function that returns the full list of items
 * @param categoryAccessor - Accessor to extract the category from an item
 * @returns Category filter function
 */
export function createListByCategory<T>(
  listFn: () => T[],
  categoryAccessor: (item: T) => string | undefined,
): (category: string) => T[] {
  return (category: string): T[] =>
    listFn().filter(item => categoryAccessor(item) === category);
}

/**
 * Create a filter-by-tags function.
 * Only items that have ALL specified tags are returned.
 *
 * @param listFn - Function that returns the full list of items
 * @param tagsAccessor - Accessor to extract the tags array from an item
 * @returns Tags filter function
 */
export function createListByTags<T>(
  listFn: () => T[],
  tagsAccessor: (item: T) => string[] | undefined,
): (tags: string[]) => T[] {
  return (tags: string[]): T[] =>
    listFn().filter(item => {
      const itemTags = tagsAccessor(item) || [];
      return tags.every(tag => itemTags.includes(tag));
    });
}

/**
 * Create a batch-register function that delegates to a single-item register function.
 *
 * @param registerFn - Single-item register function
 * @returns Batch register function
 */
export function createRegisterBatch<T>(
  registerFn: (item: T) => void,
): (items: T[]) => void {
  return (items: T[]): void => {
    for (const item of items) {
      registerFn(item);
    }
  };
}

/**
 * Create a batch-unregister function that delegates to a single-item unregister function.
 *
 * @param unregisterFn - Single-item unregister function
 * @returns Batch unregister function
 */
export function createUnregisterBatch(
  unregisterFn: (key: string) => void,
): (keys: string[]) => void {
  return (keys: string[]): void => {
    for (const key of keys) {
      unregisterFn(key);
    }
  };
}

/**
 * Create an export function that serializes an item to JSON.
 *
 * @param getFn - Function that retrieves an item by key
 * @param notFoundFn - Function that throws a not-found error (or creates an error message)
 * @returns Export function
 */
export function createExport<T>(
  getFn: (key: string) => T | undefined,
  notFoundFn: (key: string) => Error,
): (key: string) => string {
  return (key: string): string => {
    const item = getFn(key);
    if (!item) {
      throw notFoundFn(key);
    }
    return JSON.stringify(item, null, 2);
  };
}

/**
 * Create an import function that deserializes and registers an item from JSON.
 *
 * @param registerFn - Single-item register function
 * @param parseErrorFn - Function that creates a parse error from an error
 * @returns Import function
 */
export function createImport<T>(
  registerFn: (item: T) => string,
  parseErrorFn: (error: unknown) => Error,
): (json: string) => string {
  return (json: string): string => {
    try {
      const item = JSON.parse(json) as T;
      return registerFn(item);
    } catch (error) {
      if (error instanceof Error && !(error instanceof SyntaxError)) {
        throw error;
      }
      throw parseErrorFn(error);
    }
  };
}