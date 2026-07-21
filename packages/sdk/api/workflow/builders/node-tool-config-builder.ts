/**
 * NodeToolConfigBuilder - Fluent builder for workflow node tool configuration
 *
 * Provides a declarative API for building AgentToolConfig objects
 * for workflow agent-loop nodes. Uses the shared ToolConfigBuilder
 * from the shared builders module.
 *
 * Both Agent and Workflow domains use the same AgentToolConfig type,
 * so this builder is a domain-specific wrapper around the shared builder.
 */

export { ToolConfigBuilder as NodeToolConfigBuilder } from "../../shared/builders/tool-config-builder.js";