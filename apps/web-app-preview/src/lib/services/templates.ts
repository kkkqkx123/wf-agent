import { client } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
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
		tags: d.tags ?? [],
	};
}

/**
 * Browse one kind of the template library.
 *
 * The `/templates/library` aggregator only understands the workflow and agent
 * registries, so the node and trigger kinds fall back to their own registry
 * routes, which are paged.
 */
export async function listTemplates(params: {
	kind: 'all' | TemplateKind;
}): Promise<Template[]> {
	const { kind } = params;
	const pageQuery = { params: { query: { limit: 100 } } };
	if (kind === 'node' || kind === 'trigger') {
		const data = await call<unknown>(
			kind === 'node'
				? client.GET('/api/v1/templates/node', pageQuery)
				: client.GET('/api/v1/templates/trigger', pageQuery),
		);
		return extractPage<TemplateDto>(data).items.map((d) => toTemplate(d, kind));
	}
	const data = await call<unknown>(
		client.GET('/api/v1/templates/library', {
			params: { query: kind === 'all' ? {} : { kind } },
		}),
	);
	return (Array.isArray(data) ? (data as TemplateDto[]) : []).map((d) =>
		toTemplate(d, kind === 'all' ? 'workflow' : kind),
	);
}

/** Public and enabled templates, most used first. */
export async function listFeaturedTemplates(): Promise<Template[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/templates/library/featured'),
	);
	return (Array.isArray(data) ? (data as TemplateDto[]) : []).map((d) =>
		toTemplate(d, 'workflow'),
	);
}

/** Copy a workflow or agent template into a new editable entry. */
export async function cloneTemplate(
	id: string,
	kind: TemplateKind,
	newName: string,
): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/templates/library/{id}/clone', {
			params: { path: { id } },
			body: { kind, new_name: newName },
		}),
	);
}
