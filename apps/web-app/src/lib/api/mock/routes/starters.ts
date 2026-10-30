/** Mock routes: chat starter prompts (suggestion chips + library tab). */

import { pageView, type MockRouter } from '../router';

interface StarterFixture {
	id: string;
	title: string;
	content: string;
	description?: string;
	locale: string;
	tags: string[];
	enabled: boolean;
	is_default: boolean;
	is_system: boolean;
	usage_count: number;
	created_at: string;
	updated_at: string;
}

function row(
	id: string,
	text: string,
	locale: string,
	overrides: Partial<StarterFixture> = {},
): StarterFixture {
	const stamp = '2026-01-01T00:00:00Z';
	return {
		id,
		title: text,
		content: text,
		locale,
		tags: ['general'],
		enabled: true,
		is_default: true,
		is_system: true,
		usage_count: 0,
		created_at: stamp,
		updated_at: stamp,
		...overrides,
	};
}

// Mirrors the backend builtin seeds; the chat surface stays identical
// with and without a server. Mutable so management actions persist in
// preview mode the same way they do against a real backend.
let starters: StarterFixture[] = [
	row('starter.summary-en', 'Summarize the current workspace state', 'en'),
	row('starter.tools-en', 'List the available tools and skills', 'en'),
	row('starter.plan-en', 'Draft a plan before making changes', 'en'),
	row('starter.summary-zh', '总结当前工作区状态', 'zh'),
	row('starter.tools-zh', '列出可用的工具和技能', 'zh'),
	row('starter.plan-zh', '在动手前先拟定计划', 'zh'),
	row('starter.custom.demo-en', 'Review the demo script', 'en', {
		content: 'Review the demo script and list follow-ups',
		tags: ['demo'],
		is_default: false,
		is_system: false,
		usage_count: 7,
	}),
];

function clone<T>(value: T): T {
	return JSON.parse(JSON.stringify(value)) as T;
}

function nowStamp(): string {
	return new Date().toISOString();
}

export function registerStarters(router: MockRouter): void {
	router.on('GET', '/api/v1/chat/starters', (_p, query) => {
		const locale = typeof query.locale === 'string' ? query.locale : '';
		const q = typeof query.q === 'string' ? query.q.toLowerCase() : '';
		const tags =
			typeof query.tags === 'string' ? query.tags.split(',').filter((t) => t !== '') : [];
		const enabledOnly = query.enabled !== false && query.enabled !== 'false';
		let items = starters.map(clone);
		if (locale.trim() !== '') items = items.filter((s) => s.locale === locale);
		if (enabledOnly) items = items.filter((s) => s.enabled);
		if (q !== '') {
			items = items.filter(
				(s) => s.title.toLowerCase().includes(q) || s.content.toLowerCase().includes(q),
			);
		}
		if (tags.length > 0) items = items.filter((s) => s.tags.some((t) => tags.includes(t)));
		return pageView(items, +(query.limit ?? 50), +(query.offset ?? 0));
	});
	router.on('GET', '/api/v1/chat/starters/{id}', (params) => {
		const found = starters.find((s) => s.id === params.id);
		return found ? clone(found) : undefined;
	});
	router.on('POST', '/api/v1/chat/starters/{id}/usage', (params) => {
		const row = starters.find((s) => s.id === params.id);
		if (!row) return undefined;
		row.usage_count += 1;
		row.updated_at = nowStamp();
		return row.usage_count;
	});
	router.on('POST', '/api/v1/chat/starters/{id}/enable', (params) => {
		const row = starters.find((s) => s.id === params.id);
		if (!row) return undefined;
		row.enabled = true;
		row.updated_at = nowStamp();
		return clone(row);
	});
	router.on('POST', '/api/v1/chat/starters/{id}/disable', (params) => {
		const row = starters.find((s) => s.id === params.id);
		if (!row) return undefined;
		row.enabled = false;
		row.updated_at = nowStamp();
		return clone(row);
	});
	router.on('POST', '/api/v1/chat/starters', (_p, _q, body) => {
		const record = (body ?? {}) as Record<string, unknown>;
		const stamp = nowStamp();
		const created: StarterFixture = {
			id:
				typeof record.id === 'string' && record.id.trim() !== ''
					? (record.id as string)
					: `starter.custom.${starters.length + 1}`,
			title: (record.title as string) ?? 'Preview starter',
			content: (record.content as string) ?? '',
			description:
				typeof record.description === 'string' ? (record.description as string) : undefined,
			locale: (record.locale as string) ?? 'en',
			tags: Array.isArray(record.tags) ? (record.tags as string[]) : [],
			enabled: record.enabled === undefined ? true : Boolean(record.enabled),
			is_default: record.is_default === undefined ? false : Boolean(record.is_default),
			is_system: false,
			usage_count: 0,
			created_at: stamp,
			updated_at: stamp,
		};
		starters = [...starters, created];
		return clone(created);
	});
	router.on('PUT', '/api/v1/chat/starters/{id}', (params, _q, body) => {
		const row = starters.find((s) => s.id === params.id);
		if (!row) return undefined;
		const record = (body ?? {}) as Record<string, unknown>;
		if (typeof record.title === 'string') row.title = record.title;
		if (typeof record.content === 'string') row.content = record.content;
		if (record.description !== undefined) {
			row.description =
				record.description === null ? undefined : (record.description as string);
		}
		if (typeof record.locale === 'string') row.locale = record.locale;
		if (Array.isArray(record.tags)) row.tags = record.tags as string[];
		if (typeof record.enabled === 'boolean') row.enabled = record.enabled;
		if (typeof record.is_default === 'boolean') row.is_default = record.is_default;
		row.updated_at = nowStamp();
		return clone(row);
	});
	router.on('DELETE', '/api/v1/chat/starters/{id}', (params) => {
		const index = starters.findIndex((s) => s.id === params.id);
		if (index < 0) return undefined;
		starters.splice(index, 1);
		return true;
	});
	router.on('GET', '/api/v1/chat/starters/{id}/export', (params) => {
		const found = starters.find((s) => s.id === params.id);
		return found ? JSON.stringify(clone(found)) : undefined;
	});
	router.on('POST', '/api/v1/chat/starters/import', (_p, _q, body) => {
		const record = (body ?? {}) as Record<string, unknown>;
		let parsed: Record<string, unknown> = {};
		if (typeof record.json === 'string') {
			try {
				parsed = JSON.parse(record.json) as Record<string, unknown>;
			} catch {
				parsed = {};
			}
		}
		const stamp = nowStamp();
		const created: StarterFixture = {
			id: `starter.custom.imported-${starters.length + 1}`,
			title: (parsed.title as string) ?? 'Imported starter',
			content: (parsed.content as string) ?? '',
			description:
				typeof parsed.description === 'string' ? (parsed.description as string) : undefined,
			locale: (parsed.locale as string) ?? 'en',
			tags: Array.isArray(parsed.tags) ? (parsed.tags as string[]) : [],
			enabled: parsed.enabled === undefined ? true : Boolean(parsed.enabled),
			is_default: parsed.is_default === undefined ? false : Boolean(parsed.is_default),
			is_system: false,
			usage_count: 0,
			created_at: stamp,
			updated_at: stamp,
		};
		starters = [...starters, created];
		return created.id;
	});
}
