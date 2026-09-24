/**
 * Global fixture-fallback switch.
 *
 * Intent: `+page.ts` load() should always try the real API first, but when
 * the backend is unreachable (dev demo, offline laptop) or when
 * `VITE_API_FALLBACK=true` is baked into the build, we silently swap in
 * fixtures so the shell keeps rendering a coherent UI.
 */
export function fallbackEnabled(): boolean {
	if (import.meta.env.VITE_API_FALLBACK === 'true') return true;
	return false;
}
