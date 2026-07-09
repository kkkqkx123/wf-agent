/**
 * JSON File Agent Profile Storage Implementation
 * Agent profile persistent storage based on JSON file system with metadata-data separation
 */

import type { AgentProfileStorageMetadata, AgentProfileListOptions } from "@wf-agent/types";
import type { AgentProfileStorageAdapter } from "../types/adapter/base-storage-adapter.js";
import { BaseJsonStorageConfig } from "./base-json-storage.js";
import { JsonEntityStorageBase } from "./json-entity-storage-base.js";

/**
 * JSON File Agent Profile Storage
 * Implements the AgentProfileStorageAdapter interface
 */
export class JsonAgentProfileStorage
  extends JsonEntityStorageBase<AgentProfileStorageMetadata, AgentProfileListOptions>
  implements AgentProfileStorageAdapter
{
  constructor(config: BaseJsonStorageConfig) {
    super(config);
  }

  protected getEntityName(): string {
    return "agentProfile";
  }

  override async list(options?: AgentProfileListOptions): Promise<string[]> {
    this.ensureInitialized();

    let ids = this.getAllIds();

    if (options) {
      ids = ids.filter(id => {
        const entry = this.metadataIndex.get(id);
        if (!entry) return false;

        const metadata = entry.metadata;

        if (options.profileId && metadata.profileId !== options.profileId) {
          return false;
        }

        if (options.nameContains && !metadata.name.toLowerCase().includes(options.nameContains.toLowerCase())) {
          return false;
        }

        if (options.descriptionContains && (!metadata.description || !metadata.description.toLowerCase().includes(options.descriptionContains.toLowerCase()))) {
          return false;
        }

        return true;
      });
    }

    const sortBy = options?.sortBy ?? "name";
    const sortOrder = options?.sortOrder ?? "asc";

    ids.sort((a, b) => {
      const metaA = this.metadataIndex.get(a)?.metadata;
      const metaB = this.metadataIndex.get(b)?.metadata;

      let valueA: string;
      let valueB: string;

      switch (sortBy) {
        case "profileId":
          valueA = metaA?.profileId ?? "";
          valueB = metaB?.profileId ?? "";
          break;
        case "name":
        default:
          valueA = metaA?.name ?? "";
          valueB = metaB?.name ?? "";
          break;
      }

      return sortOrder === "asc" ? valueA.localeCompare(valueB) : valueB.localeCompare(valueA);
    });

    const offset = options?.offset ?? 0;
    const limit = options?.limit ?? ids.length;

    return ids.slice(offset, offset + limit);
  }
}
