/**
 * Agent Profile Command Group
 */

import { Command } from "commander";
import { AgentProfileAdapter } from "../../adapters/agent-profile-adapter.js";
import { getOutput } from "../../utils/output.js";
import { handleError } from "../../utils/error-handler.js";

const output = getOutput();

/**
 * Create Agent Profile Command Group
 */
export function createAgentProfileCommands(): Command {
  const agentProfileCmd = new Command("agent-profile")
    .description("Manage agent profile metadata for call_agent tool discovery");

  // Register from file
  agentProfileCmd
    .command("register <file>")
    .description("Register agent profile metadata from configuration file")
    .action(async (file) => {
      try {
        output.infoLog(`Registering agent profile from file: ${file}`);
        const adapter = new AgentProfileAdapter();
        const profile = await adapter.registerFromFile(file);
        output.info(`Agent profile registered: ${profile.id}`);
        if (profile.description) {
          output.output(`  Description: ${profile.description}`);
        }
      } catch (error) {
        handleError(error, {
          operation: "registerAgentProfile",
          additionalInfo: { file },
        });
      }
    });

  // List profiles
  agentProfileCmd
    .command("list")
    .description("List all registered agent profiles")
    .action(async () => {
      try {
        const adapter = new AgentProfileAdapter();
        const profiles = await adapter.listProfiles();

        if (profiles.length === 0) {
          output.info("No agent profiles registered");
          return;
        }

        output.newLine();
        output.info("Registered Agent Profiles:");
        output.output("─".repeat(60));
        for (const p of profiles) {
          output.output(`  ${p.id}`);
          output.output(`    Name: ${p.name}`);
          if (p.description) {
            output.output(`    Description: ${p.description}`);
          }
          output.output("");
        }
        output.info(`Total: ${profiles.length} profile(s)`);
      } catch (error) {
        handleError(error, {
          operation: "listAgentProfiles",
        });
      }
    });

  // Show profile details
  agentProfileCmd
    .command("show <id>")
    .description("Show agent profile details by ID")
    .action(async (id) => {
      try {
        const adapter = new AgentProfileAdapter();
        const profile = await adapter.getProfile(id);

        output.newLine();
        output.info("Agent Profile Details:");
        output.output("─".repeat(40));
        output.output(`  ID:          ${profile.id}`);
        output.output(`  Name:        ${profile.name}`);
        output.output(`  Description: ${profile.description || "(none)"}`);
      } catch (error) {
        handleError(error, {
          operation: "showAgentProfile",
          additionalInfo: { profileId: id },
        });
      }
    });

  // Delete profile
  agentProfileCmd
    .command("delete <id>")
    .description("Delete an agent profile by ID")
    .option("--force", "Skip confirmation")
    .action(async (id, _options) => {
      try {
        const adapter = new AgentProfileAdapter();
        await adapter.deleteProfile(id);
        output.info(`Agent profile deleted: ${id}`);
      } catch (error) {
        handleError(error, {
          operation: "deleteAgentProfile",
          additionalInfo: { profileId: id },
        });
      }
    });

  return agentProfileCmd;
}