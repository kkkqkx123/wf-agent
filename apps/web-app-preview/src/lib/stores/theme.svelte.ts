import { browser } from '$app/environment';
import { preferences, type ThemeMode } from './preferences.svelte';

export const systemPreference = $state({ dark: false });

const DARK_META: Record<'light' | 'dark', string> = {
	light: '#ffffff',
	dark: '#16181d',
};

export function resolvedTheme(): 'light' | 'dark' {
	const mode: ThemeMode = preferences.theme;
	if (mode === 'system') return systemPreference.dark ? 'dark' : 'light';
	return mode;
}

export function applyTheme(theme: 'light' | 'dark'): void {
	if (!browser) return;
	const root = document.documentElement;
	root.classList.toggle('dark', theme === 'dark');
	root.style.colorScheme = theme;
	document
		.querySelector('meta[name="theme-color"]')
		?.setAttribute('content', DARK_META[theme]);
}

export function applyFontScale(scale: number): void {
	if (!browser) return;
	document.documentElement.style.setProperty('--font-scale', String(scale));
}

/** Subscribes to OS color-scheme changes; returns the unsubscribe hook. */
export function listenToSystemTheme(): () => void {
	if (!browser) return () => {};
	const query = window.matchMedia('(prefers-color-scheme: dark)');
	systemPreference.dark = query.matches;
	const onChange = (event: MediaQueryListEvent): void => {
		systemPreference.dark = event.matches;
	};
	query.addEventListener('change', onChange);
	return () => query.removeEventListener('change', onChange);
}
