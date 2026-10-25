import { backendEdgeType } from '#lib/graph/display-model';
import type { DisplayEdge, DisplayNode } from '#lib/graph/display-model';
import { asRecordList } from '#lib/services/graph';
import type { KeyValue, TemplateKind } from '#lib/types/models';

function field(record: Record<string, unknown>, key: string, fallback: string): string {
	const value = record[key];
	return typeof value === 'string' && value !== '' ? value : fallback;
}

/** Definition-level object a form edits, unwrapped for library kinds. */
export function templateEditTarget(kind: TemplateKind, value: unknown): unknown {
	if (kind === 'node' || kind === 'trigger') return value;
	return (value as Record<string, unknown>)?.definition ?? value;
}

export interface TemplateTopology {
	nodes: DisplayNode[];
	edges: DisplayEdge[];
}

/** Parsed workflow topology from a stored value with canonical fields. */
export function parseTemplateTopology(value: unknown): TemplateTopology {
	const target =
		value && typeof value === 'object' && !Array.isArray(value)
			? (value as Record<string, unknown>).definition &&
				typeof (value as Record<string, unknown>).definition === 'object'
				? ((value as Record<string, unknown>).definition as Record<string, unknown>)
				: (value as Record<string, unknown>)
			: {};
	return {
		nodes: asRecordList(target.nodes).map((node, index) => ({
			id: field(node, 'id', `node-${index}`),
			label: field(node, 'name', field(node, 'id', `node-${index}`)),
			kind: field(node, 'node_type', 'STEP'),
		})),
		edges: asRecordList(target.edges)
			.map((edge, index) => ({
				id: field(edge, 'id', `edge-${index}`),
				source: field(edge, 'source_node_id', ''),
				target: field(edge, 'target_node_id', ''),
				label: typeof edge.condition === 'string' && edge.condition ? edge.condition : undefined,
				kind: field(edge, 'type', 'DEFAULT'),
			}))
			.filter((edge) => edge.source && edge.target),
	};
}

/** Deterministic node id within one editing session. */
export function allocateGraphNodeId(existing: Set<string>): string {
	let counter = existing.size + 1;
	let candidate = `node-${counter}`;
	while (existing.has(candidate)) {
		counter += 1;
		candidate = `node-${counter}`;
	}
	return candidate;
}

/**
 * Merge graph store state back into a stored template value. Only node and
 * edge arrays are replaced, and unknown entry fields survive by matching ids.
 */
export function mergeTemplateGraph(
	value: unknown,
	nodes: DisplayNode[],
	edges: DisplayEdge[],
): unknown {
	if (!value || typeof value !== 'object' || Array.isArray(value)) return value;
	const record = value as Record<string, unknown>;
	const hasDefinition =
		record.definition && typeof record.definition === 'object' && !Array.isArray(record.definition);
	const target = (hasDefinition ? record.definition : record) as Record<string, unknown>;
	const prevNodes = asRecordList(target.nodes);
	const prevEdges = asRecordList(target.edges);
	const nodeById = new Map(prevNodes.map((node) => [field(node, 'id', ''), node]));
	const edgeById = new Map(prevEdges.map((edge) => [field(edge, 'id', ''), edge]));
	const nextNodes = nodes.map((node, index) => {
		const prev = nodeById.get(node.id) ?? {};
		return {
			...prev,
			id: node.id || field(prev, 'id', `node-${index}`),
			node_type: node.kind,
			name: node.label,
		};
	});
	const nextEdges = edges.map((edge, index) => {
		const prev = edgeById.get(edge.id) ?? {};
		return {
			...prev,
			id: edge.id || field(prev, 'id', `edge-${index}`),
			source_node_id: edge.source,
			target_node_id: edge.target,
			type: backendEdgeType(edge.kind ?? 'DEFAULT'),
			...(edge.label ? { condition: edge.label } : {}),
		};
	});
	const nextTarget = { ...target, nodes: nextNodes, edges: nextEdges };
	if (hasDefinition) return { ...record, definition: nextTarget };
	return { ...record, ...nextTarget };
}

