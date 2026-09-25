import { client } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type { Template, TemplateKind } from '$lib/types/models';

interface TemplateDto {
	id?: string;
	name?: string;
	kind?: string;
	category?: string;
	description?: string;
	usage?: number;
	used?: number;
	featured?: boolean;
	tags?: string[];
	popular?: boolean;
}

const KIND_MAP: Record<string, TemplateKind | undefined> = {
	node: 'node',
	'node-template': 'node',
	trigger: 'trigger',
	'trigger-template': 'trigger',
	agent: 'agent',
	'agent-template': 'agent',
	'agent-trigger': 'trigger',
	workflow: 'workflow',
	'workflow-template': 'workflow',
};

function normalizeKind(raw: unknown): TemplateKind | undefined {
	if (typeof raw !== 'string') return undefined;
	return KIND_MAP[raw.toLowerCase()];
}

function toTemplate(d: TemplateDto, fallbackKind?: TemplateKind): Template {
	const kind = normalizeKind(d.kind) ?? fallbackKind ?? 'node';
	return {
		id: d.id ?? d.name ?? '',
		name: d.name ?? '',
		kind,
		category: d.category ?? '',
		description: d.description ?? '',
		usage: d.usage ?? d.used ?? 0,
		featured: d.featured ?? false,
		tags: d.tags ?? [],
	};
}

/**
 * List templates filtered by kind. Uses the `/api/v1/templates/library`
 * aggregator endpoint which accepts a `kind` query param. Falls back to
 * per-kind routes when the aggregator returns nothing.
 */
export async function listTemplates(params?: {
	kind?: TemplateKind | 'all';
	featuredOnly?: boolean;
	limit?: number;
	offset?: number;
}): Promise<Template[]> {
	const all: Template[] = [];
	const kind = params?.kind ?? 'all';
	const limit = params?.limit ?? 50;
	const offset = params?.offset ?? 0;

	const fetchOne = async (query?: Record<string, unknown>, fallback?: TemplateKind) => {
		const data = await call<unknown>(
			client.GET('/api/v1/templates/library', {
				params: { query },
			// eslint-disable-next-line @typescript-eslint/no-explicit-any
		} as any),
		);
		if (Array.isArray(data)) {
			for (const item of data as TemplateDto[]) {
				all.push(toTemplate(item, fallback));
			}
		} else if (data && typeof data === 'object') {
			const page = extractPage<TemplateDto>(data);
			for (const item of page.items) {
				all.push(toTemplate(item, fallback));
			}
		}
	};

	if (kind === 'all') {
		// Aggregator endpoint, no kind filter
		await fetchOne({ limit, offset });
	} else {
		await fetchOne({ kind, limit, offset }, kind);
	}

	// Apply featuredOnly filter client-side when needed
	if (params?.featuredOnly) {
		return all.filter((t) => t.featured);
	}
	return all;
}

export async function listFeaturedTemplates(): Promise<Template[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/templates/library/featured'),
	);
	if (Array.isArray(data)) {
		return (data as TemplateDto[]).map((d) => toTemplate(d));
	}
	return [];
}

export async function listPopularTemplates(): Promise<Template[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/templates/library/popular'),
	);
	if (Array.isArray(data)) {
		return (data as TemplateDto[]).map((d) => toTemplate(d));
	}
	return [];
}

export async function listNodeTemplates(params?: {
	limit?: number;
	offset?: number;
}): Promise<PageResult<Template>> {
	const data = await call<unknown>(
		client.GET('/api/v1/templates/node', {
			params: { query: params ?? {} },
		// eslint-disable-next-line @typescript-eslint/no-explicit-any
		} as any),
	);
	const page = extractPage<TemplateDto>(data);
	return { ...page, items: page.items.map((d) => toTemplate(d, 'node')) };
}
