import { client, downloadFile } from '$lib/api/client';
import { call, extractPage, requireData } from '$lib/api/envelope';
import { ApiHttpError } from '$lib/api/envelope';
import type { Template, TemplateKind } from '$lib/types/models';

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
	const copy = {
		...raw,
		id: `${id}-copy`,
		name: newName,
		updated_at: Date.now(),
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
 * routes; workflow and agent kinds register, and update as delete plus
 * re-register with the previous entry restored on failure.
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
	const previous = await getTemplateDetail(id, kind).catch(() => null);
	await deleteLibraryTemplate(kind, id);
	try {
		return await registerLibraryTemplate(kind, body);
	} catch (e) {
		if (previous) {
			await registerLibraryTemplate(
				kind,
				previous.raw as Record<string, unknown>,
			).catch(() => null);
		}
		throw e;
	}
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

/** Import a node or trigger template from JSON text, returning the new id. */
export async function importTemplate(
	kind: TemplateKind,
	json: string,
): Promise<string> {
	if (kind !== 'node' && kind !== 'trigger') {
		throw new Error(`Import is not available for ${kind} templates`);
	}
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

/** Export a template entry as a JSON file download. */
export async function exportTemplate(
	kind: TemplateKind,
	id: string,
	raw?: unknown,
): Promise<void> {
	if (kind === 'node' || kind === 'trigger') {
		await downloadFile(
			`/api/v1/templates/${kind}/${encodeURIComponent(id)}/export?download=true`,
			`${kind}-template-${id}.json`,
		);
		return;
	}
	const detail =
		raw !== undefined ? null : await getTemplateDetail(id, kind);
	const payload = raw ?? detail?.raw ?? null;
	const blob = new Blob([JSON.stringify(payload, null, 2)], {
		type: 'application/json',
	});
	const url = URL.createObjectURL(blob);
	const anchor = document.createElement('a');
	anchor.href = url;
	anchor.download = `${kind}-template-${id}.json`;
	anchor.click();
	setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function isHttpError(e: unknown): e is ApiHttpError {
	return e instanceof ApiHttpError;
}