/** Backend-shaped workflow definition for draft save and validation. */
export function templateBackendDefinition(
	kind: TemplateKind,
	value: unknown,
	fallbackId: string,
	fallbackName: string,
): Record<string, unknown> {
	const target = templateEditTarget(kind, value);
	const record =
		target && typeof target === 'object' && !Array.isArray(target)
			? (target as Record<string, unknown>)
			: {};
	const nodes = asRecordList(record.nodes);
	if (nodes.length === 0) throw new Error('No nodes array in definition');
	return {
		id: fallbackId,
		name: field(record, 'name', '') || fallbackName,
		nodes: nodes.map((node, index) => ({
			...node,
			id: field(node, 'id', `node-${index}`),
			node_type: field(node, 'node_type', 'STEP'),
			name: field(node, 'name', field(node, 'id', `node-${index}`)),
		})),
		edges: asRecordList(record.edges).map((edge, index) => ({
			...edge,
			id: field(edge, 'id', `edge-${index}`),
			source_node_id: field(edge, 'source_node_id', ''),
			target_node_id: field(edge, 'target_node_id', ''),
			type: backendEdgeType(field(edge, 'type', 'DEFAULT')),
			...(typeof edge.condition === 'string' && edge.condition
				? { condition: edge.condition }
				: {}),
		})),
	};
}

/** Hint lines for canvas nodes that still carry the minimal new-node skeleton. */
export function skeletonNodeHints(nodes: DisplayNode[]): string[] {
	return nodes
		.filter((node) => node.label === node.id)
		.map(
			(node) =>
				`node ${node.id} still uses the default name; set a name and type or complete details in JSON`,
		);
}

/**
 * Derive a short human summary from a stored template entry for the
 * read-only preview. Unknown shapes degrade to a version row only.
 */
export function summarizeTemplate(kind: TemplateKind, raw: unknown): KeyValue[] {
	const asRecord = (value: unknown): Record<string, unknown> =>
		value && typeof value === 'object' && !Array.isArray(value)
			? (value as Record<string, unknown>)
			: {};
	const record = asRecord(raw);
	const definition = asRecord(record.definition ?? raw);
	const summary: KeyValue[] = [];
	const version = definition.version ?? record.version;
	if (typeof version === 'string' && version) {
		summary.push({ key: 'Version', value: version });
	}
	if (kind === 'workflow') {
		const nodes = Array.isArray(definition.nodes) ? definition.nodes : null;
		const edges = Array.isArray(definition.edges) ? definition.edges : null;
		if (nodes !== null) summary.push({ key: 'Nodes', value: String(nodes.length) });
		if (edges !== null) summary.push({ key: 'Edges', value: String(edges.length) });
		if (nodes !== null && edges !== null) {
			const targets = new Set(
				(edges as Array<Record<string, unknown>>).map((edge) => String(edge.target_node_id ?? '')),
			);
			const sources = new Set(
				(edges as Array<Record<string, unknown>>).map((edge) => String(edge.source_node_id ?? '')),
			);
			const starts = (nodes as Array<Record<string, unknown>>)
				.map((node) => String(node.id ?? ''))
				.filter((id) => id && !targets.has(id))
				.slice(0, 3);
			const ends = (nodes as Array<Record<string, unknown>>)
				.map((node) => String(node.id ?? ''))
				.filter((id) => id && !sources.has(id))
				.slice(0, 3);
			if (starts.length > 0) summary.push({ key: 'Start', value: starts.join(', ') });
			if (ends.length > 0) summary.push({ key: 'End', value: ends.join(', ') });
		}
		return summary;
	}
	if (kind === 'agent') {
		const config = asRecord(definition.config);
		const maxIterations = config.max_iterations;
		if (typeof maxIterations === 'number') {
			summary.push({ key: 'Max iterations', value: String(maxIterations) });
		}
		const tools = asRecord(config.available_tools);
		const available = Array.isArray(tools.available)
			? (tools.available as unknown[]).map(String)
			: [];
		if (available.length > 0) {
			const shown = available.slice(0, 5).join(', ');
			summary.push({
				key: 'Tools',
				value: available.length > 5 ? `${shown} (+${available.length - 5} more)` : shown,
			});
		} else {
			const configKeys = Object.keys(config);
			if (configKeys.length > 0) {
				summary.push({ key: 'Config keys', value: configKeys.join(', ') });
			}
		}
		return summary;
	}
	const typeKey = kind === 'trigger' ? 'trigger_type' : 'node_type';
	const typeValue = definition[typeKey] ?? record[typeKey];
	if (typeof typeValue === 'string' && typeValue) {
		summary.push({
			key: kind === 'trigger' ? 'Trigger type' : 'Node type',
			value: typeValue,
		});
	}
	return summary;
}
