/**
 * User Context Builder
 *
 * Builds variable user context that changes frequently during execution:
 * - TODO lists and task status
 * - Pinned files content
 * - Workspace file updates
 * - Runtime state changes
 *
 * These fragments are appended to the last user message to avoid
 * invalidating the KV cache on the system message.
 */

import type { DynamicRuntimeContext } from "@wf-agent/types";
import { createContextualLogger } from "../../../utils/contextual-logger.js";

const logger = createContextualLogger({ component: "UserContextBuilder" });

/**
 * Build user context content
 *
 * Generates variable content for last user message.
 * This allows frequent updates without invalidating system message cache.
 *
 * @param context Runtime context with TODO list, pinned files, etc.
 * @returns User context content string
 */
export async function buildUserContextContent(
  context?: DynamicRuntimeContext,
): Promise<string> {
  if (!context) {
    return "";
  }

  const parts: string[] = [];

  // 1. TODO list injection
  if (context.todoList && context.todoList.length > 0) {
    const todoSection = [
      "## Current TODOs",
      ...context.todoList.map(
        (todo, index) => `${index + 1}. [${todo.status === "completed" ? "x" : " "}] ${todo.content}`,
      ),
    ].join("\n");
    parts.push(todoSection);
  }

  // 2. Pinned files content
  if (context.pinnedFiles && context.pinnedFiles.length > 0) {
    for (const pinnedFile of context.pinnedFiles) {
      try {
        const fileContent = await readFileContent(pinnedFile.path);
        if (fileContent !== null) {
          const fileSection = [
            `## File: ${pinnedFile.path}`,
            "```",
            fileContent,
            "```",
          ].join("\n");
          parts.push(fileSection);
        }
      } catch (error) {
        logger.debug("Failed to read pinned file", {
          path: pinnedFile.path,
          error,
        });
      }
    }
  }

  // 3. Workspace file tree
  if (context.workspaceFileTree) {
    const workspaceSection = [
      "## Workspace File Tree",
      "```",
      context.workspaceFileTree,
      "```",
    ].join("\n");
    parts.push(workspaceSection);
  }

  // 4. Current time
  if (context.currentTime) {
    const timeSection = `## Current Time\n${new Date(context.currentTime).toISOString()}`;
    parts.push(timeSection);
  }

  // 5. Custom data sections
  if (context.customData) {
    for (const [key, value] of Object.entries(context.customData)) {
      const customSection = `## ${key}\n${JSON.stringify(value, null, 2)}`;
      parts.push(customSection);
    }
  }

  const result = parts.join("\n\n");
  if (result) {
    logger.debug("Built user context content", { length: result.length });
  }
  return result;
}

/**
 * Read file content for pinned files
 */
async function readFileContent(path: string): Promise<string | null> {
  try {
    // Use dynamic import for fs to avoid issues in browser environments
    const fs = await import("fs/promises");
    const content = await fs.readFile(path, "utf-8");
    return content;
  } catch {
    return null;
  }
}
