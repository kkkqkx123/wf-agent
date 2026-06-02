# MCP Integration Analysis

## Overview

The Modular Agent Framework integrates MCP (Model Context Protocol) as a core extension mechanism for adding external capabilities to the agent system. MCP allows the agent to interact with external services through standardized protocols for tool execution and resource access.

## Architecture

The MCP integration follows a layered architecture:

### 1. MCP Client (mcp-client.ts)
- Implements the MCP protocol as a JSON-RPC 2.0 client
- Manages connection lifecycle and request/response handling
- Provides methods for:
  - `connect()`: Establishes connection with server
  - `listTools()`: Retrieves available tools from server
  - `callTool()`: Executes remote tools
  - `listResources()`: Lists available resources
  - `readResource()`: Reads resource content

### 2. Connection Manager (connection-manager.ts)
- Manages multiple MCP server connections
- Tracks server states (connecting, connected, disconnected)
- Handles connection timeouts and error recovery
- Provides event-driven notifications for state changes

### 3. Server Registry (server-registry.ts)
- Implements singleton pattern for global MCP access
- Provides `getMcpManager()` for centralized access
- Manages connection pooling and lifecycle
- Supports client info and configuration options

### 4. Configuration System (config/)
- Defines configuration schema with Zod validation
- Supports three transport types:
  - `stdio`: Local executable (command + args)
  - `sse`: Server-Sent Events endpoint
  - `streamable-http`: HTTP streaming endpoint
- Configuration files:
  - `mcp-settings.json` (global)
  - `.agent/mcp.json` (project-specific)
- Validation ensures proper field combinations for each transport type

## Integration with System Prompt System

The MCP integration is properly integrated into the system prompt system through:

### 1. Tool Description Registration
- The `use_mcp` tool is registered in `tool-description-registry.ts`
- Contains comprehensive description with:
  - Category: "code"
  - Detailed usage instructions
  - Parameter definitions (server_name, tool_name, arguments, uri)
  - Tips for proper usage

### 2. System Prompt Generation
- The `system-prompt-builder.ts` includes `use_mcp` in the tool availability section
- Generated prompts include:
  - Tool availability section with descriptions
  - Tool usage rules
  - Clear instructions on when to use MCP tools

### 3. Skill Integration
- MCP tools are treated as standard tools in the system
- When skills are injected into system prompts via `injectSkillMetadata()`, MCP tools appear alongside other tools
- The system prompt template includes a "Tool Usage Rules" section that applies to MCP tools

### 4. Dynamic Tool Discovery
- The `use_mcp` tool allows agents to discover and use tools from connected MCP servers
- When an agent calls `use_mcp`, it dynamically accesses:
  - Tools available on the specified server
  - Resources available on the specified server
- This enables dynamic capability expansion without requiring static tool definitions

## Tool Definition

The `use_mcp` tool is defined with:

### Schema (use-mcp/schema.ts)
```json
{
  "type": "object",
  "properties": {
    "server_name": {"type": "string", "required": true},
    "tool_name": {"type": "string", "required": false},
    "arguments": {"type": "object", "required": false},
    "uri": {"type": "string", "required": false}
  },
  "required": ["server_name"]
}
```

### Description (use-mcp/description.ts)
- Clearly explains the two modes of operation:
  - Tool calls: `server_name` + `tool_name` + `arguments`
  - Resource access: `server_name` + `uri`
- Specifies that MCP servers must be configured and connected first
- Provides guidance on when to use this tool

### Handler (use-mcp/handler.ts)
- Validates server existence and connection status
- Handles both tool calls and resource access
- Provides meaningful error messages for misconfigurations
- Includes fallback behavior when MCP manager is unavailable
