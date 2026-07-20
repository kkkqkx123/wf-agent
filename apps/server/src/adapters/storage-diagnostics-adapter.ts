/**
 * Storage Diagnostics Adapter
 * Monitor storage health and diagnostics.
 */

import { BaseAdapter } from "./base-adapter.js";

export class StorageDiagnosticsAdapter extends BaseAdapter {
  override getResourceName(): string {
    return "StorageDiagnostics";
  }

  async diagnose(): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("diagnose");
      const api = this.sdk.getFactory().createStorageDiagnosticsAPI();
      const report = await api.getDiagnosticsReport();
      return report as any;
    }, "Diagnose storage");
  }

  async getHealth(): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getHealth");
      const api = this.sdk.getFactory().createStorageDiagnosticsAPI();
      const health = await api.getAdapterHealth();
      return health as any;
    }, "Get storage health");
  }

  async getItemCounts(): Promise<Record<string, any>> {
    return this.executeWithErrorHandling(async () => {
      this.logOperation("getItemCounts");
      const api = this.sdk.getFactory().createStorageDiagnosticsAPI();
      const counts = await api.getItemCounts();
      return counts as any;
    }, "Get storage item counts");
  }
}