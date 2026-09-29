import { client, downloadFile, request } from '$lib/api/client';
import { call, extractPage, requireData } from '$lib/api/envelope';
import { ApiHttpError } from '$lib/api/envelope';
import { backendEdgeType } from '$lib/graph/display-model';
import { issueTargetsNode } from '$lib/graph/execution-projection';
import { asRecordList, graphField } from '$lib/services/graph';
import type { DisplayEdge, DisplayNode } from '$lib/graph/display-model';
import type { KeyValue, Template, TemplateKind } from '$lib/types/models';

interface TemplateDto {
	id?: string;
	name?: string;
	kind?: string;
	category?: string | null;
	description?: string | null;
	usage_count?: number;
	tags?: string[] | null;
}

const KINDS: readonly TemplateKind[] = ['node', 'trigger', 'agent', 'workflow'];

function normalizeKind(raw: unknown): TemplateKind | undefined {
	if (typeof raw !== 'string') return undefined;
	const lower = raw.toLowerCase();
	return KINDS.find((kind) => kind === lower);
}

function toTemplate(d: TemplateDto, fallbackKind: TemplateKind): Template {
	return {
		id: d.id ?? d.name ?? '',
		name: d.name ?? '',
		kind: normalizeKind(d.kind) ?? fallbackKind,
		category: d.category ?? '',
		description: d.description ?? '',
		usage: d.usage_count ?? 0,
		featured: false,
		tags: d.tags ?? [],
	};
}

/**
 * Browse the template library. The `/templates/library` aggregator only
 * covers the workflow and agent registries, so `all` merges those with the
 * node and trigger registries instead of hard-splitting by kind upstream.
 */
export async function listTemplates(params: {
	kind: 'all' | TemplateKind;
}): Promise<Template[]> {
	const { kind } = params;
	if (kind === 'all') {
		const [library, nodes, triggers] = await Promise.all([
			listLibraryTemplates(),
			listRegistryTemplates('node'),
			listRegistryTemplates('trigger'),
		]);
		return [...library, ...nodes, ...triggers];
	}
	if (kind === 'node' || kind === 'trigger') {
		return listRegistryTemplates(kind);
	}
	const data = await call<unknown>(
		client.GET('/api/v1/templates/library', {
			params: { query: { kind } },
		}),
	);
	requireData(data, `Template library (${kind})`);
	return (Array.isArray(data) ? (data as TemplateDto[]) : []).map((d) =>
		toTemplate(d, kind),
	);
}

async function listLibraryTemplates(): Promise<Template[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/templates/library', { params: { query: {} } }),
	);
	requireData(data, 'Template library');
	return (Array.isArray(data) ? (data as TemplateDto[]) : []).map((d) =>
		toTemplate(d, d.kind === 'agent' ? 'agent' : 'workflow'),
	);
}

async function listRegistryTemplates(
	kind: 'node' | 'trigger',
): Promise<Template[]> {
	const data = await call<unknown>(
		kind === 'node'
			? client.GET('/api/v1/templates/node', {
					params: { query: { limit: 100 } },
				})
			: client.GET('/api/v1/templates/trigger', {
					params: { query: { limit: 100 } },
				}),
	);
	requireData(data, `Template registry (${kind})`);
	return extractPage<TemplateDto>(data).items.map((d) => toTemplate(d, kind));
}

/** Public and enabled templates, most used first. */
export async function listFeaturedTemplates(): Promise<Template[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/templates/library/featured'),
	);
	requireData(data, 'Featured templates');
	return (Array.isArray(data) ? (data as TemplateDto[]) : []).map((d) =>
		toTemplate(d, 'workflow'),
	);
}

