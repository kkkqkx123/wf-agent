/**
 * Workflow Version Management Command
 * Manage and compare workflow versions
 */

import { Command } from "commander";
import { WorkflowVersionAdapter } from "../../adapters/workflow-version-adapter.js";
import {
  formatVersionList,
  formatVersionDiff,
  formatVersionDetails,
  formatChangeLog,
  formatVersionTable,
} from "../../utils/formatters/version-formatters.js";
import { getRouter } from "../../utils/output-router.js";
import type { ID } from "@wf-agent/types";

export function createWorkflowVersionCommand(): Command {
  const cmd = new Command("version");
  cmd.description("Manage workflow versions");

  cmd
    .command("list <workflowId>")
    .description("List all versions of a workflow")
    .option("--json", "Output as JSON")
    .option("--table", "Output as table")
    .action(async (workflowId: string, options) => {
      const adapter = new WorkflowVersionAdapter();
      const versions = await adapter.listVersions(workflowId as ID);

      const router = getRouter();
      let formatFn: () => string;
      if (options.json) {
        formatFn = () => JSON.stringify(versions, null, 2);
      } else if (options.table) {
        formatFn = () => formatVersionTable(versions);
      } else {
        formatFn = () => formatVersionList(versions);
      }

      router.render(versions, {
        type: "list",
        entity: "workflow_version",
        format: formatFn,
      });
    });

  cmd
    .command("show <workflowId>")
    .option("--version <v>", "Specific version to show")
    .option("--json", "Output as JSON")
    .description("Show version details")
    .action(async (workflowId: string, options) => {
      const adapter = new WorkflowVersionAdapter();

      const router = getRouter();

      if (options.version) {
        const version = await adapter.getVersion(
          workflowId as ID,
          options.version
        );
        let formatFn: () => string;
        if (options.json) {
          formatFn = () => JSON.stringify(version, null, 2);
        } else {
          formatFn = () => formatVersionDetails(version);
        }

        router.render(version, {
          type: "detail",
          entity: "workflow_version",
          format: formatFn,
        });
      } else {
        const versions = await adapter.listVersions(workflowId as ID);
        if (versions.length > 0) {
          const firstVersion = versions[0];
          if (firstVersion) {
            let formatFn: () => string;
            if (options.json) {
              formatFn = () => JSON.stringify(firstVersion, null, 2);
            } else {
              formatFn = () => formatVersionDetails(firstVersion);
            }

            router.render(firstVersion, {
              type: "detail",
              entity: "workflow_version",
              format: formatFn,
            });
          } else {
            router.render(null, {
              type: "detail",
              entity: "workflow_version",
              format: () => "No versions found",
            });
          }
        } else {
          router.render(null, {
            type: "detail",
            entity: "workflow_version",
            format: () => "No versions found",
          });
        }
      }
    });

  cmd
    .command("diff <workflowId>")
    .requiredOption(
      "--from <v1>",
      "Source version to compare from"
    )
    .requiredOption(
      "--to <v2>",
      "Target version to compare to"
    )
    .option("--json", "Output as JSON")
    .description("Compare two versions")
    .action(async (workflowId: string, options) => {
      const adapter = new WorkflowVersionAdapter();
      const diff = await adapter.compareVersions(
        workflowId as ID,
        options.from,
        options.to
      );

      const router = getRouter();
      let formatFn: () => string;
      if (options.json) {
        formatFn = () => JSON.stringify(diff, null, 2);
      } else {
        formatFn = () => formatVersionDiff(diff);
      }

      router.render(diff, {
        type: "detail",
        entity: "version_diff",
        format: formatFn,
      });
    });

  cmd
    .command("changelog <workflowId>")
    .option("--json", "Output as JSON")
    .description("Show version changelog")
    .action(async (workflowId: string, options) => {
      const adapter = new WorkflowVersionAdapter();
      const changelog = await adapter.getChangeLog(workflowId as ID);

      const router = getRouter();
      let formatFn: () => string;
      if (options.json) {
        formatFn = () => JSON.stringify(changelog, null, 2);
      } else {
        formatFn = () => formatChangeLog(changelog);
      }

      router.render(changelog, {
        type: "list",
        entity: "changelog",
        format: formatFn,
      });
    });

  cmd
    .command("detailed-diff <workflowId>")
    .requiredOption(
      "--from <v1>",
      "Source version to compare from"
    )
    .requiredOption(
      "--to <v2>",
      "Target version to compare to"
    )
    .option("--json", "Output as JSON")
    .description("Get detailed diff between versions")
    .action(async (workflowId: string, options) => {
      const adapter = new WorkflowVersionAdapter();
      const result = await adapter.getDiff(
        workflowId as ID,
        options.from,
        options.to
      );

      const router = getRouter();
      let formatFn: () => string;
      if (options.json) {
        formatFn = () => JSON.stringify(result, null, 2);
      } else {
        formatFn = () => {
          const lines = [
            formatVersionDiff(result.summary),
            "[METADATA] Details",
            "",
            JSON.stringify(result.details, null, 2),
          ];
          return lines.join("\n");
        };
      }

      router.render(result, {
        type: "detail",
        entity: "detailed_diff",
        format: formatFn,
      });
    });

  return cmd;
}
