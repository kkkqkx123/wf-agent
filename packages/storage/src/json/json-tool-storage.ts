/**
 * JSON File Tool Storage Implementation
 * Tool persistent storage based on JSON file system with metadata-data separation
 */

import type { ToolStorageMetadata, ToolListOptions } from "@wf-agent/types";
import type { ToolStorageAdapter } from "../types/adapter/base-storage-adapter.js";
import { BaseJsonStorageConfig } from "./base-json-storage.js";
import { JsonEntityStorageBase } from "./json-entity-storage-base.js";

/**
 * JSON File Tool Storage
 * Implements the ToolStorageAdapter interface
 */
export class JsonToolStorage
  extends JsonEntityStorageBase<ToolStorageMetadata, ToolListOptions>
  implements ToolStorageAdapter
{
  constructor(config: BaseJsonStorageConfig) {
    super(config);
  }

  protected getEntityName(): string {
    return "tool";
  }

  override async list(options?: ToolListOptions): Promise<string[]> {
    this.ensureInitialized();

    let ids = this.getAllIds();

    if (options) {
      ids = ids.filter(id => {
        const entry = this.metadataIndex.get(id);
        if (!entry) return false;

        const metadata = entry.metadata;

        if (options.toolId && metadata.toolId !== options.toolId) {
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

        return true;
      });
    }

    const sortBy = options?.sortBy ?? "toolId";
    const sortOrder = options?.sortOrder ?? "asc";

    ids.sort((a, b) => {
      const metaA = this.metadataIndex.get(a)?.metadata;
      const metaB = this.metadataIndex.get(b)?.metadata;

      let valueA: string;
      let valueB: string;

      switch (sortBy) {
        case "type":
          valueA = metaA?.type ?? "";
          valueB = metaB?.type ?? "";
          break;
        case "category":
          valueA = metaA?.category ?? "";
          valueB = metaB?.category ?? "";
          break;
        case "toolId":
        default:
          valueA = metaA?.toolId ?? "";
          valueB = metaB?.toolId ?? "";
          break;
      }

      return sortOrder === "asc" ? valueA.localeCompare(valueB) : valueB.localeCompare(valueA);
    });

    const offset = options?.offset ?? 0;
    const limit = options?.limit ?? ids.length;

    return ids.slice(offset, offset + limit);
  }
}