/** Copy a template into a new entry, returning the new id. */
export async function cloneTemplate(
	id: string,
	kind: TemplateKind,
	newName: string,
): Promise<string> {
	if (kind === 'workflow' || kind === 'agent') {
		const data = await call<{ id?: unknown }>(
			client.POST('/api/v1/templates/library/{id}/clone', {
				params: { path: { id } },
				body: { kind, new_name: newName },
			}),
		);
		const newId = typeof data?.id === 'string' ? data.id : '';
		if (!newId) throw new Error('Clone returned no id');
		return newId;
	}
	const detail = await getTemplateDetail(id, kind);
	const raw = (
		detail.raw && typeof detail.raw === 'object' ? detail.raw : {}
	) as Record<string, unknown>;
	const now = Date.now();
	const copy = {
		...raw,
		id: `${id}-copy-${now.toString(36)}`,
		name: newName,
		created_at: now,
		updated_at: now,
	};
	return saveTemplate(kind, null, copy);
}

export interface TemplateDetail {
	template: Template;
	raw: unknown;
	definitionJson: string;
	version: string | null;
}

/** Full template entry plus a pretty-printed definition for preview/edit. */
export async function getTemplateDetail(
	id: string,
	kind: TemplateKind,
): Promise<TemplateDetail> {
	let raw: unknown;
	if (kind === 'node') {
		raw = await call<unknown>(
			client.GET('/api/v1/templates/node/{id}', { params: { path: { id } } }),
		);
	} else if (kind === 'trigger') {
		raw = await call<unknown>(
			client.GET('/api/v1/templates/trigger/{id}', {
				params: { path: { id } },
			}),
		);
	} else if (kind === 'agent') {
		raw = await call<unknown>(
			client.GET('/api/v1/templates/library/agents/{id}', {
				params: { path: { id } },
			}),
		);
	} else {
		raw = await call<unknown>(
			client.GET('/api/v1/templates/library/workflows/{id}', {
				params: { path: { id } },
			}),
		);
	}
	requireData(raw, `Template ${id}`);
	const record =
		raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
	const definition = record.definition ?? raw;
	return {
		template: toTemplate(
			{
				id: typeof record.id === 'string' ? record.id : id,
				name: typeof record.name === 'string' ? record.name : id,
				kind,
				category:
					typeof record.template_category === 'string'
						? record.template_category
						: typeof record.category === 'string'
							? record.category
							: null,
				description:
					typeof record.description === 'string' ? record.description : null,
				tags: Array.isArray(record.template_tags)
					? (record.template_tags as string[])
					: null,
			},
			kind,
		),
		raw,
		definitionJson: JSON.stringify(definition ?? null, null, 2),
		version:
			definition && typeof definition === 'object'
				? typeof (definition as Record<string, unknown>).version === 'string'
					? ((definition as Record<string, unknown>).version as string)
					: null
				: null,
	};
}

/**
 * Unified validation issue shared by the template drawer, the workflow
 * editor and the tool dialog. Local issues come from client field checks,
 * server issues from rule endpoints; both render in one list and locate
 * through the same field and node targeting.
 */
export interface TemplateIssue {
	source: 'local' | 'server';
	field: string | null;
	message: string;
	nodeId: string | null;
}

interface FieldCheck {
	field: string;
	message: string;
}

function missingFieldChecks(
	kind: TemplateKind,
	record: Record<string, unknown>,
): FieldCheck[] {
	const checks: FieldCheck[] = [];
	if (typeof record.name !== 'string' || !record.name.trim()) {
		checks.push({ field: 'name', message: 'name: required' });
	}
	if (
		kind === 'trigger' &&
		(typeof record.trigger_type !== 'string' || !record.trigger_type.trim())
	) {
		checks.push({ field: 'trigger_type', message: 'trigger_type: required' });
	}
	if (
		kind === 'node' &&
		(typeof record.node_type !== 'string' || !record.node_type.trim())
	) {
		checks.push({ field: 'node_type', message: 'node_type: required' });
	}
	return checks;
}

function asLocalIssues(checks: FieldCheck[]): TemplateIssue[] {
	return checks.map((check) => ({
		source: 'local' as const,
		field: check.field,
		message: check.message,
		nodeId: null,
	}));
}

