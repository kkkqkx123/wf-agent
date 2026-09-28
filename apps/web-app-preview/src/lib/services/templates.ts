import { client, downloadFile, request } from '$lib/api/client';
import { call, extractPage, requireData } from '$lib/api/envelope';
import { ApiHttpError } from '$lib/api/envelope';
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
 * Server-side validation for an edited definition. Returns issue strings;
 * empty means the definition validates. Node and trigger templates have no
 * dry-run endpoint, so they run the client field gate here and validate
 * for real on save.
 */
export async function validateTemplateDefinition(
	kind: TemplateKind,
	definition: unknown,
): Promise<string[]> {
	if (kind === 'workflow') {
		try {
			await call<unknown>(
				client.POST('/api/v1/workflows/validate', { body: definition }),
			);
			return [];
		} catch (e) {
			return [e instanceof Error ? e.message : 'Workflow invalid'];
		}
	}
	if (kind === 'agent') {
		try {
			await call<unknown>(
				client.POST('/api/v1/agents/validate', { body: definition }),
			);
			return [];
		} catch (e) {
			return [e instanceof Error ? e.message : 'Agent invalid'];
		}
	}
	const issues: string[] = [];
	const record =
		definition && typeof definition === 'object'
			? (definition as Record<string, unknown>)
			: null;
	if (!record) return ['Definition must be a JSON object'];
	if (typeof record.name !== 'string' || !record.name.trim()) {
		issues.push('name: required');
	}
	if (
		kind === 'trigger' &&
		(typeof record.trigger_type !== 'string' || !record.trigger_type.trim())
	) {
		issues.push('trigger_type: required');
	}
	if (
		kind === 'node' &&
		(typeof record.node_type !== 'string' || !record.node_type.trim())
	) {
		issues.push('node_type: required');
	}
	return issues;
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
 * their category/tags live on the outer template record.
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
	return [NAME_FIELD, DESCRIPTION_FIELD, VERSION_FIELD];
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
