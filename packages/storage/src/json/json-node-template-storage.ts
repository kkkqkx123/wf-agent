/**
 * JSON File Node Template Storage Implementation
 * Node template persistent storage based on JSON file system with metadata-data separation
 */

import * as path from "path";
import type { NodeTemplateStorageMetadata, NodeTemplateListOptions } from "@wf-agent/types";
import type { NodeTemplateStorageAdapter } from "../types/adapter/base-storage-adapter.js";
import { BaseJsonStorage, BaseJsonStorageConfig } from "./base-json-storage.js";

/**
 * JSON File Node Template Storage
 * Implements the NodeTemplateStorageAdapter interface
 */
export class JsonNodeTemplateStorage
  extends BaseJsonStorage<NodeTemplateStorageMetadata, NodeTemplateListOptions>
  implements NodeTemplateStorageAdapter
{
  constructor(config: BaseJsonStorageConfig) {
    super(config);
  }

  /**
   * Get metadata directory path for node templates
   */
  protected override getMetadataDir(): string {
    return path.join(this.config.baseDir, "metadata", "nodeTemplate");
  }

  /**
   * Get data directory path for node templates
   */
  protected override getDataDir(): string {
    return path.join(this.config.baseDir, "data", "nodeTemplate");
  }

  override async list(options?: NodeTemplateListOptions): Promise<string[]> {
    this.ensureInitialized();

    let ids = this.getAllIds();

    if (options) {
      ids = ids.filter(id => {
        const entry = this.metadataIndex.get(id);
        if (!entry) return false;

        const metadata = entry.metadata;

        if (options.nameContains && !metadata.name.toLowerCase().includes(options.nameContains.toLowerCase())) {
          return false;
        }

        if (options.type) {
          const types = Array.isArray(options.type) ? options.type : [options.type];
          if (!types.includes(metadata.type)) {
            return false;
          }
        }

        if (options.category) {
          const categories = Array.isArray(options.category) ? options.category : [options.category];
          if (!metadata.category || !categories.includes(metadata.category)) {
            return false;
          }
        }

        if (options.tags && options.tags.length > 0) {
          if (!metadata.tags || !options.tags.some(tag => metadata.tags!.includes(tag))) {
            return false;
          }
        }

        if (options.descriptionContains && (!metadata.description || !metadata.description.toLowerCase().includes(options.descriptionContains.toLowerCase()))) {
          return false;
        }

        if (options.createdAfter !== undefined && metadata.createdAt < options.createdAfter) {
          return false;
        }
        if (options.createdBefore !== undefined && metadata.createdAt > options.createdBefore) {
          return false;
        }

        if (options.updatedAfter !== undefined && metadata.updatedAt < options.updatedAfter) {
          return false;
        }
        if (options.updatedBefore !== undefined && metadata.updatedAt > options.updatedBefore) {
          return false;
        }

        return true;
      });
    }

    const sortBy = options?.sortBy ?? "createdAt";
    const sortOrder = options?.sortOrder ?? "desc";

    ids.sort((a, b) => {
      const metaA = this.metadataIndex.get(a)?.metadata;
      const metaB = this.metadataIndex.get(b)?.metadata;

      let valueA: string | number;
      let valueB: string | number;

      switch (sortBy) {
        case "name":
          valueA = metaA?.name ?? "";
          valueB = metaB?.name ?? "";
          break;
        case "type":
          valueA = metaA?.type ?? "";
          valueB = metaB?.type ?? "";
          break;
        case "updatedAt":
          valueA = metaA?.updatedAt ?? 0;
          valueB = metaB?.updatedAt ?? 0;
          break;
        case "createdAt":
        default:
          valueA = metaA?.createdAt ?? 0;
          valueB = metaB?.createdAt ?? 0;
          break;
      }

      if (typeof valueA === "string") {
        return sortOrder === "asc" ? valueA.localeCompare(valueB as string) : (valueB as string).localeCompare(valueA);
      }
      return sortOrder === "asc" ? (valueA as number) - (valueB as number) : (valueB as number) - (valueA as number);
    });

    const offset = options?.offset ?? 0;
    const limit = options?.limit ?? ids.length;

    return ids.slice(offset, offset + limit);
  }
}
