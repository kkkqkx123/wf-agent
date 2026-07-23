/**
 * Sandbox Command Group
 * Commands for inspecting sandbox security policies and runtime status
 */

import { Command } from "commander";
import { SandboxAdapter } from "../../adapters/sandbox-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { handleError } from "../../utils/error-handler.js";

const output = getOutput();
const router = getRouter();

/**
 * Create Sandbox Command Group
 */
export function createSandboxCommands(): Command {
  const sandboxCmd = new Command("sandbox").description(
    "Inspect sandbox security policies and runtime status",
  );

  // --- sandbox status ---
  sandboxCmd
    .command("status")
    .description("Show sandbox runtime status")
    .action(async () => {
      try {
        const adapter = new SandboxAdapter();
        const status = await adapter.getStatus();

        router.render(status, {
          type: "detail",
          entity: "sandbox",
          format: () => JSON.stringify(status, null, 2),
        });
      } catch (error) {
        handleError(error, { operation: "sandbox-status" });
      }
    });

  // --- sandbox presets ---
  sandboxCmd
    .command("presets")
    .description("Show available shell policy presets (SAFE / BALANCED / PERMISSIVE)")
    .action(async () => {
      try {
        const adapter = new SandboxAdapter();
        const presets = await adapter.getPresets();

        router.render(presets, {
          type: "detail",
          entity: "sandbox-presets",
          format: () => JSON.stringify(presets, null, 2),
        });
      } catch (error) {
        handleError(error, { operation: "sandbox-presets" });
      }
    });

  // --- sandbox strategies ---
  sandboxCmd
    .command("strategies")
    .description("List available sandbox strategies")
    .action(async () => {
      try {
        const adapter = new SandboxAdapter();
        const strategies = await adapter.getStrategies();

        router.render(strategies, {
          type: "list",
          entity: "sandbox-strategy",
          format: () => strategies.join("\n"),
          metadata: { total: strategies.length },
        });
      } catch (error) {
        handleError(error, { operation: "sandbox-strategies" });
      }
    });

  // Subcommand: sandbox policy
  const policyCmd = new Command("policy").description("Show sandbox security policy details");

  // --- sandbox policy show <type> ---
  policyCmd
    .command("show <type>")
    .description("Show policy details (shell | python | js | all)")
    .action(async (type: string) => {
      try {
        const adapter = new SandboxAdapter();
        let result: Record<string, unknown>;

        switch (type.toLowerCase()) {
          case "shell":
            result = await adapter.getShellPolicy();
            break;
          case "python":
            result = await adapter.getPythonPolicy();
            break;
          case "js":
          case "javascript":
            result = await adapter.getJsPolicy();
            break;
          case "all":
            result = await adapter.getDefaultPolicy();
            break;
          default:
            output.warnLog(`Unknown policy type: "${type}". Use: shell, python, js, or all`);
            return;
        }

        router.render(result, {
          type: "detail",
          entity: `sandbox-policy-${type}`,
          format: () => JSON.stringify(result, null, 2),
        });
      } catch (error) {
        handleError(error, {
          operation: "sandbox-policy-show",
          additionalInfo: { type },
        });
      }
    });

  sandboxCmd.addCommand(policyCmd);

  return sandboxCmd;
}
