/**
 * Template Command Group
 */

import { Command } from "commander";
import { TemplateAdapter } from "../../adapters/template-adapter.js";
import { getOutput } from "../../utils/output.js";
import { formatWorkflow, formatWorkflowList } from "../../utils/cli-formatters.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";

const output = getOutput();

/**
 * Create Template Command Group
 */
export function createTemplateCommands(): Command {
  const templateCmd = new Command("template").description("Manage Templates");

  // Register node template command
  templateCmd
    .command("register-node <file>")
    .description("Register node template from file")
    .option("-v, --verbose", "Detailed output")
    .action(async (file, options: CommandOptions) => {
      try {
        output.infoLog(`Registering node template from file: ${file}`);

        const adapter = new TemplateAdapter();
        const template = await adapter.registerNodeTemplateFromFile(file);

        output.output(formatWorkflow(template, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "registerNodeTemplateFromFile",
          additionalInfo: { file },
        });
      }
    });

  // Batch node registration template command
  templateCmd
    .command("register-nodes-batch <directory>")
    .description("Batch register node templates from directory")
    .option("-r, --recursive", "Recursive loading of subdirectories")
    .option("-p, --pattern <pattern>", "File pattern (regular expression)")
    .action(
      async (
        directory,
        options: {
          recursive?: boolean;
          pattern?: string;
        },
      ) => {
        try {
          output.infoLog(`Batch registering node templates from directory: ${directory}`);

          // Parse file mode
          const filePattern = options.pattern ? new RegExp(options.pattern) : undefined;

          const adapter = new TemplateAdapter();
          const result = await adapter.registerNodeTemplatesFromDirectory({
            configDir: directory,
            recursive: options.recursive,
            filePattern,
          });

          // Display the results
          output.output(`Success: ${result.success.length} node templates registered`);
          if (result.failures.length > 0) {
            output.output(`Failed: ${result.failures.length} node templates`);
            result.failures.forEach(failure => {
              output.output(`  - ${failure.filePath}: ${failure.error}`);
            });
          }
        } catch (error) {
          handleError(error, {
            operation: "registerNodeTemplatesFromDirectory",
            additionalInfo: { directory, recursive: options.recursive, pattern: options.pattern },
          });
        }
      },
    );

  // Register trigger template command
  templateCmd
    .command("register-trigger <file>")
    .description("Register trigger template from file")
    .option("-v, --verbose", "Detailed output")
    .action(async (file, options: CommandOptions) => {
      try {
        output.infoLog(`Registering trigger template from file: ${file}`);

        const adapter = new TemplateAdapter();
        const template = await adapter.registerTriggerTemplateFromFile(file);

        output.output(formatWorkflow(template, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "registerTriggerTemplateFromFile",
          additionalInfo: { file },
        });
      }
    });

  // Batch registration of trigger templates command
  templateCmd
    .command("register-triggers-batch <directory>")
    .description("Batch register trigger templates from directory")
    .option("-r, --recursive", "Recursive loading of subdirectories")
    .option("-p, --pattern <pattern>", "File pattern (regular expression)")
    .action(
      async (
        directory,
        options: {
          recursive?: boolean;
          pattern?: string;
        },
      ) => {
        try {
          output.infoLog(`Batch registering trigger templates from directory: ${directory}`);

          // Parse file mode
          const filePattern = options.pattern ? new RegExp(options.pattern) : undefined;

          const adapter = new TemplateAdapter();
          const result = await adapter.registerTriggerTemplatesFromDirectory({
            configDir: directory,
            recursive: options.recursive,
            filePattern,
          });

          // Display the results
          output.output(`Success: ${result.success.length} trigger templates registered`);
          if (result.failures.length > 0) {
            output.output(`Failed: ${result.failures.length} trigger templates`);
            result.failures.forEach(failure => {
              output.output(`  - ${failure.filePath}: ${failure.error}`);
            });
          }
        } catch (error) {
          handleError(error, {
            operation: "registerTriggerTemplatesFromDirectory",
            additionalInfo: { directory, recursive: options.recursive, pattern: options.pattern },
          });
        }
      },
    );

  // List node template commands
  templateCmd
    .command("list-nodes")
    .description("List all node templates")
    .option("-t, --table", "Output in table format")
    .option("-v, --verbose", "Detailed output")
    .action(async (options: CommandOptions) => {
      try {
        const adapter = new TemplateAdapter();
        const templates = await adapter.listNodeTemplates();

        output.output(formatWorkflowList(templates, { table: options.table }));
      } catch (error) {
        handleError(error, {
          operation: "listNodeTemplates",
        });
      }
    });

  // List trigger template commands
  templateCmd
    .command("list-triggers")
    .description("List all trigger templates")
    .option("-t, --table", "Output in table format")
    .option("-v, --verbose", "Detailed output")
    .action(async (options: CommandOptions) => {
      try {
        const adapter = new TemplateAdapter();
        const templates = await adapter.listTriggerTemplates();

        output.output(formatWorkflowList(templates, { table: options.table }));
      } catch (error) {
        handleError(error, {
          operation: "listTriggerTemplates",
        });
      }
    });

  // Command to view node template details
  templateCmd
    .command("show-node <id>")
    .description("View node template details")
    .option("-v, --verbose", "Detailed output")
    .action(async (id, options: CommandOptions) => {
      try {
        const adapter = new TemplateAdapter();
        const template = await adapter.getNodeTemplate(id);

        output.output(formatWorkflow(template, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "getNodeTemplate",
          additionalInfo: { id },
        });
      }
    });

  // View Trigger Template Details Command
  templateCmd
    .command("show-trigger <id>")
    .description("View trigger template details")
    .option("-v, --verbose", "Detailed output")
    .action(async (id, options: CommandOptions) => {
      try {
        const adapter = new TemplateAdapter();
        const template = await adapter.getTriggerTemplate(id);

        output.output(formatWorkflow(template, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "getTriggerTemplate",
          additionalInfo: { id },
        });
      }
    });

  // Delete Node Template Command
  templateCmd
    .command("delete-node <id>")
    .description("Delete the node template.")
    .option("-f, --force", "Forced deletion, without prompting for confirmation")
    .action(async (id, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete node template: ${id}`);
          // In practical applications, interactive confirmation can be added here.
          output.infoLog("Use the --force option to skip the confirmation.");
          return;
        }

        const adapter = new TemplateAdapter();
        await adapter.deleteNodeTemplate(id);
      } catch (error) {
        handleError(error, {
          operation: "deleteNodeTemplate",
          additionalInfo: { id },
        });
      }
    });

  // Delete the trigger template command.
  templateCmd
    .command("delete-trigger <id>")
    .description("Delete the trigger template")
    .option("-f, --force", "Force deletion without prompting for confirmation")
    .action(async (id, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete trigger template: ${id}`);
          // In practical applications, interactive confirmation can be added here.
          output.infoLog("Use the --force option to skip confirmation.");
          return;
        }

        const adapter = new TemplateAdapter();
        await adapter.deleteTriggerTemplate(id);
      } catch (error) {
        handleError(error, {
          operation: "deleteTriggerTemplate",
          additionalInfo: { id },
        });
      }
    });

  return templateCmd;
}