/**
 * Synchronous local checks for live editor hints. Workflow and agent kinds
 * only validate through the server gate, so they report nothing here.
 */
export function localTemplateIssues(
	kind: TemplateKind,
	definition: unknown,
): TemplateIssue[] {
	if (kind === 'workflow' || kind === 'agent') return [];
	const record =
		definition && typeof definition === 'object'
			? (definition as Record<string, unknown>)
			: null;
	if (!record) {
		return [
			{
				source: 'local',
				field: null,
				message: 'Definition must be a JSON object',
				nodeId: null,
			},
		];
	}
	return asLocalIssues(missingFieldChecks(kind, record));
}

/**
 * Map server rule issues to the unified model, resolving the first canvas
 * node whose id appears as a field-path segment. Unmatched issues stay
 * global with a null node id.
 */
export function serverTemplateIssues(
	issues: Array<{ field: string; message: string }>,
	nodes: DisplayNode[],
): TemplateIssue[] {
	return issues.map((issue) => ({
		source: 'server' as const,
		field: issue.field,
		message: issue.message,
		nodeId:
			nodes.find((node) => issueTargetsNode(issue.field, node.id))?.id ??
			null,
	}));
}

/**
 * Server-side validation for an edited definition. Returns unified issues;
 * empty means the definition validates. Node and trigger templates have no
 * dry-run endpoint, so they run the client field gate here and validate
 * for real on save.
 */
export async function validateTemplateDefinition(
	kind: TemplateKind,
	definition: unknown,
): Promise<TemplateIssue[]> {
	if (kind === 'workflow') {
		try {
			await call<unknown>(
				client.POST('/api/v1/workflows/validate', { body: definition }),
			);
			return [];
		} catch (e) {
			return [
				{
					source: 'server',
					field: null,
					message: e instanceof Error ? e.message : 'Workflow invalid',
					nodeId: null,
				},
			];
		}
	}
	if (kind === 'agent') {
		try {
			await call<unknown>(
				client.POST('/api/v1/agents/validate', { body: definition }),
			);
			return [];
		} catch (e) {
			return [
				{
					source: 'server',
					field: null,
					message: e instanceof Error ? e.message : 'Agent invalid',
					nodeId: null,
				},
			];
		}
	}
	return localTemplateIssues(kind, definition);
}

/**
 * Save a template entry. Node and trigger kinds upsert through their own
 * routes; workflow and agent kinds register on create and replace in place
 * on update. Update failures surface directly without rollback.
 */
export async function saveTemplate(
	kind: TemplateKind,
	id: string | null,
	raw: unknown,
): Promise<string> {
	const body = raw as Record<string, unknown>;
	if (kind === 'node') {
		const saved = id
			? await call<string>(
					client.PUT('/api/v1/templates/node/{id}', {
						params: { path: { id } },
						body,
					}),
				)
			: await call<string>(client.POST('/api/v1/templates/node', { body }));
		const result = saved ?? id ?? '';
		if (!result) throw new Error('Template save returned no id');
		return result;
	}
	if (kind === 'trigger') {
		const saved = id
			? await call<string>(
					client.PUT('/api/v1/templates/trigger/{id}', {
						params: { path: { id } },
						body,
					}),
				)
			: await call<string>(
					client.POST('/api/v1/templates/trigger', { body }),
				);
		const result = saved ?? id ?? '';
		if (!result) throw new Error('Template save returned no id');
		return result;
	}
	if (id === null) {
		return registerLibraryTemplate(kind, body);
	}
	const saved =
		kind === 'agent'
			? await call<string>(
					client.PUT('/api/v1/templates/library/agents/{id}', {
						params: { path: { id } },
						body,
					}),
				)
			: await call<string>(
					client.PUT('/api/v1/templates/library/workflows/{id}', {
						params: { path: { id } },
						body,
					}),
				);
	const result = saved ?? id;
	if (!result) throw new Error('Template save returned no id');
	return result;
}

