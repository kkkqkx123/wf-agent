/**
 * Workflow Command Group
 */

import { Command } from "commander";
import { WorkflowAdapter } from "../../adapters/workflow-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { getFormatter } from "../../utils/formatter.js";
import { formatWorkflow } from "../../utils/cli-formatters.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const output = getOutput();
const router = getRouter();

/**
 * Extended options for workflow commands
 */
interface WorkflowCommandOptions extends CommandOptions {
  type?: string;
  status?: string;
  tag?: string;
  json?: boolean;
  versions?: boolean;
  force?: boolean;
  fromFile?: string;
  params?: string;
  keepVersion?: boolean;
  name?: string;
  description?: string;
  toVersion?: string;
  confirm?: boolean;
}

/**
 * Create Workflow Command Group
 */
export function createWorkflowCommands(): Command {
  const workflowCmd = new Command("workflow").description("Manage workflows").alias("wf");

  // Register workflow command
  workflowCmd
    .command("register <file>")
    .description("Register workflow from file")
    .option("-v, --verbose", "Detailed output")
    .option("-p, --params <params>", "Runtime parameters (in JSON format)")
    .action(async (file, options: CommandOptions) => {
      try {
        output.infoLog(`Registering workflow from file: ${file}`);

        // Parse parameters
        const parameters = options.params ? JSON.parse(options.params) : undefined;

        const adapter = new WorkflowAdapter();
        const workflow = await adapter.registerFromFile(file, parameters);

        router.render(workflow, {
          type: "detail",
          entity: "workflow",
          format: () => formatWorkflow(workflow, { verbose: options.verbose }),
          message: `Workflow registered successfully: ${workflow.id}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "register-workflow",
          filePath: file,
          additionalInfo: { params: options.params },
        });
      }
    });

  // Batch registration workflow command
  workflowCmd
    .command("register-batch <directory>")
    .description("Batch register workflows from directory")
    .option("-r, --recursive", "Recursive loading of subdirectories")
    .option("-p, --pattern <pattern>", "File pattern (regular expression)")
    .option("--params <params>", "Runtime parameters (in JSON format)")
    .action(
      async (
        directory,
        options: {
          recursive?: boolean;
          pattern?: string;
          params?: string;
        },
      ) => {
        try {
          output.infoLog(`Batch registering workflows from directory: ${directory}`);

          // Parse parameters
          const parameters = options.params ? JSON.parse(options.params) : undefined;

          // Parse file mode
          const filePattern = options.pattern ? new RegExp(options.pattern) : undefined;

          const adapter = new WorkflowAdapter();
          const result = await adapter.registerFromDirectory({
            configDir: directory,
            recursive: options.recursive,
            filePattern,
            parameters,
          });

          // Display the results
          const successCount = result.success.length;
          const failureCount = result.failures.length;
          const batchMessage =
            failureCount > 0
              ? `Batch registration completed. Success: ${successCount}, Failed: ${failureCount}`
              : `Batch registration completed. Success: ${successCount}`;

          router.render(result, {
            type: "action",
            entity: "workflow",
            message: batchMessage,
            metadata: { successCount, failureCount },
            format: () => {
              let text = `Batch registration completed.\n`;
              text += `Success: ${successCount} instances\n`;
              if (failureCount > 0) {
                text += `Failed: ${failureCount} times\n`;
                result.failures.forEach(failure => {
                  text += `  - ${failure.filePath}: ${failure.error}\n`;
                });
              } else {
                text += `Failed: 0 times\n`;
              }
              return text;
            },
          });
        } catch (error) {
          handleError(error, {
            operation: "register-workflow-batch",
            additionalInfo: { directory, recursive: options.recursive, pattern: options.pattern },
          });
        }
      },
    );

  // List the workflow commands
  workflowCmd
    .command("list")
    .description("List all workflows")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .option("--type <type>", "Filter by workflow type")
    .option("--status <status>", "Filter by status")
    .option("--tag <tag>", "Filter by tag")
    .option("--json", "Output as JSON array")
    .action(async (options: WorkflowCommandOptions) => {
      try {
        const adapter = new WorkflowAdapter();
        const filter: Record<string, unknown> = {};
        if (options.tag) filter["tags"] = [options.tag];

        let workflows = await adapter.listWorkflows(
          Object.keys(filter).length > 0 ? filter : undefined,
        );

        // Apply CLI-side filters for type and status (not supported by SDK filter)
        if (options.type) {
          workflows = workflows.filter(w => w.type === options.type);
        }
        if (options.status) {
          workflows = workflows.filter(w => w.status === options.status);
        }

        if (workflows.length === 0) {
          router.render(workflows, {
            type: "list",
            entity: "workflow",
            format: () => "No workflow found.",
          });
          return;
        }

        // JSON output mode
        if (options.json) {
          output.output(JSON.stringify(workflows, null, 2));
          return;
        }

        router.render(workflows, {
          type: "list",
          entity: "workflow",
          format: () => {
            if (options.table) {
              const headers = ["ID", "Name", "Type", "Status", "Version", "Creation time"];
              const rows = workflows.map(w => [
                w.id?.substring(0, 8) || "N/A",
                w.name || "Unnamed",
                w.type || "unknown",
                w.status || "unknown",
                w.version || "N/A",
                w.createdAt || "N/A",
              ]);
              return getFormatter().table(headers, rows);
            }
            return workflows
              .map(w =>
                options.verbose
                  ? getFormatter().json(w)
                  : formatWorkflow(w),
              )
              .join("\n");
          },
          metadata: { total: workflows.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "list-workflows",
        });
      }
    });

  // View workflow details command
  workflowCmd
    .command("show <id>")
    .description("View workflow details")
    .option("-v, --verbose", "Detailed output")
    .option("--json", "Output as JSON")
    .option("--versions", "Show version history")
    .action(async (id, options: WorkflowCommandOptions) => {
      try {
        const adapter = new WorkflowAdapter();
        const workflow = await adapter.getWorkflow(id);

        // If --versions, also fetch and display version history
        let versionsOutput = "";
        if (options.versions) {
          try {
            const versions = await adapter.listWorkflowVersions(id);
            if (versions.length > 0) {
              versionsOutput = "\n\nVersion History:\n" + versions
                .map(v => `  ${v.version} - ${v.createdAt}${v.description ? ` - ${v.description}` : ""}`)
                .join("\n");
            }
          } catch {
            versionsOutput = "\n\n(Version history not available)";
          }
        }

        router.render(workflow, {
          type: "detail",
          entity: "workflow",
          format: () => {
            if (options.json || options.verbose) {
              return getFormatter().json(workflow) + versionsOutput;
            }
            return formatWorkflow(workflow) + versionsOutput;
          },
        });
      } catch (error) {
        handleError(error, {
          operation: "show-workflow",
          additionalInfo: { id },
        });
      }
    });

  // Delete the workflow command
  workflowCmd
    .command("delete <id>")
    .description("Delete the workflow")
    .option("-f, --force", "Forced deletion, without prompting for confirmation.")
    .action(async (id, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          router.render(
            { id },
            {
              type: "action",
              entity: "workflow",
              message: `Use --force to delete workflow: ${id}`,
              format: () => `Use the --force option to delete workflow: ${id}`,
            },
          );
          output.infoLog("Use the --force option to skip the confirmation.");
          return;
        }

        const adapter = new WorkflowAdapter();
        await adapter.deleteWorkflow(id);

        router.render(
          { id },
          {
            type: "action",
            entity: "workflow",
            message: `Workflow deleted: ${id}`,
            format: () => `Workflow deleted successfully: ${id}`,
          },
        );
      } catch (error) {
        handleError(error, {
          operation: "delete-workflow",
          additionalInfo: { id },
        });
      }
    });

  // Update workflow command
  workflowCmd
    .command("update <id>")
    .description("Update an existing workflow from a file")
    .requiredOption("--from-file <file>", "Workflow configuration file")
    .option("-p, --params <params>", "Runtime parameters (JSON format)")
    .option("-k, --keep-version", "Keep the current version number (do not auto-increment)")
    .action(async (id, options: WorkflowCommandOptions) => {
      try {
        const adapter = new WorkflowAdapter();
        const parameters = options.params ? JSON.parse(options.params) : undefined;

        await adapter.updateWorkflow(id, options.fromFile!, parameters);

        router.render(
          { id },
          {
            type: "action",
            entity: "workflow",
            message: `Workflow updated: ${id}`,
            format: () => `Workflow updated successfully: ${id}`,
          },
        );
      } catch (error) {
        handleError(error, {
          operation: "update-workflow",
          additionalInfo: { id, fromFile: options.fromFile },
        });
      }
    });

  // Clone workflow command
  workflowCmd
    .command("clone <source-id> <target-id>")
    .description("Clone an existing workflow with a new ID")
    .option("-n, --name <name>", "New workflow name")
    .option("-d, --description <desc>", "New description")
    .action(async (sourceId, targetId, options: WorkflowCommandOptions) => {
      try {
        const adapter = new WorkflowAdapter();
        const cloned = await adapter.cloneWorkflow(sourceId, targetId, {
          name: options.name,
          description: options.description,
        });

        router.render(cloned, {
          type: "detail",
          entity: "workflow",
          format: () => formatWorkflow(cloned),
          message: `Workflow cloned: ${sourceId} -> ${targetId}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "clone-workflow",
          additionalInfo: { sourceId, targetId },
        });
      }
    });

  // Rollback workflow command
  workflowCmd
    .command("rollback <id>")
    .description("Rollback workflow to a previous version")
    .requiredOption("--to-version <v>", "Target version to rollback to")
    .option("--confirm", "Skip confirmation prompt")
    .action(async (id, options: WorkflowCommandOptions) => {
      try {
        if (!options.confirm) {
          output.warn(`This will rollback workflow '${id}' to version ${options.toVersion}.`);
          output.warn("Use --confirm to proceed without prompt.");
          output.info("Note: Interactive confirmation is not yet implemented in headless mode.");
          output.info(`To proceed, run: workflow rollback ${id} --to-version ${options.toVersion} --confirm`);
          return;
        }

        const adapter = new WorkflowAdapter();
        const rolledBack = await adapter.rollbackWorkflow(id, options.toVersion!);

        router.render(rolledBack, {
          type: "detail",
          entity: "workflow",
          format: () => formatWorkflow(rolledBack),
          message: `Workflow rolled back: ${id} to version ${options.toVersion}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "rollback-workflow",
          additionalInfo: { id, version: options.toVersion },
        });
      }
    });

  return workflowCmd;
}
