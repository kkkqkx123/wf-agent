/**
 * Script Command Group
 */

import { Command } from "commander";
import { ScriptAdapter } from "../../adapters/script-adapter.js";
import { getOutput } from "../../utils/output.js";
import { formatScript, formatScriptList } from "../../utils/cli-formatters.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";
import { CLIValidationError } from "../../types/cli-types.js";

const output = getOutput();

/**
 * Create Script Command Group
 */
export function createScriptCommands(): Command {
  const scriptCmd = new Command("script").description("Manage Scripts");

  // Register Script Command
  scriptCmd
    .command("register <file>")
    .description("Register script from file")
    .option("-v, --verbose", "Detailed output")
    .action(async (file, options: CommandOptions) => {
      try {
        output.infoLog(`Registering script from file: ${file}`);

        const adapter = new ScriptAdapter();
        const script = await adapter.registerFromFile(file);

        output.output(formatScript(script, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "registerScript",
          additionalInfo: { file },
        });
      }
    });

  // Batch Registration Script Command
  scriptCmd
    .command("register-batch <directory>")
    .description("Batch register scripts from directory")
    .option("-r, --recursive", "Recursively loading subdirectories")
    .option("-p, --pattern <pattern>", "File patterns (regular expressions)")
    .action(
      async (
        directory,
        options: {
          recursive?: boolean;
          pattern?: string;
        },
      ) => {
        try {
          output.infoLog(`Batch registering scripts from directory: ${directory}`);

          // Parsing file patterns
          const filePattern = options.pattern ? new RegExp(options.pattern) : undefined;

          const adapter = new ScriptAdapter();
          const result = await adapter.registerFromDirectory({
            configDir: directory,
            recursive: options.recursive,
            filePattern,
          });

          // Show results
          output.output(`Success: ${result.success.length} scripts registered`);
          if (result.failures.length > 0) {
            output.output(`Failed: ${result.failures.length} scripts`);
            result.failures.forEach(failure => {
              output.output(`  - ${failure.filePath}: ${failure.error}`);
            });
          }
        } catch (error) {
          handleError(error, {
            operation: "registerScriptsBatch",
            additionalInfo: { directory, recursive: options.recursive, pattern: options.pattern },
          });
        }
      },
    );

  // List script commands
  scriptCmd
    .command("list")
    .description("List all scripts")
    .option("-t, --table", "Output in tabular format")
    .option("-v, --verbose", "Detailed output")
    .action(async (options: CommandOptions) => {
      try {
        const adapter = new ScriptAdapter();
        const scripts = await adapter.listScripts();

        output.output(formatScriptList(scripts, { table: options.table }));
      } catch (error) {
        handleError(error, {
          operation: "listScripts",
        });
      }
    });

  // View Script Details command
  scriptCmd
    .command("show <id>")
    .description("View script details")
    .option("-v, --verbose", "Detailed output")
    .action(async (id, options: CommandOptions) => {
      try {
        const adapter = new ScriptAdapter();
        const script = await adapter.getScript(id);

        output.output(formatScript(script, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "getScript",
          additionalInfo: { id },
        });
      }
    });

  // Delete Script command
  scriptCmd
    .command("delete <id>")
    .description("Delete Script")
    .option("-f, --force", "Forced deletion without prompting for confirmation")
    .action(async (id, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete script: ${id}`);
          // In practice, an interactive confirmation can be added here
          output.infoLog("Skip confirmation with the --force option");
          return;
        }

        const adapter = new ScriptAdapter();
        await adapter.deleteScript(id);
      } catch (error) {
        handleError(error, {
          operation: "deleteScript",
          additionalInfo: { id },
        });
      }
    });

  // Update Script Command
  scriptCmd
    .command("update <id>")
    .description("Update script")
    .option("-n, --name <name>", "Script name")
    .option("-d, --description <description>", "Script description")
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
          const adapter = new ScriptAdapter();

          if (options.name) updates.name = options.name;
          if (options.description) updates.description = options.description;

          const script = await adapter.updateScript(id, updates);
          output.output(formatScript(script, { verbose: options.verbose }));
        } catch (error) {
          handleError(error, {
            operation: "updateScript",
            additionalInfo: { id, updates },
          });
        }
      },
    );

  // Verify script configuration commands
  scriptCmd
    .command("validate <file>")
    .description("Validate script configuration file")
    .action(async file => {
      try {
        output.infoLog(`Validating script: ${file}`);

        const adapter = new ScriptAdapter();
        const result = await adapter.validateScript(file);

        if (result.valid) {
          output.info("Configuration validation passed");
        } else {
          output.fail("Configuration validation failed.");
          result.errors.forEach(err => {
            output.output(`  - ${err}`);
          });
          handleError(new CLIValidationError("Configuration validation failure"), {
            operation: "validateScript",
            additionalInfo: { file, errors: result.errors },
          });
          return;
        }
      } catch (error) {
        handleError(error, {
          operation: "validateScript",
          additionalInfo: { file },
        });
      }
    });

  // Execute script commands
  scriptCmd
    .command("execute <name>")
    .description("Execute script")
    .option("-i, --input <json>", "Input data (JSON format)")
    .option("-t, --timeout <timeout>", "Timeout time (milliseconds)")
    .option("-r, --retries <retries>", "Retries")
    .option("-d, --retry-delay <retryDelay>", "Retry delay (milliseconds)")
    .option("-w, --working-dir <workingDir>", "Working directory")
    .option("-e, --env <env>", "Environment variables (JSON format)")
    .option("-s, --sandbox", "Enable sandbox mode")
    .option("-v, --verbose", "Detailed output")
    .action(
      async (
        name,
        options: CommandOptions & {
          input?: string;
          timeout?: string;
          retries?: string;
          retryDelay?: string;
          workingDir?: string;
          env?: string;
          sandbox?: boolean;
        },
      ) => {
        try {
          output.infoLog(`Executing script: ${name}`);

          // Parsing Input Data
          let inputData: Record<string, any> | undefined;
          if (options.input) {
            try {
              inputData = JSON.parse(options.input);
            } catch (error) {
              handleError(new CLIValidationError("Input data must be in a valid JSON format"), {
                operation: "executeScript",
                additionalInfo: { name, input: options.input },
              });
              return;
            }
          }

          // Parsing Environment Variables
          let environment: Record<string, string> | undefined;
          if (options.env) {
            try {
              environment = JSON.parse(options.env);
            } catch (error) {
              handleError(
                new CLIValidationError("Environment variables must be in a valid JSON format"),
                {
                  operation: "executeScript",
                  additionalInfo: { name, env: options.env },
                },
              );
              return;
            }
          }

          // Build execution options
          const scriptOptions: any = {};
          if (options.timeout) scriptOptions.timeout = parseInt(options.timeout, 10);
          if (options.retries) scriptOptions.retries = parseInt(options.retries, 10);
          if (options.retryDelay) scriptOptions.retryDelay = parseInt(options.retryDelay, 10);
          if (options.workingDir) scriptOptions.workingDirectory = options.workingDir;
          if (environment) scriptOptions.environment = environment;
          if (options.sandbox) scriptOptions.sandbox = true;

          const adapter = new ScriptAdapter();
          const result = await adapter.executeScript(name, scriptOptions);

          if (options.verbose) {
            output.json(result);
          } else {
            output.info("Script executed successfully");
            if (result.output !== undefined) {
              output.output(`Output: ${JSON.stringify(result.output)}`);
            }
          }
        } catch (error) {
          handleError(error, {
            operation: "executeScript",
            additionalInfo: { name },
          });
        }
      },
    );

  return scriptCmd;
}
