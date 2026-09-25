import { base, resolve } from '$app/paths';
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

/** Query keys that carry list-page state, so a URL alone reproduces the view. */
export const LIST_KEYS = ['q', 'status', 'page', 'id', 'tab', 'panel'] as const;

export type ListKey = (typeof LIST_KEYS)[number];

export type ListState = Partial<Record<ListKey, string>>;

export function parseListParams(url: URL): ListState {
	const state: ListState = {};
	for (const key of LIST_KEYS) {
		const value = url.searchParams.get(key);
		if (value) state[key] = value;
	}
	return state;
}

/**
 * Apply a patch to the list keys of `url`, dropping empty ones. Pure so the
 * write-back rules can be tested without a router.
 */
export function buildListQuery(url: URL, patch: ListState): string {
	const params = new URLSearchParams(url.search);
	for (const key of LIST_KEYS) {
		const value = patch[key];
		if (value) params.set(key, value);
		else if (key in patch) params.delete(key);
	}
	return params.toString();
}

/**
 * Mirror list state into the address bar without disturbing focus or scroll.
 * Only the known keys are rewritten, so any other query survives.
 */
export function gotoWithParams(url: URL, patch: ListState): void {
	const query = buildListQuery(url, patch);
	if (query === url.searchParams.toString()) return;
	const path = url.pathname.startsWith(base)
		? url.pathname.slice(base.length)
		: url.pathname;
	const href = `${resolve(appPath(path))}${query ? `?${query}` : ''}`;
	// The rule cannot see that the path half of `href` is already resolved.
	// eslint-disable-next-line svelte/no-navigation-without-resolve
	void goto(href, {
		replaceState: true,
		keepFocus: true,
		noScroll: true,
	});
}
