/**
 * Human Relay Command Group
 */

import { Command } from "commander";
import * as fs from "fs/promises";
import { HumanRelayAdapter } from "../../adapters/human-relay-adapter.js";
import { getOutput } from "../../utils/output.js";
import { formatHumanRelay, formatHumanRelayList } from "../../utils/cli-formatters.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";

const output = getOutput();

/**
 * Create Human Relay Command Group
 */
export function createHumanRelayCommands(): Command {
  const humanRelayCmd = new Command("human-relay")
    .description("Manage Human Relay Configuration")
    .alias("hr");

  // Registering Human Relay Configuration Commands
  humanRelayCmd
    .command("register <file>")
    .description("Register Human Relay configuration from file")
    .option("-v, --verbose", "Detailed output")
    .action(async (file, options: CommandOptions) => {
      try {
        output.infoLog(`Registering Human Relay from file: ${file}`);

        // Read configuration file
        const content = await fs.readFile(file, "utf-8");
        const config = JSON.parse(content);

        const adapter = new HumanRelayAdapter();
        const result = await adapter.createConfig(config);

        output.output(formatHumanRelay(result, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "registerHumanRelayConfig",
          additionalInfo: { file },
        });
      }
    });

  // List Human Relay configuration commands
  humanRelayCmd
    .command("list")
    .description("List all Human Relay configurations")
    .option("-t, --table", "Output in tabular format")
    .option("-v, --verbose", "Detailed output")
    .option("--enabled", "Show only enabled configurations")
    .action(async (options: CommandOptions & { enabled?: boolean }) => {
      try {
        const adapter = new HumanRelayAdapter();
        const filter = options.enabled !== undefined ? { enabled: options.enabled } : undefined;
        const configs = await adapter.listConfigs(filter);

        output.output(formatHumanRelayList(configs, { table: options.table }));
      } catch (error) {
        handleError(error, {
          operation: "listHumanRelayConfigs",
          additionalInfo: { enabled: options.enabled },
        });
      }
    });

  // View Human Relay configuration details command
  humanRelayCmd
    .command("show <id>")
    .description("View Human Relay configuration details")
    .option("-v, --verbose", "Detailed output")
    .action(async (id, options: CommandOptions) => {
      try {
        const adapter = new HumanRelayAdapter();
        const config = await adapter.getConfig(id);

        output.output(formatHumanRelay(config, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "getHumanRelayConfig",
          additionalInfo: { id },
        });
      }
    });

  // Deleting the Human Relay Configuration Command
  humanRelayCmd
    .command("delete <id>")
    .description("Delete Human Relay configuration")
    .option("-f, --force", "Forced deletion without prompting for confirmation")
    .action(async (id, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete Human Relay: ${id}`);
          // In practice, an interactive confirmation can be added here
          output.infoLog("Skip confirmation with the --force option");
          return;
        }

        const adapter = new HumanRelayAdapter();
        await adapter.deleteConfig(id);
      } catch (error) {
        handleError(error, {
          operation: "deleteHumanRelayConfig",
          additionalInfo: { id },
        });
      }
    });

  // Update Human Relay Configuration Commands
  humanRelayCmd
    .command("update <id>")
    .description("Update Human Relay Configuration")
    .option("-n, --name <name>", "Configuration name")
    .option("-d, --description <description>", "Configuration description")
    .option("-t, --timeout <timeout>", "Default timeout in milliseconds")
    .option("-v, --verbose", "Detailed output")
    .action(
      async (
        id,
        options: CommandOptions & {
          name?: string;
          description?: string;
          timeout?: string;
        },
      ) => {
        try {
          const adapter = new HumanRelayAdapter();
          const updates: any = {};

          if (options.name) updates.name = options.name;
          if (options.description) updates.description = options.description;
          if (options.timeout) updates.defaultTimeout = parseInt(options.timeout, 10);

          const config = await adapter.updateConfig(id, updates);
          output.output(formatHumanRelay(config, { verbose: options.verbose }));
        } catch (error) {
          handleError(error, {
            operation: "updateHumanRelayConfig",
            additionalInfo: { id },
          });
        }
      },
    );

  // Enabling Human Relay Configuration Commands
  humanRelayCmd
    .command("enable <id>")
    .description("Enable Human Relay configuration")
    .action(async id => {
      try {
        output.infoLog(`Enabling Human Relay: ${id}`);

        const adapter = new HumanRelayAdapter();
        await adapter.enableConfig(id);
      } catch (error) {
        handleError(error, {
          operation: "enableHumanRelayConfig",
          additionalInfo: { id },
        });
      }
    });

  // Disabling the Human Relay Configuration Command
  humanRelayCmd
    .command("disable <id>")
    .description("Disable Human Relay configuration")
    .action(async id => {
      try {
        output.infoLog(`Disabling Human Relay: ${id}`);

        const adapter = new HumanRelayAdapter();
        await adapter.disableConfig(id);
      } catch (error) {
        handleError(error, {
          operation: "disableHumanRelayConfig",
          additionalInfo: { id },
        });
      }
    });

  return humanRelayCmd;
}
