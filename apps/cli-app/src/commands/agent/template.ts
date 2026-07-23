/**
 * Agent Template Registry Command Group
 * Subcommands for managing agent-level configuration templates
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
 * Create Agent Template Commands
 */
export function createAgentTemplateCommands(): Command {
  const templateCmd = new Command("template").description("Manage agent configuration templates");

  // agent template list
  templateCmd
    .command("list")
    .description("List all agent configuration templates")
    .option("-c, --category <category>", "Filter by category")
    .option("-t, --tags <tags>", "Filter by tags (comma separated)")
    .action(async (options: CommandOptions & { category?: string; tags?: string }) => {
      try {
        output.infoLog("Listing agent templates...");
        const adapter = new AgentTemplateRegistryAdapter();
        const filter: { category?: string; tags?: string[] } = {};
        if (options.category) filter.category = options.category;
        if (options.tags) filter.tags = options.tags.split(",").map((s) => s.trim());

        const templates = Object.keys(filter).length > 0
          ? (filter.tags
            ? await adapter.queryByTags(filter.tags)
            : await adapter.queryByCategory(filter.category!))
          : await adapter.listTemplates();

        router.render(templates, {
          type: "list",
          entity: "agent-template",
          format: () => {
            if (templates.length === 0) return "No agent templates found.";
            return templates
              .map(
                (t) =>
                  `  ${t.id} — ${t.templateName || "unnamed"} [${t.templateCategory ?? "uncategorized"}]`,
              )
              .join("\n");
          },
          metadata: { total: templates.length },
        });
      } catch (error) {
        handleError(error, { operation: "agent-template-list" });
      }
    });

  // agent template show <id>
  templateCmd
    .command("show <id>")
    .description("Show details of an agent configuration template")
    .action(async (id: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Fetching agent template "${id}"...`);
        const adapter = new AgentTemplateRegistryAdapter();
        const template = await adapter.getTemplate(id);

        router.render(template, {
          type: "detail",
          entity: "agent-template",
          format: () => {
            if (!template) return `Agent template "${id}" not found.`;
            const lines: string[] = [];
            lines.push(`Agent Template: ${template.templateName ?? id}`);
            lines.push(`  ID: ${template.id}`);
            lines.push(`  Category: ${template.templateCategory ?? "N/A"}`);
            if (template.templateTags?.length) {
              lines.push(`  Tags: ${template.templateTags.join(", ")}`);
            }
            lines.push(`  Usage count: ${template.usageCount ?? 0}`);
            lines.push(`  Public: ${template.isPublic ?? false}`);
            return lines.join("\n");
          },
          metadata: { id },
        });
      } catch (error) {
        handleError(error, { operation: "agent-template-show", additionalInfo: { id } });
      }
    });

  // agent template delete <id>
  templateCmd
    .command("delete <id>")
    .description("Delete an agent configuration template")
    .action(async (id: string, _options: CommandOptions) => {
      try {
        output.infoLog(`Deleting agent template "${id}"...`);
        const adapter = new AgentTemplateRegistryAdapter();
        await adapter.deleteTemplate(id);
        output.success(`Agent template "${id}" deleted.`);
      } catch (error) {
        handleError(error, { operation: "agent-template-delete", additionalInfo: { id } });
      }
    });

  return templateCmd;
}
