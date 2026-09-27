import { goto } from '$app/navigation';
import type { Pathname } from '$app/types';

/**
 * Union of every pathname SvelteKit generated for this app.
 * Importing it keeps route literals honest: a typo in a href becomes a type error
 * instead of a runtime 404.
 */
export type AppPath = Pathname;

/**
 * Narrow a dynamically built string to the generated pathname union.
 * This is the single place where a cast to `Pathname` is allowed; every call site
 * must still go through `resolve()` so the base path is honoured.
 */
export function appPath(path: string): AppPath {
	return path as AppPath;
}

/** Query keys the app round-trips through the address bar. */
const LIST_KEYS = ['id', 'q', 'status', 'page', 'tab', 'panel'] as const;

export type ListParamKey = (typeof LIST_KEYS)[number];

export type ListParams = Partial<Record<ListParamKey, string>>;

/** Read the list keys that carry a non-empty value out of a URL. */
export function parseListParams(url: URL): ListParams {
	const params: ListParams = {};
	for (const key of LIST_KEYS) {
		const value = url.searchParams.get(key);
		if (value) params[key] = value;
	}
	return params;
}

/**
 * Merge a list-param patch onto a URL's query. Empty values clear their key;
 * keys outside the owned set are ignored so unrelated params stay untouched.
 */
export function buildListQuery(url: URL, patch: ListParams): string {
	const params = new URLSearchParams(url.searchParams);
	const owned: readonly string[] = LIST_KEYS;
	for (const [key, value] of Object.entries(patch)) {
		if (!owned.includes(key)) continue;
		if (value) params.set(key, value);
		else params.delete(key);
	}
	return params.toString();
}

/**
 * Write list params back to the address bar in place. A no-op when the query is
 * unchanged, which is what keeps a selection effect from looping on its own
 * navigation.
 */
export async function gotoWithParams(
	url: URL,
	patch: ListParams,
): Promise<void> {
	const next = buildListQuery(url, patch);
	if (next === url.searchParams.toString()) return;
	// The pathname already comes from the live URL, so the base is baked in and
	// resolve() (route literals only) cannot re-derive it.
	const target = `${url.pathname}${next ? `?${next}` : ''}${url.hash}`;
	// eslint-disable-next-line svelte/no-navigation-without-resolve
	await goto(target, {
		keepFocus: true,
		noScroll: true,
		replaceState: true,
	});
}
