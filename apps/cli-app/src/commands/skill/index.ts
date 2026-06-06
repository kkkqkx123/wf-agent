/**
 * Skill Command Group
 * Manage Skill viewing, loading and search
 */

import { Command } from "commander";
import { SkillAdapter } from "../../adapters/skill-adapter.js";
import { getOutput } from "../../utils/output.js";
import { getRouter } from "../../utils/output-router.js";
import { getFormatter } from "../../utils/formatter.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";
import { CLIValidationError } from "../../types/cli-types.js";
import type { SkillMatchResult, SkillMetadata, SkillResourceType } from "@wf-agent/types";

const output = getOutput();
const router = getRouter();

/**
 * Format Skill metadata
 */
function formatSkillMetadata(skill: SkillMetadata, verbose?: boolean, enabled?: boolean): string {
  const lines: string[] = [];

  lines.push(`\n${"─".repeat(60)}`);
  lines.push(`  Name: ${skill.name}`);
  lines.push(`  Description: ${skill.description}`);
  if (enabled !== undefined) {
    lines.push(`  Status: ${enabled ? "enabled" : "disabled"}`);
  }

  if (verbose) {
    if (skill.version) {
      lines.push(`  Version: ${skill.version}`);
    }
    if (skill.license) {
      lines.push(`  License: ${skill.license}`);
    }
    if (skill.allowedTools && skill.allowedTools.length > 0) {
      lines.push(`  Allowed Tools: ${skill.allowedTools.join(", ")}`);
    }
    if (skill.metadata) {
      lines.push(`  Metadata: ${JSON.stringify(skill.metadata, null, 2)}`);
    }
  }

  lines.push("─".repeat(60));

  return lines.join("\n");
}

/**
 * Format Skill list
 */
function formatSkillList(skills: SkillMetadata[], options?: { table?: boolean }): string {
  if (skills.length === 0) {
    return "No Skill found";
  }

  if (options?.table) {
    const lines: string[] = [];
    lines.push("\n Name | Description | Version");
    lines.push("-".repeat(60));

    for (const skill of skills) {
      const desc =
        skill.description.length > 40
          ? skill.description.substring(0, 40) + "..."
          : skill.description;
      const version = skill.version || "-";
      lines.push(`${skill.name} | ${desc} | ${version}`);
    }

    return lines.join("\n");
  }

  const lines: string[] = [`Found ${skills.length} skills:`];

  for (const skill of skills) {
    lines.push(`  • ${skill.name}`);
    lines.push(`    ${skill.description}`);
    if (skill.version) {
      lines.push(`    (v${skill.version})`);
    }
    lines.push("");
  }

  return lines.join("\n");
}

/**
 * Format match results
 */
function formatMatchResults(results: SkillMatchResult[]): string {
  if (results.length === 0) {
    return "No matching Skill found";
  }

  const lines: string[] = [`Found ${results.length} matching skills:`];

  for (const result of results) {
    lines.push(`  • ${result.skill.name} (Score: ${result.score.toFixed(2)})`);
    lines.push(`    ${result.skill.description}`);
    lines.push(`    Match reason: ${result.reason}`);
    lines.push("");
  }

  return lines.join("\n");
}

/**
 * Create Skill Command Group
 */
