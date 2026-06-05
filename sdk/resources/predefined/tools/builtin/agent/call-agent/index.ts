/**
 * Call Agent Tool Entry
 */

export { callAgentSchema } from "./schema.js";
export { CALL_AGENT_TOOL_DESCRIPTION, generateCallAgentDescription } from "./description.js";
export { createCallAgentHandler } from "./handler.js";
export type { AgentInfo, AgentHandlerConfig } from "./handler.js";
