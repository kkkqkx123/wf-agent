/**
 * TODO List Template
 *
 * Define the display format for the TODO list
 */

import type { PromptTemplate } from "../../types/template.js";

/**
 * TODO List Title Template
 */
export const TODO_LIST_HEADER_TEMPLATE: PromptTemplate = {
  id: "dynamic.todo-list.header",
  name: "TODO List Header",
  description: "TODO List statistics header",
  category: "dynamic",
  content: `Total: {{total}} | pending: {{pending}} | in_progress: {{inProgress}} | completed: {{completed}} | cancelled: {{cancelled}}`,
  variables: [
    { name: "total", type: "number", required: true, description: "Total count" },
    { name: "pending", type: "number", required: true, description: "Pending count" },
    { name: "inProgress", type: "number", required: true, description: "In progress count" },
    { name: "completed", type: "number", required: true, description: "Completed count" },
    { name: "cancelled", type: "number", required: true, description: "Cancelled count" },
  ],
};

/**
 * TODO Item Template
 */
export const TODO_ITEM_TEMPLATE: PromptTemplate = {
  id: "dynamic.todo-list.item",
  name: "TODO Item",
  description: "单个 TODO 项模板",
  category: "dynamic",
  content: "- [{{status}}] {{content}}  `#{{id}}`",
  variables: [
    { name: "status", type: "string", required: true, description: "TODO status" },
    { name: "content", type: "string", required: true, description: "TODO content" },
    { name: "id", type: "string", required: true, description: "TODO ID" },
  ],
};

/**
 * TODO List Truncation Prompt Template
 */
export const TODO_TRUNCATION_TEMPLATE: PromptTemplate = {
  id: "dynamic.todo-list.truncation",
  name: "TODO List Truncation",
  description: "TODO 列表截断提示",
  category: "dynamic",
  content: "... and {{count}} more items.",
  variables: [{ name: "count", type: "number", required: true, description: "Remaining count" }],
};
