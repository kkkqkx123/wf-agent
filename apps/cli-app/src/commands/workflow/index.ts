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
    .action(async (options: CommandOptions) => {
      try {
        const adapter = new WorkflowAdapter();
        const workflows = await adapter.listWorkflows();

        if (workflows.length === 0) {
          router.render(workflows, {
            type: "list",
            entity: "workflow",
            format: () => "No workflow found.",
          });
          return;
        }

        router.render(workflows, {
          type: "list",
          entity: "workflow",
          format: () => {
            if (options.table) {
              const headers = ["ID", "Name", "Status", "Creation time"];
              const rows = workflows.map(w => [
                w.id?.substring(0, 8) || "N/A",
                w.name || "Unnamed",
                w.status || "unknown",
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
    .action(async (id, options: CommandOptions) => {
      try {
        const adapter = new WorkflowAdapter();
        const workflow = await adapter.getWorkflow(id);

        router.render(workflow, {
          type: "detail",
          entity: "workflow",
          format: () => {
            if (options.verbose) {
              return getFormatter().json(workflow);
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

  return workflowCmd;
}
