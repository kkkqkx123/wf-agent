/**
 * Trigger Command Group
 */

import { Command } from "commander";
import { TriggerAdapter } from "../../adapters/trigger-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import {
  formatTrigger,
  formatTriggerList,
  formatTriggerTemplate,
  formatTriggerTemplateList,
} from "../../utils/cli-formatters.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const output = getOutput();
const router = getRouter();

/**
 * Extended options for trigger template commands
 */
interface TemplateCommandOptions extends CommandOptions {
  params?: string;
  type?: string;
  category?: string;
  tag?: string;
  json?: boolean;
  force?: boolean;
  cascade?: boolean;
  output?: string;
}

/**
 * Create Trigger Command Group
 */
export function createTriggerCommands(): Command {
  const triggerCmd = new Command("trigger").description("Manage Triggers");

  // List trigger commands
  triggerCmd
    .command("list")
    .description("List all triggers")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .action(async (options: CommandOptions) => {
      try {
        const adapter = new TriggerAdapter();
        const triggers = await adapter.listTriggers();

        router.render(triggers, {
          type: "list",
          entity: "trigger",
          format: () => formatTriggerList(triggers, { table: options.table }),
          metadata: { total: triggers.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "list-triggers",
        });
      }
    });

  // View Trigger Details Command
  triggerCmd
    .command("show <id>")
    .description("View trigger details")
    .option("-v, --verbose", "Detailed output")
    .action(async (id, options: CommandOptions) => {
      try {
        const adapter = new TriggerAdapter();
        const trigger = await adapter.getTrigger(id);

        output.output(formatTrigger(trigger, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "show-trigger",
          additionalInfo: { id },
        });
      }
    });

  // Enable trigger command
  triggerCmd
    .command("enable <executionId> <triggerId>")
    .description("Enable the trigger")
    .action(async (executionId, triggerId) => {
      try {
        output.infoLog(`Enabling trigger: ${triggerId}`);

        const adapter = new TriggerAdapter();
        await adapter.enableTrigger(executionId, triggerId);
      } catch (error) {
        handleError(error, {
          operation: "enable-trigger",
          additionalInfo: { executionId, triggerId },
        });
      }
    });

  // Disable trigger command
  triggerCmd
    .command("disable <executionId> <triggerId>")
    .description("Disable the trigger.")
    .action(async (executionId, triggerId) => {
      try {
        output.infoLog(`Disabling trigger: ${triggerId}`);

        const adapter = new TriggerAdapter();
        await adapter.disableTrigger(executionId, triggerId);
      } catch (error) {
        handleError(error, {
          operation: "disable-trigger",
          additionalInfo: { executionId, triggerId },
        });
      }
    });

  // List trigger commands by execution entity
  triggerCmd
    .command("list-by-execution <execution-id>")
    .description("List triggers by workflow execution ID")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .action(async (executionId, options: CommandOptions) => {
      try {
        const adapter = new TriggerAdapter();
        const triggers = await adapter.listTriggersByWorkflowExecution(executionId);

        router.render(triggers, {
          type: "list",
          entity: "trigger",
          format: () => formatTriggerList(triggers, { table: options.table }),
          metadata: { total: triggers.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "list-triggers-by-execution",
          additionalInfo: { executionId },
        });
      }
    });

  // ========================================================================
  // Trigger Template Subcommand Group
  // ========================================================================

  const templateCmd = new Command("template").description("Manage trigger templates");

  // Register trigger template from file
  templateCmd
    .command("register <file>")
    .description("Register a trigger template from a configuration file")
    .option("-p, --params <params>", "Runtime parameters (JSON format)")
    .action(async (file, options: TemplateCommandOptions) => {
      try {
        const adapter = new TriggerAdapter();
        let parameters: Record<string, unknown> | undefined;

        if (options.params) {
          try {
            parameters = JSON.parse(options.params) as Record<string, unknown>;
          } catch {
            throw new Error("Invalid JSON format for --params option");
          }
        }

        const template = await adapter.registerTriggerTemplateFromFile(file, parameters);
        output.output(formatTriggerTemplate(template, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "register-trigger-template",
          additionalInfo: { file },
        });
      }
    });

  // List trigger templates
  templateCmd
    .command("list")
    .description("List all trigger templates")
    .option("-t, --table", "Output in table format")
    .option("--type <type>", "Filter by trigger type")
    .option("--category <category>", "Filter by category")
    .option("--tag <tag>", "Filter by tag")
    .action(async (options: TemplateCommandOptions) => {
      try {
        const adapter = new TriggerAdapter();
        const filter: Record<string, unknown> = {};
        if (options.type) filter["triggerType"] = options.type;
        if (options.category) filter["category"] = options.category;
        if (options.tag) filter["tags"] = [options.tag];

        const templates = await adapter.listTriggerTemplates(
          Object.keys(filter).length > 0 ? (filter as any) : undefined,
        );

        router.render(templates, {
          type: "list",
          entity: "trigger-template",
          format: () => formatTriggerTemplateList(templates, { table: options.table }),
          metadata: { total: templates.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "list-trigger-templates",
        });
      }
    });

  // Show trigger template details
  templateCmd
    .command("show <name>")
    .description("Show trigger template details")
    .option("--json", "Output as JSON")
    .action(async (name, options: TemplateCommandOptions) => {
      try {
        const adapter = new TriggerAdapter();
        const template = await adapter.getTriggerTemplate(name);

        router.render(template, {
          type: "detail",
          entity: "trigger-template",
          format: () => formatTriggerTemplate(template, { verbose: options.json }),
        });
      } catch (error) {
        handleError(error, {
          operation: "show-trigger-template",
          additionalInfo: { name },
        });
      }
    });

  // Delete trigger template
  templateCmd
    .command("delete <name>")
    .description("Delete a trigger template")
    .option("-f, --force", "Force deletion even if referenced")
    .option("--cascade", "Cascade delete references")
    .action(async (name, options: TemplateCommandOptions) => {
      try {
        const adapter = new TriggerAdapter();
        await adapter.deleteTriggerTemplate(name, {
          force: options.force ?? options.cascade ?? false,
          checkReferences: !options.force && !options.cascade,
        });
      } catch (error) {
        handleError(error, {
          operation: "delete-trigger-template",
          additionalInfo: { name },
        });
      }
    });

  // Export trigger template
  templateCmd
    .command("export <name>")
    .description("Export a trigger template as JSON")
    .option("--format <format>", "Output format (json, toml)", "json")
    .option("-o, --output <file>", "Output file path")
    .action(async (name, options: TemplateCommandOptions) => {
      try {
        const adapter = new TriggerAdapter();
        const json = await adapter.exportTriggerTemplate(name);

        if (options.output) {
          const fs = await import("fs/promises");
          await fs.writeFile(options.output, json, "utf-8");
          output.success(`Trigger template exported to: ${options.output}`);
        } else {
          output.output(json);
        }
      } catch (error) {
        handleError(error, {
          operation: "export-trigger-template",
          additionalInfo: { name },
        });
      }
    });

  triggerCmd.addCommand(templateCmd);

  return triggerCmd;
}
