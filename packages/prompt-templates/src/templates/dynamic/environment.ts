/**
 * Environmental information templates
 *
 * Purely static template definition for generating environmental information prompt words
 */

import type { PromptTemplate } from "../../types/template.js";

/**
 * Environmental information templates
 *
 * Variables:
 * - {{workspaceInfo}}: workspace information (single or multiple workspaces)
 * - {{os}}: Operating System
 * - {{timezone}}: Time zone
 * - {{userLanguage}}: User language
 */
export const ENVIRONMENT_TEMPLATE: PromptTemplate = {
  id: "dynamic.environment",
  name: "Environment Information",
  description:
    "Template for environment information (operating system, time zone, user language, workspace)",
  category: "dynamic",
  content: `## Environment

{{workspaceInfo}}
{{#if os}}
Operating System: {{os}}
{{/if}}
{{#if timezone}}
Timezone: {{timezone}}
{{/if}}
{{#if userLanguage}}
User Language: {{userLanguage}}
Please respond using the user's language by default.
{{/if}}`,
  variables: [
    { name: "workspaceInfo", type: "string", required: true, description: "Workspace information" },
    { name: "os", type: "string", required: false, description: "Operating system" },
    { name: "timezone", type: "string", required: false, description: "Timezone" },
    { name: "userLanguage", type: "string", required: false, description: "User language" },
  ],
};

/**
 * Single workspace template
 */
export const SINGLE_WORKSPACE_TEMPLATE: PromptTemplate = {
  id: "dynamic.environment.single-workspace",
  name: "Single Workspace",
  description: "Single workspace information template",
  category: "dynamic",
  content: "Current Workspace: {{workspacePath}}",
  variables: [
    { name: "workspacePath", type: "string", required: true, description: "Workspace path" },
  ],
};

/**
 * Multi-workspace templates
 */
export const MULTI_WORKSPACE_TEMPLATE: PromptTemplate = {
  id: "dynamic.environment.multi-workspace",
  name: "Multi-root Workspace",
  description: "Multi-workspace message templates",
  category: "dynamic",
  content: `Multi-root Workspace:
{{#each workspaces}}
  - {{this.name}}: {{this.path}}
{{/each}}

Use "workspace_name/path" format to access files in specific workspace.`,
  variables: [{ name: "workspaces", type: "array", required: true, description: "Workspace list" }],
};

/**
 * No workspace template
 */
export const NO_WORKSPACE_TEMPLATE: PromptTemplate = {
  id: "dynamic.environment.no-workspace",
  name: "No Workspace",
  description: "No workspace information template",
  category: "dynamic",
  content: "No workspace open",
  variables: [],
};
