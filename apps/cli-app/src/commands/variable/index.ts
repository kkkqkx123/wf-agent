/**
 * Variable Command Group
 */

import { Command } from "commander";
import { VariableAdapter } from "../../adapters/variable-adapter.js";
import { getOutput } from "../../utils/output.js";
import { formatVariable, formatVariableList } from "../../utils/cli-formatters.js";
import type { CommandOptions } from "../../types/cli-types.js";
import { handleError } from "../../utils/error-handler.js";
import { CLIValidationError } from "../../types/cli-types.js";

const output = getOutput();

/**
 * Create Variable Command Group
 */
export function createVariableCommands(): Command {
  const variableCmd = new Command("variable").description("Manage Variables");

  // List variable commands
  variableCmd
    .command("list <thread-id>")
    .description("List all variables of the thread")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .action(async (threadId, options: CommandOptions) => {
      try {
        const adapter = new VariableAdapter();
        const variables = await adapter.listVariables(threadId);

        output.output(formatVariableList(variables, { table: options.table }));
      } catch (error) {
        handleError(error, {
          operation: "listVariables",
          additionalInfo: { threadId },
        });
      }
    });

  // View variable value command
  variableCmd
    .command("show <thread-id> <variable-name>")
    .description("View the variable value")
    .option("-v, --verbose", "Detailed output")
    .action(async (threadId, variableName, options: CommandOptions) => {
      try {
        const adapter = new VariableAdapter();
        const value = await adapter.getVariable(threadId, variableName);

        output.output(formatVariable(variableName, value, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "getVariable",
          additionalInfo: { threadId, variableName },
        });
      }
    });

  // Command to set variable values
  variableCmd
    .command("set <thread-id> <variable-name> <value>")
    .description("Set variable values")
    .option("-j, --json", "The value is in JSON format.")
    .action(async (threadId, variableName, value, options: { json?: boolean }) => {
      try {
        output.infoLog(`Setting variable: ${variableName}`);

        // Parse the value
        let parsedValue: any = value;
        if (options.json) {
          try {
            parsedValue = JSON.parse(value);
          } catch (error) {
            handleError(new CLIValidationError("The value must be in a valid JSON format."), {
              operation: "setVariable",
              additionalInfo: { threadId, variableName, value },
            });
            return;
          }
        }

        const adapter = new VariableAdapter();
        await adapter.setVariable(threadId, variableName, parsedValue);
      } catch (error) {
        handleError(error, {
          operation: "setVariable",
          additionalInfo: { threadId, variableName },
        });
      }
    });

  // Delete variable command
  variableCmd
    .command("delete <thread-id> <variable-name>")
    .description("Delete the variable")
    .option("-f, --force", "Forced deletion, without prompting for confirmation.")
    .action(async (threadId, variableName, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete variable: ${variableName}`);
          // In practical applications, interactive confirmation can be added here.
          output.infoLog("Use the --force option to skip confirmation.");
          return;
        }

        const adapter = new VariableAdapter();
        await adapter.deleteVariable(threadId, variableName);
      } catch (error) {
        handleError(error, {
          operation: "deleteVariable",
          additionalInfo: { threadId, variableName },
        });
      }
    });

  // Command to retrieve variable definitions
  variableCmd
    .command("definition <thread-id> <variable-name>")
    .description("Obtain variable definition information")
    .action(async (threadId, variableName) => {
      try {
        const adapter = new VariableAdapter();
        const definition = await adapter.getVariableDefinition(threadId, variableName);

        if (definition) {
          output.output(`\nVariable Definition:`);
          output.output(`  Name: ${definition.name}`);
          output.output(`  Type: ${definition.type}`);
          if (definition.description) {
            output.output(`  Description: ${definition.description}`);
          }
          if (definition.defaultValue !== undefined) {
            output.output(`  Default: ${JSON.stringify(definition.defaultValue)}`);
          }
          if (definition.required !== undefined) {
            output.output(`  Required: ${definition.required ? "Yes" : "No"}`);
          }
        } else {
          output.output("Variable definition not found.");
        }
      } catch (error) {
        handleError(error, {
          operation: "getVariableDefinition",
          additionalInfo: { threadId, variableName },
        });
      }
    });

  return variableCmd;
}
