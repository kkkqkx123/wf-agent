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
    .command("list <execution-id>")
    .description("List all variables of the workflow execution")
    .option("-t, --table", "Output in table format:")
    .option("-v, --verbose", "Detailed output")
    .action(async (executionId, options: CommandOptions) => {
      try {
        const adapter = new VariableAdapter();
        const variables = await adapter.listVariables(executionId);

        output.output(formatVariableList(variables, { table: options.table }));
      } catch (error) {
        handleError(error, {
          operation: "listVariables",
          additionalInfo: { executionId },
        });
      }
    });

  // View variable value command
  variableCmd
    .command("show <execution-id> <variable-name>")
    .description("View the variable value")
    .option("-v, --verbose", "Detailed output")
    .action(async (executionId, variableName, options: CommandOptions) => {
      try {
        const adapter = new VariableAdapter();
        const value = await adapter.getVariable(executionId, variableName);

        output.output(formatVariable(variableName, value, { verbose: options.verbose }));
      } catch (error) {
        handleError(error, {
          operation: "getVariable",
          additionalInfo: { executionId, variableName },
        });
      }
    });

  // Command to set variable values
  variableCmd
    .command("set <execution-id> <variable-name> <value>")
    .description("Set variable values")
    .option("-j, --json", "The value is in JSON format.")
    .action(async (executionId, variableName, value, options: { json?: boolean }) => {
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
              additionalInfo: { executionId, variableName, value },
            });
            return;
          }
        }

        const adapter = new VariableAdapter();
        await adapter.setVariable(executionId, variableName, parsedValue);
      } catch (error) {
        handleError(error, {
          operation: "setVariable",
          additionalInfo: { executionId, variableName },
        });
      }
    });

  // Delete variable command
  variableCmd
    .command("delete <execution-id> <variable-name>")
    .description("Delete the variable")
    .option("-f, --force", "Forced deletion, without prompting for confirmation.")
    .action(async (executionId, variableName, options: { force?: boolean }) => {
      try {
        if (!options.force) {
          output.warnLog(`About to delete variable: ${variableName}`);
          // In practical applications, interactive confirmation can be added here.
          output.infoLog("Use the --force option to skip confirmation.");
          return;
        }

        const adapter = new VariableAdapter();
        await adapter.deleteVariable(executionId, variableName);
      } catch (error) {
        handleError(error, {
          operation: "deleteVariable",
          additionalInfo: { executionId, variableName },
        });
      }
    });

  // Command to retrieve variable definitions
  variableCmd
    .command("definition <execution-id> <variable-name>")
    .description("Obtain variable definition information")
    .action(async (executionId, variableName) => {
      try {
        const adapter = new VariableAdapter();
        const definition = await adapter.getVariableDefinition(executionId, variableName);

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
          additionalInfo: { executionId, variableName },
        });
      }
    });

  return variableCmd;
}
