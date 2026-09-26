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
