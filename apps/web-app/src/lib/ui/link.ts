/**
 * Base path resolution for components that render links.
 *
 * The kit has no router of its own: how a route becomes a URL is a property of
 * the host application (SvelteKit resolves through its configured base path, a
 * plain Vite app may have none). The host installs one resolver at startup and
 * every link built inside the kit goes through it. Without a resolver the
 * route is used verbatim, which keeps components renderable in isolation.
 */

export type HrefResolver = (route: string) => string;

let resolver: HrefResolver | null = null;

/** Install the host's route-to-URL resolver. */
export function setHrefResolver(next: HrefResolver | null): void {
	resolver = next;
}

/** Resolve a route into a URL using the installed resolver, if any. */
export function resolveHref(route: string): string {
	return resolver ? resolver(route) : route;
}
