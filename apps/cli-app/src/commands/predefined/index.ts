/**
 * Predefined Resources Command Group
 * List and manage predefined resources
 */

import { Command } from "commander";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { getFormatter } from "../../utils/formatter.js";
import { handleError } from "../../utils/error-handler.js";
import type { CommandOptions } from "../../types/cli-types.js";

const output = getOutput();
const router = getRouter();

/**
 * Create Predefined Resources Command Group
 */
export function createPredefinedCommands(): Command {
  const predefinedCmd = new Command("predefined").description("Manage predefined resources");

  // List predefined tool IDs
  predefinedCmd
    .command("list-tools")
    .description("List all predefined tool IDs")
    .option("-t, --table", "Output in table format")
    .action(async (options: CommandOptions) => {
      try {
        // List tools from the SDK tool registry
        const { getSDKInstance } = await import("../../services/sdk-globals.js");
        const sdk = getSDKInstance();
        if (!sdk) throw new Error("SDK not available");
        const tools = await sdk.tools.getAll();

        const toolIds = tools.map((t: any) => t.id || t.name || "unknown");

        router.render(toolIds, {
          type: "list",
          entity: "predefined-tool",
          format: () => {
            if (toolIds.length === 0) return "No predefined tools found";
            if (options.table) {
              const formatter = getFormatter();
              const headers = ["Tool ID"];
              const rows = toolIds.map((id: string) => [id]);
              return formatter.table(headers, rows);
            }
            return toolIds.map((id: string) => `  ${id}`).join("\n");
          },
          metadata: { total: toolIds.length },
        });
      } catch (error) {
        handleError(error, { operation: "predefined-list-tools" });
      }
    });

  // Register predefined content
  predefinedCmd
    .command("register")
    .description("Register all predefined content")
    .action(async () => {
      try {
        // Dynamically access the SDK's registries
        const { getSDKInstance } = await import("../../services/sdk-globals.js");
        const sdk = getSDKInstance();
        if (!sdk) throw new Error("SDK not available");

        const deps = sdk.getFactory().getDependencies();
        const triggerRegistry = deps.getTriggerTemplateRegistry();
        const workflowRegistry = deps.getWorkflowRegistry();
        const toolService = deps.getToolService();

        const { registerPredefinedContent } = await import("@wf-agent/sdk/resources");
        await registerPredefinedContent(triggerRegistry, workflowRegistry, toolService);
        output.success("Predefined content registered successfully");
      } catch (error) {
        handleError(error, { operation: "predefined-register" });
      }
    });

  // Unregister predefined content
  predefinedCmd
    .command("unregister")
    .description("Unregister all predefined content")
    .action(async () => {
      try {
        const { getSDKInstance } = await import("../../services/sdk-globals.js");
        const sdk = getSDKInstance();
        if (!sdk) throw new Error("SDK not available");

        const deps = sdk.getFactory().getDependencies();
        const triggerRegistry = deps.getTriggerTemplateRegistry();
        const workflowRegistry = deps.getWorkflowRegistry();
        const toolService = deps.getToolService();

        const { unregisterPredefinedContent } = await import("@wf-agent/sdk/resources");
        await unregisterPredefinedContent(triggerRegistry, workflowRegistry, toolService);
        output.success("Predefined content unregistered successfully");
      } catch (error) {
        handleError(error, { operation: "predefined-unregister" });
      }
    });

  return predefinedCmd;
}