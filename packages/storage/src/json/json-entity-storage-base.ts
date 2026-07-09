import * as path from "path";
import { BaseJsonStorage, BaseJsonStorageConfig } from "./base-json-storage.js";

export abstract class JsonEntityStorageBase<TMetadata extends object, TListOptions = Record<string, unknown>>
  extends BaseJsonStorage<TMetadata, TListOptions>
{
  protected abstract getEntityName(): string;

  constructor(config: BaseJsonStorageConfig) {
    super(config);
  }

  protected override getMetadataDir(): string {
    return path.join(this.config.baseDir, "metadata", this.getEntityName());
  }

  protected override getDataDir(): string {
    return path.join(this.config.baseDir, "data", this.getEntityName());
  }
}
