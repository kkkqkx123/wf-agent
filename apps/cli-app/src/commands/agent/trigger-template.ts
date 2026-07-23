/**
 * Agent Trigger Template Command Group
 * Subcommands for managing agent-level trigger templates
 */

import { Command } from "commander";
import { AgentTemplateRegistryAdapter } from "../../adapters/agent-template-registry-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const output = getOutput();
const router = getRouter();

/**
 * Create Agent Trigger Template Commands
 */
export function createAgentTriggerTemplateCommands(): Command {
  const triggerTemplateCmd = new Command("trigger-template")
    .description("Manage agent trigger templates");

  // agent trigger-template list
  triggerTemplateCmd
    .command("list")
    .description("List all agent trigger templates")
    .option("-t, --type <triggerType>", "Filter by trigger type (event/condition/schedule)")
    .action(async (options: CommandOptions & { type?: string }) => {
      try {
        output.infoLog("Listing agent trigger templates...");
        const adapter = new AgentTemplateRegistryAdapter();

        let templates;
        if (options.type && ["event", "condition", "schedule"].includes(options.type)) {
          templates = await adapter.queryTriggerByType(
            options.type as "event" | "condition" | "schedule",
          );
        } else {
          templates = await adapter.listTriggerTemplates();
        }

        router.render(templates, {
          type: "list",
          entity: "agent-trigger-template",
          format: () => {
            if (templates.length === 0) return "No agent trigger templates found.";
            return templates
              .map(
                (t) =>
                  `  ${t.id} — ${t.name} [${t.type}] (${t.enabled ? "enabled" : "disabled"})`,
              )
              .join("\n");
          },
          metadata: { total: templates.length },
        });
      } catch (error) {
        handleError(error, { operation: "agent-trigger-template-list" });
      }
    });

  // agent trigger-template show <id>
  triggerTemplateCmd
    .command("show <id>")
    .description("Show details of an agent trigger template")
    .action(async (id: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Fetching trigger template "${id}"...`);
        const adapter = new AgentTemplateRegistryAdapter();
        const template = await adapter.getTriggerTemplate(id);

        router.render(template, {
          type: "detail",
          entity: "agent-trigger-template",
          format: () => {
            if (!template) return `Trigger template "${id}" not found.`;
            const lines: string[] = [];
            lines.push(`Trigger Template: ${template.name}`);
            lines.push(`  ID: ${template.id}`);
            lines.push(`  Type: ${template.type}`);
            lines.push(`  Action: ${template.actionType}`);
            lines.push(`  Enabled: ${template.enabled}`);
            if (template.description) lines.push(`  Description: ${template.description}`);
            if (template.category) lines.push(`  Category: ${template.category}`);
            if (template.condition) lines.push(`  Condition: ${template.condition}`);
            if (template.eventName) lines.push(`  Event: ${template.eventName}`);
            return lines.join("\n");
          },
          metadata: { id },
        });
      } catch (error) {
        handleError(error, { operation: "agent-trigger-template-show", additionalInfo: { id } });
      }
    });

  // agent trigger-template delete <id>
  triggerTemplateCmd
    .command("delete <id>")
    .description("Delete an agent trigger template")
    .action(async (id: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Deleting trigger template "${id}"...`);
        const adapter = new AgentTemplateRegistryAdapter();
        await adapter.deleteTriggerTemplate(id);
        output.success(`Trigger template "${id}" deleted.`);
      } catch (error) {
        handleError(error, { operation: "agent-trigger-template-delete", additionalInfo: { id } });
      }
    });

  return triggerTemplateCmd;
}