export function createSkillCommands(): Command {
  const skillCmd = new Command("skill").description("Manage Skill");

  // List all Skill commands
  skillCmd
    .command("list")
    .description("List all available Skills")
    .option("-n, --name <name>", "Filter by name")
    .option("-t, --table", "Output in tabular format")
    .option("-v, --verbose", "Detailed output")
    .action(
      async (options: CommandOptions & { name?: string; table?: boolean; verbose?: boolean }) => {
        try {
          const adapter = new SkillAdapter();

          const filter = options.name ? { name: options.name } : undefined;
          const skills = await adapter.listSkills(filter);

          // Pre-resolve verbose data outside the sync format callback
          let enabledNames: Set<string> | undefined;
          if (options.verbose) {
            const enabledSkills = await adapter.getEnabledSkills();
            enabledNames = new Set(enabledSkills.map(s => s.name));
          }

          router.render(skills, {
            type: "list",
            entity: "skill",
            format: () => {
              let text = formatSkillList(skills, { table: options.table }) + "\n";
              if (options.verbose && enabledNames) {
                text += getFormatter().subsection("Detailed list.") + "\n";
                for (const skill of skills) {
                  text += formatSkillMetadata(skill, true, enabledNames.has(skill.name)) + "\n";
                }
              }
              return text;
            },
            metadata: { total: skills.length },
          });
        } catch (error) {
          handleError(error, {
            operation: "listSkills",
            additionalInfo: { filter: { name: options.name } },
          });
        }
      },
    );

  // View Skill Details command
  skillCmd
    .command("show <name>")
    .description("View Skill details")
    .option("-v, --verbose", "Detailed output")
    .option("-c, --content", "Show full content")
    .action(async (name, options: CommandOptions & { verbose?: boolean; content?: boolean }) => {
      try {
        const adapter = new SkillAdapter();

        const skill = await adapter.getSkill(name);

        if (!skill) {
          handleError(new CLIValidationError(`Skill not found: ${name}`), {
            operation: "getSkill",
            additionalInfo: { name },
          });
          return;
        }

        const enabledSkills = await adapter.getEnabledSkills();
        const isEnabled = enabledSkills.some(s => s.name === name);
        output.output(formatSkillMetadata(skill, options.verbose, isEnabled));

        if (options.content) {
          output.newLine();
          output.output(getFormatter().subsection("Full Content."));
          output.output("─".repeat(60));
          const content = await adapter.loadContent(name);
          output.output(content);
          output.output("─".repeat(60));
        }
      } catch (error) {
        handleError(error, {
          operation: "getSkill",
          additionalInfo: { name },
        });
      }
    });

  // The Load Skill Content command
  skillCmd
    .command("load <name>")
    .description("Load Skill full content")
    .option("-p, --prompt", "Convert to prompt format")
    .action(async (name, options: CommandOptions & { prompt?: boolean }) => {
      try {
        const adapter = new SkillAdapter();

        if (options.prompt) {
          const prompt = await adapter.toPrompt(name);
          output.output(prompt);
        } else {
          const content = await adapter.loadContent(name);
          output.output(content);
        }
      } catch (error) {
        handleError(error, {
          operation: "loadSkill",
          additionalInfo: { name, toPrompt: options.prompt },
        });
      }
    });

  // Search Skill commands
  skillCmd
    .command("search <query>")
    .description("Search Skills by description")
    .action(async query => {
      try {
        const adapter = new SkillAdapter();
        const results = await adapter.matchSkills(query);

        output.output(formatMatchResults(results));
      } catch (error) {
        handleError(error, {
          operation: "matchSkills",
          additionalInfo: { query },
        });
      }
    });

  // List Skill Resources command
  skillCmd
    .command("resources <name>")
    .description("List Skill's resources")
    .option("-t, --type <type>", "Resource type (references|examples|scripts|assets)")
    .action(async (name, options: CommandOptions & { type?: string }) => {
      try {
        const adapter = new SkillAdapter();

        const resourceType = (options.type || "scripts") as SkillResourceType;

        const resources = await adapter.listResources(name, resourceType);

        if (resources.length === 0) {
          output.output(`No resources of type ${resourceType} for skill ${name}`);
          return;
        }

        output.newLine();
        output.output(getFormatter().subsection(`Resources (${resourceType}) for ${name}:`));
        output.output("─".repeat(40));

        for (const resource of resources) {
          output.output(`  • ${resource}`);
        }
      } catch (error) {
        handleError(error, {
          operation: "listSkillResources",
          additionalInfo: { name, type: options.type },
        });
      }
    });

  // Reload Skill Command
  skillCmd
    .command("reload")
    .description("Reload all Skills")
    .option("-d, --dir <directory>", "Skill directory path")
    .action(async (options: CommandOptions & { dir?: string }) => {
      try {
        const adapter = new SkillAdapter();

        if (options.dir) {
          await adapter.initialize(options.dir);
        } else {
          await adapter.reload();
        }

        output.newLine();
        output.info("Skills reloaded");
      } catch (error) {
        handleError(error, {
          operation: "reloadSkills",
          additionalInfo: { dir: options.dir },
        });
      }
    });

  // Clear Cache Command
  skillCmd
    .command("clear-cache")
    .description("Clear Skill cache")
    .option("-n, --name <name>", "Specify the Skill name to be cleared")
    .action(async (options: CommandOptions & { name?: string }) => {
      try {
        const adapter = new SkillAdapter();
        adapter.clearCache(options.name);

        output.newLine();
        output.info("Cache cleared");
      } catch (error) {
        handleError(error, {
          operation: "clearSkillCache",
          additionalInfo: { name: options.name },
        });
      }
    });

  // Enable Skill command
  skillCmd
    .command("enable <name>")
    .description("Enable a Skill")
    .action(async name => {
      try {
        const adapter = new SkillAdapter();
        await adapter.enable(name);

        output.newLine();
        output.info(`Skill enabled: ${name}`);
      } catch (error) {
        handleError(error, {
          operation: "enableSkill",
          additionalInfo: { name },
        });
      }
    });

  // Disable Skill command
  skillCmd
    .command("disable <name>")
    .description("Disable a Skill")
    .action(async name => {
      try {
        const adapter = new SkillAdapter();
        await adapter.disable(name);

        output.newLine();
        output.info(`Skill disabled: ${name}`);
      } catch (error) {
        handleError(error, {
          operation: "disableSkill",
          additionalInfo: { name },
        });
      }
    });

  // List enabled/disabled skills command
  skillCmd
    .command("status")
    .description("List enabled and disabled skills")
    .option("-t, --table", "Output in tabular format")
    .action(async (options: CommandOptions & { table?: boolean }) => {
      try {
        const adapter = new SkillAdapter();
        const enabled = await adapter.getEnabledSkills();
        const disabled = await adapter.getDisabledSkills();

        router.render({ enabled, disabled }, {
          type: "list",
          entity: "skill",
          format: () => {
            let text = "\n";
            text += getFormatter().subsection("Enabled Skills") + "\n";
            text += formatSkillList(enabled, { table: options.table }) + "\n";
            if (disabled.length > 0) {
              text += "\n" + getFormatter().subsection("Disabled Skills") + "\n";
              text += formatSkillList(disabled, { table: options.table }) + "\n";
            }
            return text;
          },
          metadata: { enabled: enabled.length, disabled: disabled.length },
        });
      } catch (error) {
        handleError(error, {
          operation: "listSkillStatus",
        });
      }
    });

  // Generate metadata prompt command
  skillCmd
    .command("metadata-prompt")
    .description("Generate Skill metadata prompts (for system prompts)")
    .action(async () => {
      try {
        const adapter = new SkillAdapter();
        const prompt = adapter.generateMetadataPrompt();

        if (prompt) {
          output.output(prompt);
        } else {
          output.output("No Skill available");
        }
      } catch (error) {
        handleError(error, {
          operation: "generateMetadataPrompt",
        });
      }
    });

  // Initializing Skill Commands
  skillCmd
    .command("init <directory>")
    .description("Initialize the Skill directory")
    .action(async directory => {
      try {
        const adapter = new SkillAdapter();
        await adapter.initialize(directory);

        output.newLine();
        output.info("Skill directory initialized");
        output.output(getFormatter().keyValue("Directory", directory));
      } catch (error) {
        handleError(error, {
          operation: "initializeSkillDirectory",
          additionalInfo: { directory },
        });
      }
    });

  return skillCmd;
}
