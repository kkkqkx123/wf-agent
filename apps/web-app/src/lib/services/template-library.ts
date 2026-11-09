import { client, downloadFile } from '#lib/api/client';
import { call, requireData } from '#lib/api/envelope';
import { ApiHttpError } from '#lib/api/envelope';
import type { GovernanceFlag, Template, TemplateKind } from '#lib/types/models';
import { describeCoordinate } from '#lib/services/template-coordinate';

interface TemplateDto {
	id?: string;
	name?: string;
	kind?: string;
	category?: string | null;
	description?: string | null;
	usage_count?: number;
	tags?: string[] | null;
	author?: string | null;
	enabled?: boolean;
	is_public?: boolean;
	is_system?: boolean;
	is_published?: boolean;
}

const KINDS: readonly TemplateKind[] = ['node', 'trigger', 'agent', 'workflow'];

/** Built-in main agent every conversation falls back to. */
export const DEFAULT_AGENT_ID = '@standard/main';

/**
 * Whether a template may be used directly from a selector: not a system
 * fixture, not disabled, not private. Mirrors the composition boundary's
 * runnable gate on the server, which enforces the same rule for every kind.
 */
export function isRunnableTemplate(template: {
	enabled?: GovernanceFlag;
	isPublic?: GovernanceFlag;
	isSystem?: GovernanceFlag;
}): boolean {
	if (template.isSystem) return false;
	if (template.enabled === false || template.isPublic === false) return false;
	return true;
}

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
		enabled: d.enabled,
		isPublic: d.is_public,
		isSystem: d.is_system,
		published: d.is_published,
		author: d.author ?? null,
	};
}

/**
 * Browse the template library. The unified library surface covers all four
 * kinds, so every browse goes through one request.
 */
export async function listTemplates(params: { kind: 'all' | TemplateKind }): Promise<Template[]> {
	const { kind } = params;
	const data = await call<unknown>(
		client.GET('/api/v1/templates/library', {
			params: { query: kind === 'all' ? {} : { kind } },
		}),
	);
	requireData(data, `Template library (${kind})`);
	return (Array.isArray(data) ? (data as TemplateDto[]) : []).map((d) =>
		toTemplate(d, normalizeKind(d.kind) ?? (kind === 'all' ? 'workflow' : kind)),
	);
}

/** Public and enabled templates, most used first. */
export async function listFeaturedTemplates(): Promise<Template[]> {
	const data = await call<unknown>(client.GET('/api/v1/templates/library/featured'));
	requireData(data, 'Featured templates');
	return (Array.isArray(data) ? (data as TemplateDto[]) : []).map((d) => toTemplate(d, 'workflow'));
}

/**
 * Copy a template into a new entry through the server, returning the new id.
 * Same-library clone: the server assigns a new id and records the source
 * template as the derivation source, so the new entry links back without
 * remark-style lineage.
 */
