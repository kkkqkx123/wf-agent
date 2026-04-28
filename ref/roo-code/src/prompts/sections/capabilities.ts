import { McpHub } from "../../../services/mcp/McpHub"

export function getCapabilitiesSection(cwd: string, mcpHub?: McpHub): string {
	return `---

CAPABILITIES

You have access to the following tools to accomplish tasks:
- execute_command: Run CLI commands (provide explanation of what the command does). Supports interactive and long-running commands, each in a new terminal
- list_files: List files. Use recursive=true for recursive listing, otherwise top-level only
- Other tools: view source definitions, regex search, read/write files, ask follow-up questions
Initially, you'll receive a recursive list of all files in the current workspace (${cwd}) to help understand the project structure.${mcpHub
			? `
- You have access to MCP servers that may provide additional tools and resources. Each server may provide different capabilities that you can use to accomplish tasks more effectively.
`
			: ""
		}`
}
