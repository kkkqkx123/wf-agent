/**
 * User Interaction Command Group
 * Manage user interaction configurations for approval workflows
 */

import { Command } from "commander";
import { UserInteractionAdapter } from "../../adapters/user-interaction-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { getFormatter } from "../../utils/formatter.js";
import { handleError } from "../../utils/error-handler.js";
import { CLIValidationError } from "../../types/cli-types.js";
import type { CommandOptions } from "../../types/cli-types.js";

const output = getOutput();
const router = getRouter();

interface UIConfigOptions extends CommandOptions {
  name?: string;
  description?: string;
  timeout?: string;
  json?: boolean;
  metadata?: string;
  force?: boolean;
}

/**
 * Create User Interaction Command Group
 */
export function createUserInteractionCommands(): Command {
  const uiCmd = new Command("user-interaction").description("Manage user interaction configurations");

  // List configurations
  uiCmd
    .command("list")
    .description("List all user interaction configurations")
    .option("-t, --table", "Output in table format")
    .option("-v, --verbose", "Detailed output")
    .action(async (options: CommandOptions) => {
      try {
        const adapter = new UserInteractionAdapter();
        const configs = await adapter.listConfigs();

        router.render(configs, {
          type: "list",
          entity: "user-interaction",
          format: () => {
            if (configs.length === 0) return "No user interaction configurations found";
            if (options.table) {
              const formatter = getFormatter();
              const headers = ["ID", "Name", "Description", "Timeout"];
              const rows = configs.map((c: any) => [
                c.id || "N/A",
                c.name || "N/A",
                (c.description || "-").substring(0, 40),
                String(c.defaultTimeout || "-"),
              ]);
              return formatter.table(headers, rows);
            }
            return configs
              .map((c: any) => {
                const desc = c.description ? ` - ${c.description}` : "";
                return `  ${c.id} (${c.name})${desc}`;
              })
              .join("\n");
          },
          metadata: { total: configs.length },
        });
      } catch (error) {
        handleError(error, { operation: "list-user-interaction-configs" });
      }
    });

  // Show configuration details
  uiCmd
    .command("show <id>")
    .description("Show user interaction configuration details")
    .option("-v, --verbose", "Detailed output")
    .action(async (id) => {
      try {
        const adapter = new UserInteractionAdapter();
        const config = await adapter.getConfig(id);

        output.newLine();
        output.output(getFormatter().subsection(`User Interaction Configuration: ${id}`));
        output.output(getFormatter().keyValue("ID", config.id));
        output.output(getFormatter().keyValue("Name", config.name));
        if (config.description) output.output(getFormatter().keyValue("Description", config.description));
        if (config.defaultTimeout) output.output(getFormatter().keyValue("Default Timeout", `${config.defaultTimeout}ms`));
        if (config.metadata) output.output(getFormatter().keyValue("Metadata", JSON.stringify(config.metadata)));
      } catch (error) {
        handleError(error, { operation: "show-user-interaction-config", additionalInfo: { id } });
      }
    });

  // Create configuration
  uiCmd
    .command("create <id>")
    .description("Create a user interaction configuration")
    .requiredOption("-n, --name <name>", "Configuration name")
    .option("--description <description>", "Configuration description")
    .option("--timeout <ms>", "Default timeout in milliseconds")
    .option("--metadata <json>", "Metadata (JSON format)")
    .action(async (id, options: UIConfigOptions) => {
      try {
        const adapter = new UserInteractionAdapter();
        const config: any = {
          id,
          name: options.name,
        };
        if (options.description) config.description = options.description;
        if (options.timeout) config.defaultTimeout = parseInt(options.timeout, 10);
        if (options.metadata) {
          try {
            config.metadata = JSON.parse(options.metadata);
          } catch {
            throw new CLIValidationError("Metadata must be valid JSON");
          }
        }

        await adapter.createConfig(config);
        output.success(`User interaction configuration created: ${id}`);
      } catch (error) {
        handleError(error, { operation: "create-user-interaction-config", additionalInfo: { id } });
      }
    });

  // Update configuration
  uiCmd
    .command("update <id>")
    .description("Update a user interaction configuration")
    .option("--name <name>", "Configuration name")
    .option("--description <description>", "Configuration description")
    .option("--timeout <ms>", "Default timeout in milliseconds")
    .option("--metadata <json>", "Metadata (JSON format)")
    .action(async (id, options: UIConfigOptions) => {
      try {
        const adapter = new UserInteractionAdapter();
        const updates: any = {};
        if (options.name) updates.name = options.name;
        if (options.description) updates.description = options.description;
        if (options.timeout) updates.defaultTimeout = parseInt(options.timeout, 10);
        if (options.metadata) {
          try {
            updates.metadata = JSON.parse(options.metadata);
          } catch {
            throw new CLIValidationError("Metadata must be valid JSON");
          }
        }

        await adapter.updateConfig(id, updates);
        output.success(`User interaction configuration updated: ${id}`);
      } catch (error) {
        handleError(error, { operation: "update-user-interaction-config", additionalInfo: { id } });
      }
    });

  // Delete configuration
  uiCmd
    .command("delete <id>")
    .description("Delete a user interaction configuration")
    .option("-f, --force", "Skip confirmation")
    .action(async (id, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete user interaction configuration: ${id}`);
          output.infoLog("Use the --force option to skip confirmation.");
          return;
        }

        const adapter = new UserInteractionAdapter();
        await adapter.deleteConfig(id);
        output.success(`User interaction configuration deleted: ${id}`);
      } catch (error) {
        handleError(error, { operation: "delete-user-interaction-config", additionalInfo: { id } });
      }
    });

  // Enable configuration
  uiCmd
    .command("enable <id>")
    .description("Enable a user interaction configuration")
    .action(async (id) => {
      try {
        const adapter = new UserInteractionAdapter();
        await adapter.enableConfig(id);
        output.success(`User interaction configuration enabled: ${id}`);
      } catch (error) {
        handleError(error, { operation: "enable-user-interaction-config", additionalInfo: { id } });
      }
    });

  // Disable configuration
  uiCmd
    .command("disable <id>")
    .description("Disable a user interaction configuration")
    .action(async (id) => {
      try {
        const adapter = new UserInteractionAdapter();
        await adapter.disableConfig(id);
        output.success(`User interaction configuration disabled: ${id}`);
      } catch (error) {
        handleError(error, { operation: "disable-user-interaction-config", additionalInfo: { id } });
      }
    });

  return uiCmd;
}