export async function cloneTemplate(
	id: string,
	kind: TemplateKind,
	newName: string,
): Promise<string> {
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

/**
 * Publish a workflow definition. The published mark is written
 * to the stored definition, so it stays published across a restart.
 */
export async function publishWorkflowTemplate(id: string): Promise<void> {
	await call<string>(
		client.POST('/api/v1/workflows/{id}/publish', {
			params: { path: { id } },
		}),
	);
}

/**
 * Remove the published mark from a workflow definition. The definition
 * itself stays where it is and keeps running.
 */
export async function unpublishWorkflowTemplate(id: string): Promise<void> {
	await call<string>(
		client.POST('/api/v1/workflows/{id}/unpublish', {
			params: { path: { id } },
		}),
	);
}

/**
 * Explicit usage ping for one template. Callers fire it after a clone,
 * derive-as-workflow, or export download; repeated calls each count once.
 */
export async function recordTemplateUsage(id: string): Promise<number> {
	const data = await call<number>(
		client.POST('/api/v1/templates/library/{id}/usage', {
			params: { path: { id } },
		}),
	);
	return typeof data === 'number' ? data : 0;
}

export interface TemplateDetail {
	template: Template;
	raw: unknown;
	definitionJson: string;
	version: string | null;
}

/** Full template entry plus a pretty-printed definition for preview/edit.
 *
 * Agent entries come from the agent definition surface as composed
 * views; workflow entries come from the workflow surface as bare
 * definitions and are normalised here. */
export async function getTemplateDetail(id: string, kind: TemplateKind): Promise<TemplateDetail> {
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
			client.GET('/api/v1/agent-definitions/{id}', {
				params: { path: { id } },
			}),
		);
	} else {
		raw = await call<unknown>(
			client.GET('/api/v1/workflows/{id}', { params: { path: { id } } }),
		);
	}
	requireData(raw, `Template ${describeCoordinate({ kind, id })}`);
	if (kind === 'workflow') {
		return workflowDefinitionDetail(id, raw);
	}
	const record = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
	const definition = record.definition ?? raw;
	return {
		template: toTemplate(
			{
				id: typeof record.id === 'string' ? record.id : id,
				name: typeof record.name === 'string' ? record.name : id,
				kind,
				category: typeof record.category === 'string' ? record.category : null,
				description: typeof record.description === 'string' ? record.description : null,
				tags: Array.isArray(record.tags) ? (record.tags as string[]) : null,
				enabled: typeof record.enabled === 'boolean' ? record.enabled : undefined,
				is_public: typeof record.is_public === 'boolean' ? record.is_public : undefined,
				is_system: typeof record.is_system === 'boolean' ? record.is_system : undefined,
				is_published: typeof record.is_published === 'boolean' ? record.is_published : undefined,
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

/** A workflow definition normalised to the template shape the detail views
 * consume. Library facts travel in the definition metadata, so the
 * composed view is rebuilt here instead of fetched. */
function workflowDefinitionDetail(id: string, raw: unknown): TemplateDetail {
	const record = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
	const metadata =
		record.metadata && typeof record.metadata === 'object'
			? (record.metadata as Record<string, unknown>)
			: {};
	const tags = Array.isArray(metadata.tags) ? (metadata.tags as string[]) : [];
	const template: Template = {
		id: typeof record.id === 'string' ? record.id : id,
		name: typeof record.name === 'string' ? record.name : id,
		kind: 'workflow',
		category: typeof metadata.category === 'string' ? metadata.category : '',
		description: typeof record.description === 'string' ? record.description : '',
		usage: 0,
		featured: false,
		tags,
		enabled: undefined,
		isPublic: typeof metadata.is_public === 'boolean' ? metadata.is_public : undefined,
		isSystem: typeof metadata.is_system === 'boolean' ? metadata.is_system : undefined,
		published: typeof metadata.published === 'boolean' ? metadata.published : undefined,
		author: typeof metadata.author === 'string' ? metadata.author : null,
	};
	return {
		template,
		raw,
		definitionJson: JSON.stringify(raw ?? null, null, 2),
		version:
			typeof record.version === 'string' && record.version ? record.version : null,
	};
}

/**
 * Save a template entry. Node and trigger kinds upsert through their own
 * routes; the agent kind writes through the agent definition surface.
 * Workflow saves go through the workflow service, which splits create and
 * replace; the workflow branch here maps a template-shaped payload onto
 * those definition endpoints for callers that only know templates.
 * Update failures surface directly without rollback.
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
			: await call<string>(client.POST('/api/v1/templates/trigger', { body }));
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
					client.PUT('/api/v1/agent-definitions/{id}', {
						params: { path: { id } },
						body,
					}),
				)
			: await call<string>(
					client.PUT('/api/v1/workflows/{id}', {
						params: { path: { id } },
						body: (body.definition ?? body) as Record<string, unknown>,
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
			? await call<string>(client.POST('/api/v1/agent-definitions', { body }))
			: await call<string>(
				client.POST('/api/v1/workflows', {
					body: { ...(body.definition ?? body), id: body.id ?? undefined } as Record<
						string,
						unknown
					>,
				}),
			);
	if (!saved) throw new Error(`Template registration returned no id for ${kind}`);
	return saved;
}

async function deleteLibraryTemplate(kind: 'agent' | 'workflow', id: string): Promise<void> {
	if (kind === 'agent') {
		await call<unknown>(
			client.DELETE('/api/v1/agent-definitions/{id}', {
				params: { path: { id } },
			}),
		);
		return;
	}
	await call<unknown>(
		client.DELETE('/api/v1/workflows/{id}', {
			params: { path: { id } },
		}),
	);
}

/** Delete a template entry. */
export async function deleteTemplate(kind: TemplateKind, id: string): Promise<void> {
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
export async function importTemplate(kind: TemplateKind, json: string): Promise<string> {
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
		kind === 'workflow'
			? client.POST('/api/v1/workflows/import', {
					body: { json },
				})
			: client.POST('/api/v1/agent-definitions/import', {
					body: { json },
				}),
	);
	if (!saved) throw new Error('Import returned no id');
	return saved;
}

/** Export a template entry as a JSON file download. */
export async function exportTemplate(kind: TemplateKind, id: string): Promise<void> {
	if (kind === 'node' || kind === 'trigger') {
		await downloadFile(
			`/api/v1/templates/${kind}/${encodeURIComponent(id)}/export?download=true`,
			`${kind}-template-${id}.json`,
		);
		return;
	}
	if (kind === 'agent') {
		await downloadFile(
			`/api/v1/agent-definitions/${encodeURIComponent(id)}/export?download=true`,
			`${kind}-template-${id}.json`,
		);
		return;
	}
	await downloadFile(
		`/api/v1/workflows/${encodeURIComponent(id)}/export?download=true`,
		`${kind}-template-${id}.json`,
	);
}

/**
 * Publish an agent definition. The published mark is written to the
 * stored definition, so it stays published across a restart.
 */
export async function publishAgentTemplate(id: string): Promise<void> {
	await call<string>(
		client.POST('/api/v1/agent-definitions/{id}/publish', {
			params: { path: { id } },
		}),
	);
}

/**
 * Remove the published mark from an agent definition. The definition
 * itself stays where it is and keeps running.
 */
export async function unpublishAgentTemplate(id: string): Promise<void> {
	await call<string>(
		client.POST('/api/v1/agent-definitions/{id}/unpublish', {
			params: { path: { id } },
		}),
	);
}

export function isHttpError(e: unknown): e is ApiHttpError {
	return e instanceof ApiHttpError;
}
