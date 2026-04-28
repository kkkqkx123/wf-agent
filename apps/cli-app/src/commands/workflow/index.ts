/**
 * Workflow Command Group
 */

import { Command } from "commander";
import { WorkflowAdapter } from "../../adapters/workflow-adapter.js";
import { getOutput } from "../../utils/output.js";
import { formatWorkflow, formatWorkflowList } from "../../utils/cli-formatters.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const output = getOutput();

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

        output.output(`Workflow registered successfully: ${workflow.id}`);
        output.output(formatWorkflow(workflow, { verbose: options.verbose }));
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
          output.output(`Batch registration completed.`);
          output.output(`Success: ${result.success.length} instances`);
          if (result.failures.length > 0) {
            output.output(`Failed: ${result.failures.length} times`);
            result.failures.forEach(failure => {
              output.output(`  - ${failure.filePath}: ${failure.error}`);
            });
          } else {
            output.output(`Failed: 0 times`);
          }
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
          output.output("No workflow found.");
          return;
        }

        if (options.table) {
          const headers = ["ID", "Name", "Status", "Creation time"];
          const rows = workflows.map(w => [
            w.id?.substring(0, 8) || "N/A",
            w.name || "Unnamed",
            w.status || "unknown",
            w.createdAt || "N/A",
          ]);
          output.table(headers, rows);
        } else {
          workflows.forEach(w => {
            if (options.verbose) {
              output.json(w);
            } else {
              output.output(output.workflow(w));
            }
          });
        }
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

        if (options.verbose) {
          output.json(workflow);
        } else {
          output.output(output.workflow(workflow));
        }
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
          output.warnLog(`About to delete workflow: ${id}`);
          // In practical applications, interactive confirmation can be added here.
          output.infoLog("Use the --force option to skip the confirmation.");
          return;
        }

        const adapter = new WorkflowAdapter();
        await adapter.deleteWorkflow(id);
      } catch (error) {
        handleError(error, {
          operation: "delete-workflow",
          additionalInfo: { id },
        });
      }
    });

  return workflowCmd;
}