async function registerLibraryTemplate(
	kind: 'agent' | 'workflow',
	body: Record<string, unknown>,
): Promise<string> {
	const saved =
		kind === 'agent'
			? await call<string>(
					client.POST('/api/v1/templates/library/agents', { body }),
				)
			: await call<string>(
					client.POST('/api/v1/templates/library/workflows', { body }),
				);
	if (!saved) throw new Error(`Template registration returned no id for ${kind}`);
	return saved;
}

async function deleteLibraryTemplate(
	kind: 'agent' | 'workflow',
	id: string,
): Promise<void> {
	if (kind === 'agent') {
		await call<unknown>(
			client.DELETE('/api/v1/templates/library/agents/{id}', {
				params: { path: { id } },
			}),
		);
		return;
	}
	await call<unknown>(
		client.DELETE('/api/v1/templates/library/workflows/{id}', {
			params: { path: { id } },
		}),
	);
}

/** Delete a template entry. */
export async function deleteTemplate(
	kind: TemplateKind,
	id: string,
): Promise<void> {
	if (kind === 'node') {
		await call<unknown>(
			client.DELETE('/api/v1/templates/node/{id}', {
				params: { path: { id } },
			}),
		);
		return;
	}
	if (kind === 'trigger') {
		await call<unknown>(
			client.DELETE('/api/v1/templates/trigger/{id}', {
				params: { path: { id } },
			}),
		);
		return;
	}
	await deleteLibraryTemplate(kind, id);
}

/**
 * Import a template from JSON text, returning the new id. Every kind goes
 * through a dedicated server import endpoint so validation and id
 * assignment stay server owned.
 */
export async function importTemplate(
	kind: TemplateKind,
	json: string,
): Promise<string> {
	if (kind === 'node' || kind === 'trigger') {
		const saved =
			kind === 'node'
				? await call<string>(
						client.POST('/api/v1/templates/node/import', {
							body: { json },
						}),
					)
				: await call<string>(
						client.POST('/api/v1/templates/trigger/import', {
							body: { json },
						}),
					);
		if (!saved) throw new Error('Import returned no id');
		return saved;
	}
	const saved = await call<string>(
		request('POST', `/api/v1/templates/library/${kind}s/import`, {
			body: { json },
		}),
	);
	if (!saved) throw new Error('Import returned no id');
	return saved;
}

/** Export a template entry as a JSON file download. */
export async function exportTemplate(
	kind: TemplateKind,
	id: string,
): Promise<void> {
	if (kind === 'node' || kind === 'trigger') {
		await downloadFile(
			`/api/v1/templates/${kind}/${encodeURIComponent(id)}/export?download=true`,
			`${kind}-template-${id}.json`,
		);
		return;
	}
	const segment = kind === 'agent' ? 'agents' : 'workflows';
	await downloadFile(
		`/api/v1/templates/library/${segment}/${encodeURIComponent(id)}/export?download=true`,
		`${kind}-template-${id}.json`,
	);
}

export function isHttpError(e: unknown): e is ApiHttpError {
	return e instanceof ApiHttpError;
}

// ── form / JSON dual editing ─────────────────────────────────────

export interface TemplateFormField {
	key: string;
	label: string;
	required: boolean;
	multiline: boolean;
}

const NAME_FIELD: TemplateFormField = {
	key: 'name',
	label: 'Name',
	required: true,
	multiline: false,
};
const DESCRIPTION_FIELD: TemplateFormField = {
	key: 'description',
	label: 'Description',
	required: false,
	multiline: true,
};
const CATEGORY_FIELD: TemplateFormField = {
	key: 'category',
	label: 'Category',
	required: false,
	multiline: false,
};
const TAGS_FIELD: TemplateFormField = {
	key: 'tags',
	label: 'Tags (comma separated)',
	required: false,
	multiline: false,
};
const VERSION_FIELD: TemplateFormField = {
	key: 'version',
	label: 'Version',
	required: false,
	multiline: false,
};

