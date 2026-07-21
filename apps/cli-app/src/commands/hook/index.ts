/**
 * Hook Command Group
 * Manage hook templates
 */

import { Command } from "commander";
import { HookAdapter } from "../../adapters/hook-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { formatHookTemplate, formatHookTemplateList } from "../../utils/formatters/hook.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";

const output = getOutput();
const router = getRouter();

interface HookCommandOptions extends CommandOptions {
  params?: string;
  type?: string;
  category?: string;
  tag?: string;
  json?: boolean;
  force?: boolean;
  output?: string;
}

/**
 * Create Hook Command Group
 */
export function createHookCommands(): Command {
  const hookCmd = new Command("hook").description("Manage hook templates");

  // Register hook template from file
  hookCmd
    .command("register <file>")
    .description("Register a hook template from a configuration file")
    .option("-p, --params <params>", "Runtime parameters (JSON format)")
    .action(async (file, options: HookCommandOptions) => {
      try {
        const adapter = new HookAdapter();
        let parameters: Record<string, unknown> | undefined;

        if (options.params) {
          try {
            parameters = JSON.parse(options.params) as Record<string, unknown>;
          } catch {
            throw new Error("Invalid JSON format for --params option");
          }
        }

        const template = await adapter.registerHookTemplateFromFile(file, parameters);
        output.output(formatHookTemplate(template, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "register-hook-template",
          additionalInfo: { file },
        });
      }
    });

  // List hook templates
  hookCmd
    .command("list")
    .description("List all hook templates")
    .option("-t, --table", "Output in table format")
    .option("--type <type>", "Filter by hook type")
    .option("--category <category>", "Filter by category")
    .option("--tag <tag>", "Filter by tag")
    .action(async (options: HookCommandOptions) => {
      try {
        const adapter = new HookAdapter();
        const filter: Record<string, unknown> = {};
        if (options.type) filter["hookType"] = options.type;
        if (options.category) filter["category"] = options.category;
        if (options.tag) filter["tags"] = [options.tag];

        const templates = await adapter.listHookTemplates(
          Object.keys(filter).length > 0 ? (filter as any) : undefined,
        );

        router.render(templates, {
          type: "list",
          entity: "hook-template",
          format: () => formatHookTemplateList(templates, { table: options.table }),
          metadata: { total: templates.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "list-hook-templates",
        });
      }
    });

  // Show hook template details
  hookCmd
    .command("show <name>")
    .description("Show hook template details")
    .option("--json", "Output as JSON")
    .action(async (name, options: HookCommandOptions) => {
      try {
        const adapter = new HookAdapter();
        const template = await adapter.getHookTemplate(name);

        router.render(template, {
          type: "detail",
          entity: "hook-template",
          format: () => formatHookTemplate(template, { verbose: options.json }),
        });
      } catch (error) {
        handleError(error, {
          operation: "show-hook-template",
          additionalInfo: { name },
        });
      }
    });

  // Delete hook template
  hookCmd
    .command("delete <name>")
    .description("Delete a hook template")
    .option("-f, --force", "Force deletion even if referenced")
    .action(async (name, options: HookCommandOptions) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete hook template: ${name}`);
          output.infoLog("Use the --force option to skip confirmation.");
          return;
        }

        const adapter = new HookAdapter();
        await adapter.deleteHookTemplate(name);
      } catch (error) {
        handleError(error, {
          operation: "delete-hook-template",
          additionalInfo: { name },
        });
      }
    });

  // Export hook template
  hookCmd
    .command("export <name>")
    .description("Export a hook template as JSON")
    .option("--format <format>", "Output format (json)", "json")
    .option("-o, --output <file>", "Output file path")
    .action(async (name, options: HookCommandOptions) => {
      try {
        const adapter = new HookAdapter();
        const json = await adapter.exportHookTemplate(name);

        if (options.output) {
          const fs = await import("fs/promises");
          await fs.writeFile(options.output, json, "utf-8");
          output.success(`Hook template exported to: ${options.output}`);
        } else {
          output.output(json);
        }
      } catch (error) {
        handleError(error, {
          operation: "export-hook-template",
          additionalInfo: { name },
        });
      }
    });

  return hookCmd;
}