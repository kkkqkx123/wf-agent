import { client } from '#lib/api/client';
import { call, extractPage, requireData } from '#lib/api/envelope';

/**
 * One chat starter: a suggestion chip filling the composer draft.
 * Backend field names arrive snake_cased and are mapped here once.
 */
export interface ChatStarter {
	id: string;
	title: string;
	content: string;
	description?: string;
	locale: string;
	tags: string[];
	enabled: boolean;
	isDefault: boolean;
	isSystem: boolean;
	usageCount: number;
	updatedAt: string;
}

interface StarterDto {
	id?: string;
	title?: string;
	content?: string;
	description?: string | null;
	locale?: string;
	tags?: string[] | null;
	enabled?: boolean;
	is_default?: boolean;
	is_system?: boolean;
	usage_count?: number;
	updated_at?: string;
}

/** Minimal chip shape the composer renders; backend ids stay optional so
 *  locale fallbacks and preview fixtures keep working without one. */
export interface StarterSuggestion {
	id?: string;
	title: string;
	content: string;
}

/** Chip shape of one starter for the composer suggestion row. */
export function toSuggestion(starter: ChatStarter): StarterSuggestion {
	return { id: starter.id, title: starter.title, content: starter.content };
}

function toStarter(d: StarterDto): ChatStarter {
	return {
		id: d.id ?? '',
		title: d.title ?? '',
		content: d.content ?? '',
		description: d.description ?? undefined,
		locale: d.locale ?? 'en',
		tags: d.tags ?? [],
		enabled: d.enabled ?? false,
		isDefault: d.is_default ?? false,
		isSystem: d.is_system ?? false,
		usageCount: d.usage_count ?? 0,
		updatedAt: d.updated_at ?? '',
	};
}

/**
 * Starter list. The composer passes a locale and sees enabled rows only;
 * the admin view passes `includeDisabled` and sees every locale.
 */
export async function listStarters(params?: {
	locale?: string;
	includeDisabled?: boolean;
}): Promise<ChatStarter[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/chat/starters', {
			params: {
				query: {
					locale: params?.locale,
					enabled: params?.includeDisabled ? false : undefined,
				},
			},
		}),
	);
	requireData(data, 'Chat starters');
	return extractPage<StarterDto>(data).items.map(toStarter);
}

/** Full starter entry plus its management flags (admin / library tab). */
export async function getStarter(id: string): Promise<ChatStarter> {
	const data = await call<unknown>(
		client.GET('/api/v1/chat/starters/{id}', { params: { path: { id } } }),
	);
	return toStarter(requireData(data, `Starter ${id}`) as StarterDto);
}

/**
 * Usage ping for one starter. Callers fire it after an accepted send;
 * click-to-fill never counts. Failures stay silent so chips never block
 * the composer.
 */
export async function recordStarterUsage(id: string): Promise<void> {
	try {
		await call<unknown>(
			client.POST('/api/v1/chat/starters/{id}/usage', {
				params: { path: { id } },
			}),
		);
	} catch {
		// Usage is observability, not control flow.
	}
}

export interface StarterInput {
	title: string;
	content: string;
	description?: string;
	locale?: string;
	tags?: string[];
	enabled?: boolean;
	isDefault?: boolean;
}

/** Create a custom starter (builtins reject with a validation error). */
export async function createStarter(input: StarterInput): Promise<ChatStarter> {
	const data = await call<unknown>(
		client.POST('/api/v1/chat/starters', {
			body: {
				title: input.title,
				content: input.content,
				description: input.description,
				locale: input.locale,
				tags: input.tags,
				enabled: input.enabled,
				is_default: input.isDefault,
			},
		}),
	);
	return toStarter(requireData(data, 'Created starter') as StarterDto);
}

/** Update a custom starter; every field replaces the stored value. */
export async function updateStarter(
	id: string,
	input: Partial<StarterInput>,
): Promise<ChatStarter> {
	const data = await call<unknown>(
		client.PUT('/api/v1/chat/starters/{id}', {
			params: { path: { id } },
			body: {
				title: input.title,
				content: input.content,
				description: input.description,
				locale: input.locale,
				tags: input.tags,
				enabled: input.enabled,
				is_default: input.isDefault,
			},
		}),
	);
	return toStarter(requireData(data, `Starter ${id}`) as StarterDto);
}

/** Delete a custom starter (builtins are protected server-side). */
export async function deleteStarter(id: string): Promise<void> {
	await call<unknown>(client.DELETE('/api/v1/chat/starters/{id}', { params: { path: { id } } }));
}

/** Enable or disable a starter (builtins accept the flag, nothing else). */
export async function setStarterEnabled(id: string, enabled: boolean): Promise<ChatStarter> {
	const data = await call<unknown>(
		enabled
			? client.POST('/api/v1/chat/starters/{id}/enable', { params: { path: { id } } })
			: client.POST('/api/v1/chat/starters/{id}/disable', { params: { path: { id } } }),
	);
	return toStarter(requireData(data, `Starter ${id}`) as StarterDto);
}

/** Export one starter as a JSON document for backup or transfer. */
export async function exportStarter(id: string): Promise<string> {
	const data = requireData(
		await call<unknown>(
			client.GET('/api/v1/chat/starters/{id}/export', { params: { path: { id } } }),
		),
		`Starter export ${id}`,
	);
	if (typeof data === 'string') return data;
	return JSON.stringify(data, null, 2);
}

/** Import a starter document; it always lands as a custom row. */
export async function importStarter(json: string): Promise<ChatStarter> {
	const data = await call<unknown>(client.POST('/api/v1/chat/starters/import', { body: { json } }));
	return toStarter(requireData(data, 'Imported starter') as StarterDto);
}
