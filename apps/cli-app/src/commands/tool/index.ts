/**
 * Tool Command Group
 */

import { Command } from "commander";
import { ToolAdapter } from "../../adapters/tool-adapter.js";
import { getOutput } from "../../utils/output.js";
import { formatTool, formatToolList } from "../../utils/cli-formatters.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";
import { CLIValidationError } from "../../types/cli-types.js";

const output = getOutput();

/**
 * Create Tool Command Group
 */
export function createToolCommands(): Command {
  const toolCmd = new Command("tool").description("Manage Tools");

  // Register tool command
  toolCmd
    .command("register <file>")
    .description("Register tool from file")
    .option("-v, --verbose", "Detailed output")
    .action(async (file, options: CommandOptions) => {
      try {
        output.infoLog(`Registering tool from file: ${file}`);

        const adapter = new ToolAdapter();
        const tool = await adapter.registerFromFile(file);

        output.output(formatTool(tool, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "registerTool",
          additionalInfo: { file },
        });
      }
    });

  // Batch registration tool command
  toolCmd
    .command("register-batch <directory>")
    .description("Batch register tools from directory")
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
          output.infoLog(`Batch registering tools from directory: ${directory}`);

          // Parse file mode
          const filePattern = options.pattern ? new RegExp(options.pattern) : undefined;

          const adapter = new ToolAdapter();
          const result = await adapter.registerFromDirectory({
            configDir: directory,
            recursive: options.recursive,
            filePattern,
          });

          // Display the results
          output.newLine();
          output.info(`Success: ${result.success.length} tools registered`);
          if (result.failures.length > 0) {
            output.newLine();
            output.fail(`Failed: ${result.failures.length} tools`);
            result.failures.forEach(failure => {
              output.output(`  - ${failure.filePath}: ${failure.error}`);
            });
          }
        } catch (error) {
          handleError(error, {
            operation: "registerToolsBatch",
            additionalInfo: { directory, recursive: options.recursive, pattern: options.pattern },
          });
        }
      },
    );

  // List tool commands
  toolCmd
    .command("list")
    .description("List all tools")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .action(async (options: CommandOptions) => {
      try {
        const adapter = new ToolAdapter();
        const tools = await adapter.listTools();

        output.output(formatToolList(tools, { table: options.table }));
      } catch (error) {
        handleError(error, {
          operation: "listTools",
        });
      }
    });

  // View tool details command
  toolCmd
    .command("show <id>")
    .description("View tool details")
    .option("-v, --verbose", "Detailed output")
    .action(async (id, options: CommandOptions) => {
      try {
        const adapter = new ToolAdapter();
        const tool = await adapter.getTool(id);

        output.output(formatTool(tool, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "getTool",
          additionalInfo: { id },
        });
      }
    });

  // Delete tool commands
  toolCmd
    .command("delete <id>")
    .description("Delete Tool")
    .option("-f, --force", "Forced deletion, without prompting for confirmation.")
    .action(async (id, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete tool: ${id}`);
          // In practical applications, interactive confirmation can be added here.
          output.infoLog("Use the --force option to skip the confirmation.");
          return;
        }

        const adapter = new ToolAdapter();
        await adapter.deleteTool(id);
      } catch (error) {
        handleError(error, {
          operation: "deleteTool",
          additionalInfo: { id },
        });
      }
    });

  // Update tool commands
  toolCmd
    .command("update <id>")
    .description("Update tool")
    .option("-n, --name <name>", "Tool Name")
    .option("-d, --description <description>", "Tool Description")
    .option("-v, --verbose", "Detailed output")
    .action(
      async (
        id,
        options: CommandOptions & {
          name?: string;
          description?: string;
        },
      ) => {
        const updates: any = {};
        try {
          const adapter = new ToolAdapter();

          if (options.name) updates.name = options.name;
          if (options.description) updates.description = options.description;

          const tool = await adapter.updateTool(id, updates);
          output.output(formatTool(tool, { verbose: options.verbose }));
        } catch (error) {
          handleError(error, {
            operation: "updateTool",
            additionalInfo: { id, updates },
          });
        }
      },
    );

  // Verification tool configuration command
  toolCmd
    .command("validate <file>")
    .description("Verify tool configuration file")
    .action(async file => {
      try {
        output.infoLog(`Validating tool configuration: ${file}`);

        const adapter = new ToolAdapter();
        const result = await adapter.validateTool(file);

        if (result.valid) {
          output.info("Configuration validation passed.");
        } else {
          output.fail("Configuration validation failed:");
          result.errors.forEach(err => {
            output.output(`  - ${err}`);
          });
          handleError(new CLIValidationError("Configuration validation failed."), {
            operation: "validateTool",
            additionalInfo: { file, errors: result.errors },
          });
          return;
        }
      } catch (error) {
        handleError(error, {
          operation: "validateTool",
          additionalInfo: { file },
        });
      }
    });

  // Execute tool commands
  toolCmd
    .command("execute <id>")
    .description("Execute Tool")
    .option("-p, --params <json>", "Parameters (JSON format)")
    .option("-t, --timeout <timeout>", "Timeout duration (in milliseconds)")
    .option("-r, --max-retries <maxRetries>", "Maximum number of retries")
    .option("-d, --retry-delay <retryDelay>", "Retry delay (in milliseconds)")
    .option("-l, --no-logging", "Disable logging")
    .option("-v, --verbose", "Detailed output")
    .action(
      async (
        id,
        options: CommandOptions & {
          params?: string;
          timeout?: string;
          maxRetries?: string;
          retryDelay?: string;
          logging?: boolean;
        },
      ) => {
        try {
          output.infoLog(`Executing tool: ${id}`);

          // Parse parameters
          let parameters: Record<string, any> = {};
          if (options.params) {
            try {
              parameters = JSON.parse(options.params);
            } catch (error) {
              handleError(new CLIValidationError("The parameters must be in valid JSON format."), {
                operation: "executeTool",
                additionalInfo: { id, params: options.params },
              });
              return;
            }
          }

          // Build execution options
          const toolOptions: any = {};
          if (options.timeout) toolOptions.timeout = parseInt(options.timeout, 10);
          if (options.maxRetries) toolOptions.maxRetries = parseInt(options.maxRetries, 10);
          if (options.retryDelay) toolOptions.retryDelay = parseInt(options.retryDelay, 10);
          if (options.logging !== undefined) toolOptions.enableLogging = options.logging;

          const adapter = new ToolAdapter();
          const result = await adapter.executeTool(id, parameters, toolOptions);

          if (options.verbose) {
            output.json(result);
          } else {
            output.info("The tool executed successfully.");
            if (result.result !== undefined) {
              output.output(`Result: ${JSON.stringify(result.result)}`);
            }
          }
        } catch (error) {
          handleError(error, {
            operation: "executeTool",
            additionalInfo: { id },
          });
        }
      },
    );

  // Verify tool parameters command
  toolCmd
    .command("validate-params <id>")
    .description("Verify tool parameters")
    .option("-p, --params <json>", "Parameters (JSON format)")
    .action(async (id, options: { params?: string }) => {
      try {
        output.infoLog(`Validating parameters for tool: ${id}`);

        // Parse parameters
        let parameters: Record<string, any> = {};
        if (options.params) {
          try {
            parameters = JSON.parse(options.params);
          } catch (error) {
            handleError(new CLIValidationError("The parameters must be in valid JSON format."), {
              operation: "validateToolParameters",
              additionalInfo: { id, params: options.params },
            });
            return;
          }
        }

        const adapter = new ToolAdapter();
        const result = await adapter.validateParameters(id, parameters);

        if (result.valid) {
          output.info("Parameter validation passed.");
        } else {
          output.fail("Parameter validation failed:");
          result.errors.forEach(err => {
            output.output(`  - ${err}`);
          });
          handleError(new CLIValidationError("Parameter validation failed."), {
            operation: "validateToolParameters",
            additionalInfo: { id, errors: result.errors },
          });
          return;
        }
      } catch (error) {
        handleError(error, {
          operation: "validateToolParameters",
          additionalInfo: { id },
        });
      }
    });

  return toolCmd;
}
