/**
 * LLM Profile Command Group
 */

import { Command } from "commander";
import { LLMProfileAdapter } from "../../adapters/llm-profile-adapter.js";
import { getOutput } from "../../utils/output.js";
import { formatLLMProfile, formatLLMProfileList } from "../../utils/cli-formatters.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";
import { CLIValidationError } from "../../types/cli-types.js";

const output = getOutput();

/**
 * Create LLM Profile Command Group
 */
export function createLLMProfileCommands(): Command {
  const llmProfileCmd = new Command("llm-profile")
    .description("Manage LLM Profile Configuration")
    .alias("llm");

  // Register LLM Profile command
  llmProfileCmd
    .command("register <file>")
    .description("Register LLM Profile from file")
    .option("-v, --verbose", "Detailed output")
    .action(async (file, options: CommandOptions) => {
      try {
        output.infoLog(`Registering LLM Profile from file: ${file}`);

        const adapter = new LLMProfileAdapter();
        const profile = await adapter.registerFromFile(file);

        output.output(formatLLMProfile(profile, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "registerLLMProfile",
          additionalInfo: { file },
        });
      }
    });

  // Batch registration command for LLM Profiles
  llmProfileCmd
    .command("register-batch <directory>")
    .description("Batch register LLM Profiles from directory")
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
          output.infoLog(`Batch registering LLM Profiles from directory: ${directory}`);

          // Parse file patterns
          const filePattern = options.pattern ? new RegExp(options.pattern) : undefined;

          const adapter = new LLMProfileAdapter();
          const result = await adapter.registerFromDirectory({
            configDir: directory,
            recursive: options.recursive,
            filePattern,
          });

          // Display the results
          output.newLine();
          output.info(`Success: ${result.success.length} profiles registered`);
          if (result.failures.length > 0) {
            output.newLine();
            output.fail(`Failed: ${result.failures.length} profiles`);
            result.failures.forEach(failure => {
              output.output(`  - ${failure.filePath}: ${failure.error}`);
            });
          }
        } catch (error) {
          handleError(error, {
            operation: "registerLLMProfilesBatch",
            additionalInfo: { directory, recursive: options.recursive, pattern: options.pattern },
          });
        }
      },
    );

  // List the LLM Profile command
  llmProfileCmd
    .command("list")
    .description("List all LLM Profiles")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .action(async (options: CommandOptions) => {
      try {
        const adapter = new LLMProfileAdapter();
        const profiles = await adapter.listProfiles();

        output.output(formatLLMProfileList(profiles, { table: options.table }));
      } catch (error) {
        handleError(error, {
          operation: "listLLMProfiles",
        });
      }
    });

  // View LLM Profile details command
  llmProfileCmd
    .command("show <id>")
    .description("View LLM Profile details")
    .option("-v, --verbose", "Detailed output")
    .action(async (id, options: CommandOptions) => {
      try {
        const adapter = new LLMProfileAdapter();
        const profile = await adapter.getProfile(id);

        output.output(formatLLMProfile(profile, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "getLLMProfile",
          additionalInfo: { id },
        });
      }
    });

  // Delete the LLM Profile command
  llmProfileCmd
    .command("delete <id>")
    .description("Delete the LLM Profile")
    .option("-f, --force", "Forced deletion, without prompting for confirmation.")
    .action(async (id, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete LLM Profile: ${id}`);
          // In practical applications, interactive confirmation can be added here.
          output.infoLog("Use the --force option to skip the confirmation.");
          return;
        }

        const adapter = new LLMProfileAdapter();
        await adapter.deleteProfile(id);
      } catch (error) {
        handleError(error, {
          operation: "deleteLLMProfile",
          additionalInfo: { id },
        });
      }
    });

  // Update LLM Profile command
  llmProfileCmd
    .command("update <id>")
    .description("Update LLM Profile")
    .option("-n, --name <name>", "Profile Name")
    .option("-m, --model <model>", "Model Name")
    .option("-k, --api-key <apiKey>", "API Key")
    .option("-b, --base-url <baseUrl>", "Base URL")
    .option("-t, --timeout <timeout>", "Timeout period (in milliseconds)")
    .option("-r, --max-retries <maxRetries>", "Maximum number of retries")
    .option("-d, --retry-delay <retryDelay>", "Retry delay (in milliseconds)")
    .option("-v, --verbose", "Detailed output")
    .action(
      async (
        id,
        options: CommandOptions & {
          name?: string;
          model?: string;
          apiKey?: string;
          baseUrl?: string;
          timeout?: string;
          maxRetries?: string;
          retryDelay?: string;
        },
      ) => {
        const updates: any = {};
        try {
          const adapter = new LLMProfileAdapter();

          if (options.name) updates.name = options.name;
          if (options.model) updates.model = options.model;
          if (options.apiKey) updates.apiKey = options.apiKey;
          if (options.baseUrl) updates.baseUrl = options.baseUrl;
          if (options.timeout) updates.timeout = parseInt(options.timeout, 10);
          if (options.maxRetries) updates.maxRetries = parseInt(options.maxRetries, 10);
          if (options.retryDelay) updates.retryDelay = parseInt(options.retryDelay, 10);

          const profile = await adapter.updateProfile(id, updates);
          output.output(formatLLMProfile(profile, { verbose: options.verbose }));
        } catch (error) {
          handleError(error, {
            operation: "updateLLMProfile",
            additionalInfo: { id, updates },
          });
        }
      },
    );

  // Verify the LLM Profile configuration command.
  llmProfileCmd
    .command("validate <file>")
    .description("Verify the LLM Profile configuration file.")
    .action(async file => {
      try {
        output.infoLog(`Validating LLM Profile: ${file}`);

        const adapter = new LLMProfileAdapter();
        const result = await adapter.validateProfile(file);

        if (result.valid) {
          output.info("Configuration validation passed.");
        } else {
          output.fail("Configuration validation failed:");
          result.errors.forEach(err => {
            output.output(`  - ${err}`);
          });
          handleError(new CLIValidationError("Configuration validation failed."), {
            operation: "validateLLMProfile",
            additionalInfo: { file, errors: result.errors },
          });
          return;
        }
      } catch (error) {
        handleError(error, {
          operation: "validateLLMProfile",
          additionalInfo: { file },
        });
      }
    });

  // Set the default LLM Profile command
  llmProfileCmd
    .command("set-default <id>")
    .description("Set the default LLM Profile")
    .action(async id => {
      try {
        output.infoLog(`Setting default LLM Profile: ${id}`);

        const adapter = new LLMProfileAdapter();
        await adapter.setDefaultProfile(id);
      } catch (error) {
        handleError(error, {
          operation: "setDefaultLLMProfile",
          additionalInfo: { id },
        });
      }
    });

  // Get the default LLM Profile command
  llmProfileCmd
    .command("get-default")
    .description("Get the default LLM Profile")
    .option("-v, --verbose", "Detailed output")
    .action(async (options: CommandOptions) => {
      try {
        const adapter = new LLMProfileAdapter();
        const profile = await adapter.getDefaultProfile();

        if (profile) {
          output.output(formatLLMProfile(profile, { verbose: options.verbose }));
        } else {
          output.output("No default LLM Profile is set.");
        }
      } catch (error) {
        handleError(error, {
          operation: "getDefaultLLMProfile",
        });
      }
    });

  // Export LLM Profile command
  llmProfileCmd
    .command("export <id>")
    .description("Export LLM Profile (hide sensitive information)")
    .action(async id => {
      try {
        const adapter = new LLMProfileAdapter();
        const json = await adapter.exportProfile(id);
        output.output(json);
      } catch (error) {
        handleError(error, {
          operation: "exportLLMProfile",
          additionalInfo: { id },
        });
      }
    });

  // Import LLM Profile command
  llmProfileCmd
    .command("import <json>")
    .description("Import LLM Profile from JSON")
    .option("-v, --verbose", "Detailed output")
    .action(async (json, options: CommandOptions) => {
      try {
        output.infoLog("Importing LLM Profile");

        const adapter = new LLMProfileAdapter();
        const profile = await adapter.importProfile(json);

        output.output(formatLLMProfile(profile, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "importLLMProfile",
        });
      }
    });

  return llmProfileCmd;
}
