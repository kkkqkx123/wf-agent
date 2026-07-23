/**
 * Agent Hook Template Command Group
 * Subcommands for managing agent-level hook templates
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
 * Create Agent Hook Template Commands
 */
export function createAgentHookTemplateCommands(): Command {
  const hookTemplateCmd = new Command("hook-template")
    .description("Manage agent hook templates");

  // agent hook-template list
  hookTemplateCmd
    .command("list")
    .description("List all agent hook templates")
    .option("-t, --type <hookType>", "Filter by hook type")
    .action(async (options: CommandOptions & { type?: string }) => {
      try {
        output.infoLog("Listing agent hook templates...");
        const adapter = new AgentTemplateRegistryAdapter();

        const templates = options.type
          ? await adapter.queryHookByType(options.type)
          : await adapter.listHookTemplates();

        router.render(templates, {
          type: "list",
          entity: "agent-hook-template",
          format: () => {
            if (templates.length === 0) return "No agent hook templates found.";
            return templates
              .map(
                (t) =>
                  `  ${t.name ?? "unnamed"} [hook type: ${(t as any).hookType ?? "N/A"}]`,
              )
              .join("\n");
          },
          metadata: { total: templates.length },
        });
      } catch (error) {
        handleError(error, { operation: "agent-hook-template-list" });
      }
    });

  // agent hook-template show <id>
  hookTemplateCmd
    .command("show <id>")
    .description("Show details of an agent hook template")
    .action(async (id: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Fetching hook template "${id}"...`);
        const adapter = new AgentTemplateRegistryAdapter();
        const template = await adapter.getHookTemplate(id);

        router.render(template, {
          type: "detail",
          entity: "agent-hook-template",
          format: () => {
            if (!template) return `Hook template "${id}" not found.`;
            const lines: string[] = [];
            lines.push(`Hook Template: ${template.name ?? id}`);
            lines.push(`  Name: ${template.name}`);
            if ((template as any).hookType) {
              lines.push(`  Hook type: ${(template as any).hookType}`);
            }
            if ((template as any).category) {
              lines.push(`  Category: ${(template as any).category}`);
            }
            if ((template as any).description) {
              lines.push(`  Description: ${(template as any).description}`);
            }
            return lines.join("\n");
          },
          metadata: { id },
        });
      } catch (error) {
        handleError(error, { operation: "agent-hook-template-show", additionalInfo: { id } });
      }
    });

  // agent hook-template delete <id>
  hookTemplateCmd
    .command("delete <id>")
    .description("Delete an agent hook template")
    .action(async (id: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Deleting hook template "${id}"...`);
        const adapter = new AgentTemplateRegistryAdapter();
        await adapter.deleteHookTemplate(id);
        output.success(`Hook template "${id}" deleted.`);
      } catch (error) {
        handleError(error, { operation: "agent-hook-template-delete", additionalInfo: { id } });
      }
    });

  return hookTemplateCmd;
}
