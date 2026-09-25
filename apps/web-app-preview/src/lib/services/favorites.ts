import { client, request } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';

export interface Favorite {
	kind: string;
	id: string;
	pinned: boolean;
	tags: string[];
}

interface FavoriteDto {
	kind?: string;
	id?: string;
	pinned?: boolean;
	tags?: string[];
}

function toFavorite(d: FavoriteDto): Favorite {
	return {
		kind: d.kind ?? '',
		id: d.id ?? '',
		pinned: d.pinned ?? false,
		tags: d.tags ?? [],
	};
}

export async function listFavorites(params?: {
	limit?: number;
	offset?: number;
	kind?: string;
	pinnedOnly?: boolean;
}): Promise<PageResult<Favorite>> {
	const data = await call<unknown>(
		client.GET('/api/v1/favorites', {
			params: {
				query: {
					...params,
					pinned_only: params?.pinnedOnly,
				},
			},
		}),
	);
	const page = extractPage<FavoriteDto>(data);
	return { ...page, items: page.items.map(toFavorite) };
}

export async function setFavorite(
	kind: string,
	id: string,
	body?: { pinned?: boolean; tags?: string[] },
): Promise<void> {
	await call<unknown>(
		request('PUT', '/api/v1/favorites/{kind}/{id}', {
			params: { path: { kind, id } },
			body: { pinned: body?.pinned ?? null, tags: body?.tags ?? null },
		}),
	);
}

export async function removeFavorite(kind: string, id: string): Promise<void> {
	await call<unknown>(
		request('DELETE', '/api/v1/favorites/{kind}/{id}', {
			params: { path: { kind, id } },
		}),
	);
}
