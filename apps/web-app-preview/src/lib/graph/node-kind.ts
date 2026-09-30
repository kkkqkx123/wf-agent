/**
 * Static workflow node types, mirrored from the backend `StaticNodeType`
 * enum so the canvas never emits a `node_type` the draft endpoint rejects.
 *
 * The backend deserializes `WorkflowDefinition.nodes[].node_type` through a
 * case-insensitive lookup over exactly these names; any other non-empty name
 * becomes a plugin-contributed type. Blank names are the only thing the
 * parser rejects, so "known builtin" and "usable" are two different checks.
 */
export const STATIC_NODE_TYPES = [
	'START',
	'END',
	'EMBED_START',
	'EMBED_END',
	'VARIABLE',
	'FORK',
	'JOIN',
	'SYNC',
	'SUBGRAPH',
	'EMBED_GRAPH',
	'SCRIPT',
	'INTERACTIVE_SCRIPT',
	'LLM',
	'TOOL_VISIBILITY',
	'USER_INTERACTION',
	'ROUTE',
	'CONTEXT_PROCESSOR',
	'LOOP_START',
	'LOOP_END',
	'AGENT_LOOP',
	'START_FROM_MESSAGE',
	'CONTINUE_FROM_MESSAGE',
] as const;

export type StaticNodeType = (typeof STATIC_NODE_TYPES)[number];

/** Node kind assigned to a node created without a template behind it. */
export const DEFAULT_NODE_TYPE: StaticNodeType = 'SCRIPT';

const BY_UPPERCASE = new Map<string, StaticNodeType>(
	STATIC_NODE_TYPES.map((type) => [type, type]),
);

/**
 * Resolve a node type string exactly the way the backend parser does:
 * case-insensitive, and null for anything outside the known set.
 */
export function parseStaticNodeType(value: string): StaticNodeType | null {
	return BY_UPPERCASE.get(value.trim().toUpperCase()) ?? null;
}

/**
 * Trim a raw node type string into exactly what the backend will store:
 * builtin names canonicalised to their SCREAMING_SNAKE_CASE variant, any other
 * name kept verbatim as a plugin-contributed type. Returns null only for blank
 * input, which is the single case the backend parser refuses.
 */
export function normalizeNodeType(value: string): string | null {
	const trimmed = value.trim();
	if (trimmed.length === 0) return null;
	return parseStaticNodeType(trimmed) ?? trimmed;
}

/**
 * Whether a node type is one of the builtin variants. Plugin-contributed types
 * are insertable but only execute once the plugin that owns them is installed.
 */
export function isBuiltinNodeType(value: string): boolean {
	return parseStaticNodeType(value) !== null;
}

/**
 * Node kinds the backend graph validator treats as graph entries; giving one
 * an incoming edge makes the draft fail validation.
 */
const ENTRY_NODE_TYPES = new Set<StaticNodeType>([
	'START',
	'START_FROM_MESSAGE',
]);

/**
 * Node kinds the backend graph validator treats as graph exits; an outgoing
 * edge from one makes the draft fail validation.
 */
const EXIT_NODE_TYPES = new Set<StaticNodeType>([
	'END',
	'CONTINUE_FROM_MESSAGE',
]);

export interface NodePorts {
	acceptsInput: boolean;
	emitsOutput: boolean;
}

/**
 * Port availability for a node kind, derived from the backend boundary
 * rules. Unrecognised kinds behave as ordinary middle nodes: the backend
 * rejects them on save, but the canvas still lets them be wired so the user
 * can see and fix the kind.
 */
export function nodePorts(kind: string): NodePorts {
	const type = parseStaticNodeType(kind);
	if (type === null) return { acceptsInput: true, emitsOutput: true };
	return {
		acceptsInput: !ENTRY_NODE_TYPES.has(type),
		emitsOutput: !EXIT_NODE_TYPES.has(type),
	};
}
