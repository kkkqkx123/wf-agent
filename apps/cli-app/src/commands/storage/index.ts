/**
 * Storage Commands
 * Manage and diagnose storage system
 */

import { Command } from "commander";
import { StorageDiagnosticsAdapter } from "../../adapters/storage-diagnostics-adapter.js";
import { getRouter } from "../../utils/output-router.js";
import { handleError } from "../../utils/error-handler.js";
import {
  formatStorageDiagnosticsReport,
  formatStorageHealth,
  formatStorageItemCounts,
} from "../../utils/formatters/storage-formatters.js";

export function createStorageCommands(): Command {
  const storage = new Command("storage").description("Storage system management and diagnostics");

  storage
    .command("diagnose")
    .description("Run comprehensive storage diagnostics")
    .option("-d, --detailed", "Show detailed information")
    .action(async () => {
      try {
        const adapter = new StorageDiagnosticsAdapter();
        const report = await adapter.diagnose();

        getRouter().render(report, {
          type: "detail",
          entity: "storage_diagnostics",
          format: () => formatStorageDiagnosticsReport(report),
          message: "Storage diagnostics completed",
        });
      } catch (error) {
        handleError(error, {
          operation: "storage-diagnose",
          additionalInfo: { command: "storage diagnose" },
        });
      }
    });

  storage
    .command("health")
    .description("Quick storage health status")
    .action(async () => {
      try {
        const adapter = new StorageDiagnosticsAdapter();
        const health = await adapter.getHealth();

        getRouter().render(health, {
          type: "detail",
          entity: "storage_health",
          format: () => formatStorageHealth(health),
          message: "Storage health check completed",
        });
      } catch (error) {
        handleError(error, {
          operation: "storage-health",
          additionalInfo: { command: "storage health" },
        });
      }
    });

  storage
    .command("stats")
    .description("Get storage statistics")
    .option("-t, --by-type", "Group by resource type")
    .action(async () => {
      try {
        const adapter = new StorageDiagnosticsAdapter();
        const counts = await adapter.getItemCounts();

        getRouter().render(counts, {
          type: "detail",
          entity: "storage_stats",
          format: () => formatStorageItemCounts(counts),
          message: "Storage statistics retrieved",
        });
      } catch (error) {
        handleError(error, {
          operation: "storage-stats",
          additionalInfo: { command: "storage stats" },
        });
      }
    });

  return storage;
}
