/**
 * Storage Adapter Base Class
 *
 * Provides shared infrastructure (metrics, initialization guard, batch templates)
 * for all storage backend implementations.
 *
 * @template TMetadata - Metadata type
 * @template TListOptions - List query option type
 * @template TSaveOptions - Save operation options type (optional)
 */
import type { BaseStorageAdapter } from "./base-storage-adapter.js";
import type { StorageMetrics } from "../metrics.js";
import { DEFAULT_STORAGE_METRICS } from "../metrics.js";
import { StorageError } from "../storage-errors.js";

export abstract class StorageAdapterBase<TMetadata, TListOptions = Record<string, unknown>, TSaveOptions = void>
  implements BaseStorageAdapter<TMetadata, TListOptions, TSaveOptions>
{
  protected initialized: boolean = false;
  protected metrics: StorageMetrics = { ...DEFAULT_STORAGE_METRICS };

  // ── Lifecycle ──────────────────────────────────────────────────────────
  abstract initialize(): Promise<void>;
  abstract close(): Promise<void>;
  abstract clear(): Promise<void>;

  // ── CRUD ───────────────────────────────────────────────────────────────
  abstract save(id: string, data: Uint8Array, metadata: TMetadata, options?: TSaveOptions): Promise<void>;
  abstract load(id: string): Promise<Uint8Array | null>;
  abstract delete(id: string): Promise<void>;
  abstract exists(id: string): Promise<boolean>;
  abstract list(options?: TListOptions): Promise<string[]>;
  abstract getMetadata(id: string): Promise<TMetadata | null>;

  // ── Metrics ────────────────────────────────────────────────────────────
  abstract getMetrics(): Promise<StorageMetrics>;

  resetMetrics(): void {
    this.metrics = { ...DEFAULT_STORAGE_METRICS };
  }

  protected updateMetric(operation: string, timeMs: number, dataSize?: number): void {
    const countKey = `${operation}Count` as keyof StorageMetrics;
    const timeKey = `avg${operation.charAt(0).toUpperCase()}${operation.slice(1)}Time` as keyof StorageMetrics;

    this.metrics[countKey] = (this.metrics[countKey] as number) + 1;

    const currentAvg = this.metrics[timeKey] as number;
    const count = this.metrics[countKey] as number;
    this.metrics[timeKey] = currentAvg + (timeMs - currentAvg) / count;

    if (dataSize !== undefined) {
      this.metrics.totalBlobSize += dataSize;
    }
  }

  // ── Guard ──────────────────────────────────────────────────────────────
  protected ensureInitialized(): void {
    if (!this.initialized) {
      throw new StorageError("Storage not initialized. Call initialize() first.", "initialize");
    }
  }

  // ── Batch operations (default implementations, override for optimization) ─
  async saveBatch(
    items: Array<{ id: string; data: Uint8Array; metadata: TMetadata }>,
  ): Promise<void> {
    for (const item of items) {
      await this.save(item.id, item.data, item.metadata);
    }
  }

  async loadBatch(ids: string[]): Promise<Array<{ id: string; data: Uint8Array | null }>> {
    return Promise.all(ids.map(async (id) => ({
      id,
      data: await this.load(id),
    })));
  }

  async deleteBatch(ids: string[]): Promise<void> {
    for (const id of ids) {
      await this.delete(id);
    }
  }
}
