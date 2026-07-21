/**
 * Plugin Command Group
 */

import { Command } from "commander";
import { PluginAdapter } from "../../adapters/plugin-adapter.js";
import { getRouter } from "../../utils/output-router.js";
import { formatPlugin, formatPluginList } from "../../utils/formatters/index.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const router = getRouter();

/**
 * Extended options for plugin commands
 */
interface PluginCommandOptions extends CommandOptions {
  table?: boolean;
  json?: boolean;
  verbose?: boolean;
}

/**
 * Create Plugin Command Group
 */
export function createPluginCommands(): Command {
  const pluginCmd = new Command("plugin").description("Manage plugins");

  // List plugins
  pluginCmd
    .command("list")
    .description("List all plugins")
    .option("-t, --table", "Output in table format")
    .option("-v, --verbose", "Detailed output")
    .action(async (options: PluginCommandOptions) => {
      try {
        const adapter = new PluginAdapter();
        const plugins = await adapter.listPlugins();

        router.render(plugins, {
          type: "list",
          entity: "plugin",
          format: () => formatPluginList(plugins as never, {
            table: options.table,
            verbose: options.verbose,
          }),
          metadata: { total: plugins.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "plugin-list",
        });
      }
    });

  // Show plugin details
  pluginCmd
    .command("show <id>")
    .description("Show plugin details")
    .option("--json", "Output as JSON")
    .action(async (id, options: PluginCommandOptions) => {
      try {
        const adapter = new PluginAdapter();
        const plugin = await adapter.getPlugin(id);

        router.render(plugin, {
          type: "detail",
          entity: "plugin",
          format: () => formatPlugin(plugin as never, { json: options.json, verbose: options.verbose }),
        });
      } catch (error) {
        handleError(error, {
          operation: "plugin-show",
          additionalInfo: { id },
        });
      }
    });

  // Load plugin from path
  pluginCmd
    .command("load <path>")
    .description("Load a plugin from a filesystem path")
    .action(async (filePath) => {
      try {
        const adapter = new PluginAdapter();
        const plugin = await adapter.loadPlugin(filePath);

        router.render(plugin, {
          type: "detail",
          entity: "plugin",
          format: () => formatPlugin(plugin as never),
          message: `Plugin loaded: ${plugin.manifest.name || plugin.manifest.id}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "plugin-load",
          additionalInfo: { path: filePath },
        });
      }
    });

  // Find plugin by source
  pluginCmd
    .command("find <source>")
    .description("Find a plugin by ID/name/entryPoint")
    .action(async (source) => {
      try {
        const adapter = new PluginAdapter();
        const plugin = await adapter.findPlugin(source);

        router.render(plugin, {
          type: "detail",
          entity: "plugin",
          format: () => formatPlugin(plugin as never),
          message: `Plugin found: ${plugin.manifest.name || plugin.manifest.id}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "plugin-find",
          additionalInfo: { source },
        });
      }
    });

  // Activate plugin
  pluginCmd
    .command("activate <id>")
    .description("Activate a plugin")
    .action(async (id) => {
      try {
        const adapter = new PluginAdapter();
        await adapter.activatePlugin(id);

        router.render(null, {
          type: "action",
          entity: "plugin",
          message: `Plugin activated: ${id}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "plugin-activate",
          additionalInfo: { id },
        });
      }
    });

  // Deactivate plugin
  pluginCmd
    .command("deactivate <id>")
    .description("Deactivate a plugin")
    .action(async (id) => {
      try {
        const adapter = new PluginAdapter();
        await adapter.deactivatePlugin(id);

        router.render(null, {
          type: "action",
          entity: "plugin",
          message: `Plugin deactivated: ${id}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "plugin-deactivate",
          additionalInfo: { id },
        });
      }
    });

  // Reload plugin
  pluginCmd
    .command("reload <id>")
    .description("Hot-reload a plugin")
    .action(async (id) => {
      try {
        const adapter = new PluginAdapter();
        await adapter.reloadPlugin(id);

        router.render(null, {
          type: "action",
          entity: "plugin",
          message: `Plugin reloaded: ${id}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "plugin-reload",
          additionalInfo: { id },
        });
      }
    });

  // Unload plugin
  pluginCmd
    .command("unload <id>")
    .description("Unload a plugin from the registry")
    .action(async (id) => {
      try {
        const adapter = new PluginAdapter();
        await adapter.unloadPlugin(id);

        router.render(null, {
          type: "action",
          entity: "plugin",
          message: `Plugin unloaded: ${id}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "plugin-unload",
          additionalInfo: { id },
        });
      }
    });

  // Config subcommand group
  const configCmd = new Command("config").description("Manage plugin configuration");

  configCmd
    .command("show <id>")
    .description("Show plugin configuration")
    .option("--json", "Output as JSON")
    .action(async (id, options: PluginCommandOptions) => {
      try {
        const adapter = new PluginAdapter();
        const config = await adapter.getPluginConfig(id);

        router.render(config, {
          type: "detail",
          entity: "plugin-config",
          format: () => {
            if (options.json) {
              return JSON.stringify(config, null, 2);
            }
            return Object.entries(config)
              .map(([key, value]) => `  ${key}: ${JSON.stringify(value)}`)
              .join("\n");
          },
        });
      } catch (error) {
        handleError(error, {
          operation: "plugin-config-show",
          additionalInfo: { id },
        });
      }
    });

  configCmd
    .command("update <id>")
    .description("Update plugin configuration")
    .option("--json <json>", "Update via JSON object")
    .argument("[key=value...]", "Configuration key=value pairs")
    .action(async (id, pairs: string[], options: PluginCommandOptions) => {
      try {
        const adapter = new PluginAdapter();
        let config: Record<string, unknown> = {};

        if (options.json) {
          config = JSON.parse(options.json as unknown as string) as Record<string, unknown>;
        } else if (pairs && pairs.length > 0) {
          for (const pair of pairs) {
            const eqIdx = pair.indexOf("=");
            if (eqIdx === -1) {
              throw new Error(`Invalid key=value pair: ${pair}`);
            }
            const key = pair.substring(0, eqIdx);
            const value = pair.substring(eqIdx + 1);
            // Try to parse as JSON value, fall back to string
            try {
              config[key] = JSON.parse(value);
            } catch {
              config[key] = value;
            }
          }
        } else {
          throw new Error("Provide configuration via --json <json> or key=value pairs");
        }

        await adapter.updatePluginConfig(id, config);

        router.render(null, {
          type: "action",
          entity: "plugin-config",
          message: `Plugin config updated: ${id}`,
        });
      } catch (error) {
        handleError(error, {
          operation: "plugin-config-update",
          additionalInfo: { id },
        });
      }
    });

  pluginCmd.addCommand(configCmd);

  return pluginCmd;
}