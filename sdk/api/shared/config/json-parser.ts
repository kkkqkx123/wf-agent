/**
 * JSON Parser
 * Responsible for parsing configuration files in JSON format
 */

import type { WorkflowConfigFile } from "./types.js";
import { ConfigurationError } from "@wf-agent/types";
import { isError } from "@wf-agent/common-utils";

/**
 * Parse JSON content
 * @param content A JSON content string
 * @returns The parsed configuration object
 * @throws {ConfigurationError} Throws an error when JSON parsing fails or the format is incorrect
 */
export function parseJson(content: string): WorkflowConfigFile {
  try {
    const parsed = JSON.parse(content);

    // Verify required fields
    if (!parsed.id) {
      throw new ConfigurationError("The JSON configuration file must contain the id field", "id");
    }

    if (!parsed.name) {
      throw new ConfigurationError(
        "The JSON configuration file must contain the name field",
        "name",
      );
    }

    if (!parsed.version) {
      throw new ConfigurationError(
        "The JSON configuration file must contain the version field",
        "version",
      );
    }

    return parsed as WorkflowConfigFile;
  } catch (error) {
    if (error instanceof ConfigurationError) {
      throw error;
    }
    if (isError(error)) {
      throw new ConfigurationError(`JSON parsing failed: ${error.message}`, undefined, {
        originalError: error.message,
      });
    }
    throw new ConfigurationError("JSON parsing failed: unknown error");
  }
}

/**
 * Convert the configuration object to a JSON string
 * @param config The configuration object
 * @param pretty Whether to format the output
 * @returns A JSON string
 * @throws {ConfigurationError} Throws an error if JSON serialization fails
 */
export function stringifyJson(config: unknown, pretty: boolean = true): string {
  try {
    if (pretty) {
      return JSON.stringify(config, null, 2);
    }
    return JSON.stringify(config);
  } catch (error) {
    if (error instanceof Error) {
      throw new ConfigurationError(`JSON serialization failed ${error.message}`, undefined, {
        originalError: error.message,
      });
    }
    throw new ConfigurationError("JSON serialization failed: unknown error");
  }
}

/**
 * Verify the basic format of JSON content (only check JSON syntax, do not validate required fields)
 * @param content JSON content as a string
 * @returns Whether it is valid
 */
export function validateJsonSyntax(content: string): boolean {
  try {
    JSON.parse(content);
    return true;
  } catch {
    return false;
  }
}
