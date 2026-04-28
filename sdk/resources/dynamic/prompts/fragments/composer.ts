/**
 * Dynamic Context Combiner
 *
 * Combines all dynamic fragments into a complete dynamic context content
 */

import type { DynamicContextConfig, DynamicRuntimeContext } from "@wf-agent/types";
import { cleanupEmptyLines } from "./utils.js";
import { generateCurrentTimeContent } from "./current-time.js";
import { generateTodoListContent } from "./todo-list.js";
import { generateWorkspaceFilesContent } from "./workspace-files.js";
import { generatePinnedFilesContent } from "./pinned-files.js";
import { generateSkillsContent } from "./skills.js";

/**
 * Generate the complete dynamic context content
 *
 * @param config Dynamic context configuration
 * @param runtime Runtime context data
 * @returns String of dynamic context content
 */
export function generateDynamicContextContent(
  config: DynamicContextConfig,
  runtime?: DynamicRuntimeContext,
): string {
  const sections: string[] = [];

  // Prefix Explanation
  sections.push(
    "This is the current turn's dynamic context information you can use. " +
      "It may change between turns. Continue with the previous task if the information is not needed and ignore it.",
  );

  // Current time
  if (config.includeCurrentTime !== false) {
    sections.push(generateCurrentTimeContent());
  }

  // TODO list
  if (config.includeTodoList && runtime?.todoList) {
    const todoContent = generateTodoListContent(runtime.todoList);
    if (todoContent) sections.push(todoContent);
  }

  // Workspace file tree
  if (config.includeWorkspaceFiles && runtime?.workspaceFileTree) {
    const fileContent = generateWorkspaceFilesContent(
      runtime.workspaceFileTree,
      config.maxFileDepth,
      config.ignorePatterns,
    );
    if (fileContent) sections.push(fileContent);
  }

  // Fix the file
  if (config.includePinnedFiles && runtime?.pinnedFiles) {
    const pinnedContent = generatePinnedFilesContent(runtime.pinnedFiles);
    if (pinnedContent) sections.push(pinnedContent);
  }

  // Skills
  if (config.includeSkills && runtime?.skills) {
    const skillsContent = generateSkillsContent(runtime.skills);
    if (skillsContent) sections.push(skillsContent);
  }

  return cleanupEmptyLines(sections.join("\n\n"));
}

/**
 * Check if there is any dynamic content.
 */
export function hasDynamicContent(config: DynamicContextConfig): boolean {
  return (
    config.includeCurrentTime !== false ||
    config.includeTodoList === true ||
    config.includeWorkspaceFiles === true ||
    config.includePinnedFiles === true ||
    config.includeSkills === true
  );
}