/**
 * Structured fields per kind, matching the backend shapes: node metadata
 * carries no category/tags/version, trigger metadata carries category/tags
 * but no version, library definitions carry name/description/version while
 * their category/tags live on the outer template record. Library kinds
 * expose category/tags in the same field table; the edit session maps
 * those two keys to the outer record so they never land inside definition.
 */
export function templateFormFields(kind: TemplateKind): TemplateFormField[] {
	if (kind === 'node') {
		return [
			NAME_FIELD,
			{
				key: 'node_type',
				label: 'Node type',
				required: true,
				multiline: false,
			},
			DESCRIPTION_FIELD,
		];
	}
	if (kind === 'trigger') {
		return [
			NAME_FIELD,
			{
				key: 'trigger_type',
				label: 'Trigger type',
				required: true,
				multiline: false,
			},
			DESCRIPTION_FIELD,
			CATEGORY_FIELD,
			TAGS_FIELD,
		];
	}
	return [NAME_FIELD, DESCRIPTION_FIELD, VERSION_FIELD, CATEGORY_FIELD, TAGS_FIELD];
}

function asRecord(value: unknown): Record<string, unknown> {
	return value && typeof value === 'object' && !Array.isArray(value)
		? (value as Record<string, unknown>)
		: {};
}

function fieldText(value: unknown): string {
	if (typeof value === 'string') return value;
	if (Array.isArray(value)) return value.map(String).join(', ');
	if (value === undefined || value === null) return '';
	return String(value);
}

/** Flatten a definition object into form strings, with legacy key fallbacks. */
export function formFromDefinition(
	kind: TemplateKind,
	definition: unknown,
): Record<string, string> {
	const record = asRecord(definition);
	const form: Record<string, string> = {};
	for (const field of templateFormFields(kind)) {
		if (field.key === 'category') {
			form.category = fieldText(record.category ?? record.template_category);
		} else if (field.key === 'tags') {
			form.tags = fieldText(record.tags ?? record.template_tags);
		} else {
			form[field.key] = fieldText(record[field.key]);
		}
	}
	return form;
}

/**
 * Merge form strings back into a definition object. Empty optional fields
 * are dropped so untouched keys keep their original shape.
 */
export function formToDefinition(
	kind: TemplateKind,
	form: Record<string, string>,
	base: unknown,
): Record<string, unknown> {
	const record = { ...asRecord(base) };
	for (const field of templateFormFields(kind)) {
		const text = (form[field.key] ?? '').trim();
		if (!text && !field.required) {
			delete record[field.key];
			continue;
		}
		if (field.key === 'tags') {
			record.tags = text
				.split(',')
				.map((tag) => tag.trim())
				.filter(Boolean);
		} else {
			record[field.key] = text;
		}
	}
	return record;
}

/**
 * Map a JSON syntax error to a 1-based line number within the source text.
 * V8 reports `position N`; other engines may only give a message.
 */
export function jsonErrorLine(text: string, message: string): number | null {
	const match = /position (\d+)/.exec(message);
	if (!match) return null;
	const position = Math.min(Number(match[1]), text.length);
	return text.slice(0, position).split('\n').length;
}

/**
 * Locate a dotted field path within JSON source for issue navigation. The
 * last path segment is matched as a JSON key first, then as plain text.
 */
export function jsonFieldLine(text: string, field: string): number | null {
	const segment = field.split('.').filter(Boolean).pop() ?? field;
	if (!segment) return null;
	const lines = text.split('\n');
	const keyIndex = lines.findIndex((line) => line.includes(`"${segment}"`));
	if (keyIndex >= 0) return keyIndex + 1;
	const rawIndex = lines.findIndex((line) => line.includes(segment));
	return rawIndex >= 0 ? rawIndex + 1 : null;
}

