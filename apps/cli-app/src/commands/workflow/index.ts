/**
 * Workflow Command Group
 */

import { Command } from "commander";
import { WorkflowAdapter } from "../../adapters/workflow-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { getFormatter } from "../../utils/formatter.js";
import { formatWorkflow } from "../../utils/formatters/index.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const output = getOutput();
const router = getRouter();

/**
 * Extended options for workflow commands
 */
interface WorkflowCommandOptions extends CommandOptions {
  table?: boolean;
  json?: boolean;
  verbose?: boolean;
  force?: boolean;
  cascade?: boolean;
  type?: string;
  status?: string;
  tag?: string;
  versions?: boolean;
  params?: string;
  name?: string;
  description?: string;
  fromFile?: string;
  "from-file"?: string;
  toVersion?: string;
  "to-version"?: string;
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
    .action(async (file, options: WorkflowCommandOptions) => {
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
    .option("--json", "Output as JSON array")
    .option("--type <type>", "Filter by workflow type (STANDALONE, DEPENDENT, TRIGGERED_SUBWORKFLOW)")
    .option("--status <status>", "Filter by workflow status")
    .option("--tag <tag>", "Filter by tag")
    .action(async (options: WorkflowCommandOptions) => {
      try {
        // Build filter from options
        const filter: Record<string, unknown> = {};
        if (options.type) filter['type'] = options.type;
        if (options.status) filter['status'] = options.status;
        if (options.tag) filter['tag'] = options.tag;

        const adapter = new WorkflowAdapter();
        const workflows = await adapter.listWorkflows(
          Object.keys(filter).length > 0 ? filter : undefined,
        );

        if (workflows.length === 0) {
          router.render(workflows, {
            type: "list",
            entity: "workflow",
            format: () => "No workflow found.",
          });
          return;
        }

        // JSON output
        if (options.json) {
          router.render(workflows, {
            type: "list",
            entity: "workflow",
            format: () => getFormatter().json(workflows),
          });
          return;
        }

        // Check verbose flag: the program-level --verbose is consumed by the
        // parent command, so fall back to checking process.argv when the
        // subcommand's --verbose is not set.
        const isVerbose = options.verbose ?? process.argv.includes("--verbose");

        router.render(workflows, {
          type: "list",
          entity: "workflow",
          format: () => {
            if (options.table) {
              const headers = ["ID", "Name", "Type", "Version", "Status", "Creation time"];
              const rows = workflows.map(w => [
                w.id?.substring(0, 8) || "N/A",
                w.name || "Unnamed",
                w.type || "unknown",
                w.version || "",
                w.status || "unknown",
                w.createdAt || "N/A",
              ]);
              return getFormatter().table(headers, rows);
            }
            return workflows
              .map(w =>
                isVerbose
                  ? formatWorkflow(w, { verbose: true })
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

        if (options.versions) {
          // Show version history
          const versions = await adapter.listWorkflowVersions(id);
          const workflow = await adapter.getWorkflow(id);

          const mainOutput = options.json
            ? getFormatter().json(workflow)
            : formatWorkflow(workflow, { verbose: options.verbose });

          const versionText = versions
            .map(v => `  Version: ${v.version} (${v.createdAt})${v.description ? ` - ${v.description}` : ""}`)
            .join("\n");

          const outputText = options.json
            ? mainOutput
            : `${mainOutput}\n\nVersion History:\n${versionText}`;

          router.render(
            { workflow, versions },
            {
              type: "detail",
              entity: "workflow",
              format: () => outputText,
            },
          );
          return;
        }

        const workflow = await adapter.getWorkflow(id);

        if (options.json) {
          router.render(workflow, {
            type: "detail",
            entity: "workflow",
            format: () => getFormatter().json(workflow),
          });
          return;
        }

        router.render(workflow, {
          type: "detail",
          entity: "workflow",
          format: () => {
            if (options.verbose) {
              return formatWorkflow(workflow, { verbose: true });
            }
            return formatWorkflow(workflow);
          },
        });
      } catch (error) {
        handleError(error, {
          operation: "show-workflow",
          additionalInfo: { id },
        });
      }
    });

  // Update workflow command
  workflowCmd
    .command("update <id>")
    .description("Update workflow from file")
    .requiredOption("--from-file <file>", "Configuration file path")
    .option("-p, --params <params>", "Runtime parameters (in JSON format)")
    .action(async (id, options: WorkflowCommandOptions) => {
      try {
        const filePath = options.fromFile || options["from-file"];
        if (!filePath) {
          throw new Error("--from-file <file> is required");
        }

        const parameters = options.params ? JSON.parse(options.params) : undefined;

        const adapter = new WorkflowAdapter();
        const workflow = await adapter.updateWorkflow(id, filePath, parameters);

        router.render(
          { id, workflow },
          {
            type: "action",
            entity: "workflow",
            message: `Workflow updated: ${id}`,
            format: () => formatWorkflow(workflow),
          },
        );
      } catch (error) {
        handleError(error, {
          operation: "update-workflow",
          additionalInfo: { id, fromFile: options.fromFile || options["from-file"] },
        });
      }
    });

  // Clone workflow command
  workflowCmd
    .command("clone <source-id> <target-id>")
    .description("Clone a workflow")
    .option("--name <name>", "New workflow name")
    .option("--description <desc>", "New workflow description")
    .action(async (sourceId, targetId, options: WorkflowCommandOptions) => {
      try {
        const adapter = new WorkflowAdapter();
        const workflow = await adapter.cloneWorkflow(sourceId, targetId, {
          name: options.name,
          description: options.description,
        });

        router.render(
          { id: targetId, workflow },
          {
            type: "action",
            entity: "workflow",
            message: `Workflow cloned: ${sourceId} -> ${targetId}`,
            format: () => `Workflow cloned: ${sourceId} -> ${targetId}`,
          },
        );
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
    .requiredOption("--to-version <version>", "Target version to rollback to")
    .option("--confirm", "Confirm the rollback operation")
    .action(async (id, options: WorkflowCommandOptions) => {
      try {
        const toVersion = options.toVersion || options["to-version"];
        if (!toVersion) {
          throw new Error("--to-version <version> is required");
        }

        const adapter = new WorkflowAdapter();
        const result = await adapter.rollbackWorkflow(id, toVersion, !!options.confirm);

        if (!result) {
          // No --confirm flag: show warning
          router.render(
            { id, toVersion },
            {
              type: "action",
              entity: "workflow",
              message: `Use --confirm to rollback workflow ${id} to version ${toVersion}`,
              format: () => `Warning: This will rollback workflow '${id}' to version ${toVersion}. Use --confirm to proceed.`,
            },
          );
          return;
        }

        router.render(
          { id, workflow: result },
          {
            type: "action",
            entity: "workflow",
            message: `Workflow rolled back: ${id}`,
            format: () => `Workflow '${id}' rolled back to version ${toVersion}.`,
          },
        );
      } catch (error) {
        handleError(error, {
          operation: "rollback-workflow",
          additionalInfo: { id, toVersion: options.toVersion || options["to-version"] },
        });
      }
    });

  // Delete the workflow command
  workflowCmd
    .command("delete <id>")
    .description("Delete the workflow")
    .option("-f, --force", "Forced deletion, without prompting for confirmation.")
    .option("--cascade", "Cascade delete dependent workflows")
    .action(async (id, options: { force?: boolean; cascade?: boolean }) => {
      try {
        const adapter = new WorkflowAdapter();

        // Check for dependent workflows first
        const dependents = await adapter.findDependentWorkflows(id);

        if (dependents.length > 0) {
          if (!options.force && !options.cascade) {
            // Refuse with dependency info
            throw new Error(
              `Cannot be deleted. Workflow '${id}' is referenced by: ${dependents.join(", ")}. Use --cascade or --force`,
            );
          }

          if (options.force && !options.cascade) {
            // Force without cascade: refuse with cascade suggestion
            throw new Error(
              `Cannot be deleted. Workflow '${id}' is referenced by: ${dependents.join(", ")}. Cascade deletion suggestion: Use --cascade`,
            );
          }
        }

        if (!options.force && !options.cascade) {
          // Without force: refuse
          throw new Error(`Use --force to delete workflow: ${id}`);
        }

        // Proceed with delete (cascade or single)
        await adapter.deleteWorkflow(id, { force: options.force, cascade: options.cascade });

        router.render(
          { id },
          {
            type: "action",
            entity: "workflow",
            message: `Workflow is deleted: ${id}`,
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

  return workflowCmd;
}