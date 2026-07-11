/**
 * Storage Diagnostics Adapter
 * Wraps StorageDiagnosticsAPI for CLI usage
 */

import { BaseAdapter } from "./base-adapter.js";
import type {
  StorageDiagnosticsReport,
  StorageItemCounts,
  StorageAdapterHealth,
} from "@wf-agent/sdk/api";

export class StorageDiagnosticsAdapter extends BaseAdapter {
  constructor() {
    super();
  }

  /**
   * Get full diagnostics report
   */
  async diagnose(): Promise<StorageDiagnosticsReport> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.getFactory().createStorageDiagnosticsAPI();
      const report = await api.getDiagnosticsReport();

      this.logOperation("Storage diagnostics completed");
      return report;
    }, "Generate storage diagnostics");
  }

  /**
   * Get quick health status
   */
  async getHealth(): Promise<{
    status: string;
    adaptersConfigured: number;
    adapterDetails: StorageAdapterHealth[];
  }> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.getFactory().createStorageDiagnosticsAPI();
      const report = await api.getDiagnosticsReport();

      return {
        status: report.overallStatus,
        adaptersConfigured: report.adapterHealth.filter((a: any) => a.configured).length,
        adapterDetails: report.adapterHealth,
      };
    }, "Get storage health status");
  }

  /**
   * Get storage item counts
   */
  async getItemCounts(): Promise<StorageItemCounts> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.getFactory().createStorageDiagnosticsAPI();
      const counts = await api.getItemCounts();

      return counts;
    }, "Get storage item counts");
  }

  /**
   * Get adapter-specific health details
   */
  async getAdapterHealth(adapterName?: string): Promise<StorageAdapterHealth[]> {
    return this.executeWithErrorHandling(async () => {
      const api = this.sdk.getFactory().createStorageDiagnosticsAPI();
      const report = await api.getDiagnosticsReport();

      if (adapterName) {
        return report.adapterHealth.filter(a => a.name.includes(adapterName));
      }

      return report.adapterHealth;
    }, "Get adapter health details");
  }
}