/**
 * Normalize form strings for dirty comparison so spacing differences
 * (for example tag separators) never report a false modification.
 */
export function normalizeTemplateForm(
	form: Record<string, string>,
): Record<string, string> {
	const normalized: Record<string, string> = {};
	for (const [key, value] of Object.entries(form)) {
		normalized[key] = value.trim();
	}
	if (normalized.tags) {
		normalized.tags = normalized.tags
			.split(',')
			.map((tag) => tag.trim())
			.filter(Boolean)
			.join(',');
	}
	return normalized;
}

// ── semantic summary ─────────────────────────────────────────────

/**
 * Derive a short human summary from a stored template entry for the
 * read-only preview. Unknown shapes degrade to a version row only.
 */
export function summarizeTemplate(kind: TemplateKind, raw: unknown): KeyValue[] {
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
				(edges as Array<Record<string, unknown>>).map((edge) =>
					String(
						edge.target_node_id ?? edge.to ?? edge.target ?? '',
					),
				),
			);
			const sources = new Set(
				(edges as Array<Record<string, unknown>>).map((edge) =>
					String(
						edge.source_node_id ?? edge.from ?? edge.source ?? '',
					),
				),
			);
			const starts = (nodes as Array<Record<string, unknown>>)
				.map((node) => String(node.id ?? node.node_id ?? ''))
				.filter((id) => id && !targets.has(id))
				.slice(0, 3);
			const ends = (nodes as Array<Record<string, unknown>>)
				.map((node) => String(node.id ?? node.node_id ?? ''))
				.filter((id) => id && !sources.has(id))
				.slice(0, 3);
			if (starts.length > 0)
				summary.push({ key: 'Start', value: starts.join(', ') });
			if (ends.length > 0) summary.push({ key: 'End', value: ends.join(', ') });
		}
		return summary;
	}
	if (kind === 'agent') {
		const config = asRecord(definition.config);
		const maxIterations = config.max_iterations ?? config.maxIterations;
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
				value:
					available.length > 5
						? `${shown} (+${available.length - 5} more)`
						: shown,
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

// ── shared topology editing ──────────────────────────────────────

/** First non-empty string field with legacy key fallbacks. */
export function templateField(
	record: Record<string, unknown>,
	keys: string[],
	fallback: string,
): string {
	return graphField(record, keys, fallback);
}

/** Backend edge type shared by template pages and graph stores. */
export function templateEdgeType(kind: string): string {
	return backendEdgeType(kind);
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

/** Parsed workflow topology from a stored value with alias-tolerant fields. */
export function parseTemplateTopology(value: unknown): TemplateTopology {
	const target =
		value && typeof value === 'object' && !Array.isArray(value)
			? (value as Record<string, unknown>).definition &&
				typeof (value as Record<string, unknown>).definition === 'object'
				? ((value as Record<string, unknown>).definition as Record<
						string,
						unknown
					>)
				: (value as Record<string, unknown>)
			: {};
	return {
		nodes: asRecordList(target.nodes).map((node, index) => ({
			id: templateField(node, ['id', 'node_id'], `node-${index}`),
			label: templateField(
				node,
				['name', 'label', 'id', 'node_id'],
				`node-${index}`,
			),
			kind: templateField(node, ['node_type', 'kind', 'type'], 'STEP'),
		})),
		edges: asRecordList(target.edges)
			.map((edge, index) => ({
				id: templateField(edge, ['id', 'edge_id'], `edge-${index}`),
				source: templateField(edge, ['source_node_id', 'from', 'source'], ''),
				target: templateField(edge, ['target_node_id', 'to', 'target'], ''),
				label:
					typeof edge.condition === 'string' && edge.condition
						? edge.condition
						: undefined,
				kind: templateField(edge, ['type', 'edge_type', 'kind'], 'DEFAULT'),
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
		record.definition &&
		typeof record.definition === 'object' &&
		!Array.isArray(record.definition);
	const target = (
		hasDefinition ? record.definition : record
	) as Record<string, unknown>;
	const prevNodes = asRecordList(target.nodes);
	const prevEdges = asRecordList(target.edges);
	const nodeById = new Map(
		prevNodes.map((node) => [
			templateField(node, ['id', 'node_id'], ''),
			node,
		]),
	);
	const edgeById = new Map(
		prevEdges.map((edge) => [
			templateField(edge, ['id', 'edge_id'], ''),
			edge,
		]),
	);
	const nextNodes = nodes.map((node, index) => {
		const prev = nodeById.get(node.id) ?? {};
		return {
			...prev,
			id: node.id || templateField(prev, ['id', 'node_id'], `node-${index}`),
			node_type: node.kind,
			name: node.label,
		};
	});
	const nextEdges = edges.map((edge, index) => {
		const prev = edgeById.get(edge.id) ?? {};
		return {
			...prev,
			id: edge.id || templateField(prev, ['id', 'edge_id'], `edge-${index}`),
			source_node_id: edge.source,
			target_node_id: edge.target,
			type: templateEdgeType(edge.kind ?? 'DEFAULT'),
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
		name: templateField(record, ['name'], '') || fallbackName,
		nodes: nodes.map((node, index) => ({
			...node,
			id: templateField(node, ['id', 'node_id'], `node-${index}`),
			node_type: templateField(node, ['node_type', 'kind', 'type'], 'STEP'),
			name: templateField(
				node,
				['name', 'label', 'id', 'node_id'],
				`node-${index}`,
			),
		})),
		edges: asRecordList(record.edges).map((edge, index) => ({
			...edge,
			id: templateField(edge, ['id', 'edge_id'], `edge-${index}`),
			source_node_id: templateField(
				edge,
				['source_node_id', 'from', 'source'],
				'',
			),
			target_node_id: templateField(
				edge,
				['target_node_id', 'to', 'target'],
				'',
			),
			type: templateEdgeType(
				templateField(edge, ['type', 'edge_type', 'kind'], 'DEFAULT'),
			),
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

function isLibraryKind(kind: TemplateKind): boolean {
	return kind !== 'node' && kind !== 'trigger';
}

function cloneStructured<T>(value: T): T {
	if (value === undefined || value === null) return value;
	try {
		return JSON.parse(JSON.stringify(value)) as T;
	} catch {
		return value;
	}
}

function sessionTextFor(kind: TemplateKind, full: unknown): string {
	return JSON.stringify(templateEditTarget(kind, full) ?? null, null, 2);
}

function emptySessionFull(kind: TemplateKind): unknown {
	return isLibraryKind(kind) ? { definition: {} } : {};
}

/**
 * Structured edit session shared by the template drawer tabs. The last
 * valid document is the single source of truth; the JSON tab only edits a
 * text projection of it. Failed parses keep the previous document so the
 * form and canvas never clear, and graph conflicts compare a snapshot
 * revision instead of raw text equality.
 */
export class TemplateEditSession {
	private kind: TemplateKind = 'node';
	private full: unknown = {};
	private textValue = '';
	private syntaxMessage: string | null = null;
	private revision = 0;
	private graphLoadedAt = -1;
	private baseline = '';

	get text(): string {
		return this.textValue;
	}

	get syntaxError(): string | null {
		return this.syntaxMessage;
	}

	/** Whether the text projection drifted from the last clean baseline. */
	get textDirty(): boolean {
		return this.textValue !== this.baseline;
	}

	load(kind: TemplateKind, raw: unknown): void {
		this.kind = kind;
		const base = raw ?? emptySessionFull(kind);
		this.full = cloneStructured(base);
		if (this.full === null || this.full === undefined) {
			this.full = emptySessionFull(kind);
		}
		this.textValue = sessionTextFor(kind, this.full);
		this.syntaxMessage = null;
		this.revision = 0;
		this.graphLoadedAt = -1;
		this.baseline = this.textValue;
	}

	/** Record the current text as clean after a save or discard. */
	markClean(): void {
		this.baseline = this.textValue;
	}

	loadEmpty(kind: TemplateKind): void {
		this.load(kind, emptySessionFull(kind));
	}

	setKind(kind: TemplateKind): void {
		if (kind === this.kind) return;
		this.kind = kind;
		try {
			const parsed: unknown = JSON.parse(this.textValue);
			this.syntaxMessage = null;
			if (isLibraryKind(kind)) {
				this.full = { ...asRecord(this.full), definition: parsed };
			} else {
				this.full = parsed;
			}
			this.revision += 1;
		} catch {
			this.full = emptySessionFull(kind);
			this.revision += 1;
		}
	}

	applyText(next: string): void {
		this.textValue = next;
		let parsed: unknown;
		try {
			parsed = JSON.parse(next);
		} catch (e) {
			this.syntaxMessage =
				e instanceof Error ? e.message : 'Invalid JSON';
			return;
		}
		if (isLibraryKind(this.kind)) {
			const current = templateEditTarget(this.kind, this.full);
			if (JSON.stringify(parsed) === JSON.stringify(current ?? null)) {
				this.syntaxMessage = null;
				return;
			}
			this.full = { ...asRecord(this.full), definition: parsed };
		} else {
			if (JSON.stringify(parsed) === JSON.stringify(this.full ?? null)) {
				this.syntaxMessage = null;
				return;
			}
			this.full = parsed;
		}
		this.syntaxMessage = null;
		this.revision += 1;
	}

	definition(): unknown {
		return templateEditTarget(this.kind, this.full);
	}

	fullValue(): unknown {
		return this.full;
	}

	formSnapshot(): Record<string, string> {
		const form = formFromDefinition(
			this.kind,
			templateEditTarget(this.kind, this.full),
		);
		if (isLibraryKind(this.kind)) {
			const outer = asRecord(this.full);
			form.category = fieldText(
				outer.category ?? outer.template_category,
			);
			form.tags = fieldText(outer.tags ?? outer.template_tags);
		}
		return form;
	}

	applyForm(form: Record<string, string>): void {
		if (isLibraryKind(this.kind)) {
			const definition = asRecord(
				templateEditTarget(this.kind, this.full),
			);
			const merged = formToDefinition(this.kind, form, definition);
			delete merged.category;
			delete merged.tags;
			delete merged.template_category;
			delete merged.template_tags;
			const outer = { ...asRecord(this.full) };
			const category = (form.category ?? '').trim();
			if (category) {
				outer.category = category;
			} else {
				delete outer.category;
			}
			delete outer.template_category;
			const tagsText = (form.tags ?? '').trim();
			if (tagsText) {
				outer.tags = tagsText
					.split(',')
					.map((tag) => tag.trim())
					.filter(Boolean);
			} else {
				delete outer.tags;
			}
			delete outer.template_tags;
			this.full = { ...outer, definition: merged };
		} else {
			this.full = formToDefinition(
				this.kind,
				form,
				templateEditTarget(this.kind, this.full) ?? {},
			);
		}
		this.revision += 1;
		this.textValue = sessionTextFor(this.kind, this.full);
		this.syntaxMessage = null;
	}

	topology(): TemplateTopology {
		return parseTemplateTopology(this.full);
	}

	noteGraphLoaded(): void {
		this.graphLoadedAt = this.revision;
	}

	hasGraphConflict(graphDirty: boolean): boolean {
		return (
			graphDirty && this.graphLoadedAt >= 0 && this.graphLoadedAt !== this.revision
		);
	}

	mergeGraph(nodes: DisplayNode[], edges: DisplayEdge[]): void {
		this.full = mergeTemplateGraph(this.full, nodes, edges);
		this.revision += 1;
		this.textValue = sessionTextFor(this.kind, this.full);
		this.syntaxMessage = null;
		this.graphLoadedAt = this.revision;
	}
}
