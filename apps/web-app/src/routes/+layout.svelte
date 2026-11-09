<script lang="ts">
	import '../app.css';
	import type { Snippet } from 'svelte';
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { setHrefResolver } from '#lib/ui/link';
	import { appPath, resolveAppPath } from '#lib/utils/route';
	import {
		DIRECTORY_PARAM,
		HOST_PARAM,
		TAB_LABEL_PARAM,
		applyDocumentTitle,
		onDirectoryChange,
		onWorkspaceLabelChange,
	} from '#lib/api/workspace';
	import AppShell from '#lib/components/layout/AppShell.svelte';
	import PtyHost from '#lib/components/pty/PtyHost.svelte';
	import Toaster from '#lib/components/layout/Toaster.svelte';
	import { ui } from '#lib/stores/ui.svelte';
	import { preferences } from '#lib/stores/preferences.svelte';
	import {
		applyDensityScale,
		applyFontScale,
		applyTheme,
		listenToSystemTheme,
		resolvedTheme,
	} from '#lib/stores/theme.svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();

	// Remount revision for the workspace scope: a directory switch
	// re-creates every page (dropping REST caches and reopening realtime
	// streams under the new directory) without reloading the browser tab.
	// A counter (not the directory value) so the initial mount never
	// remounts itself when the scope is first read.
	let scopeRev = $state(0);

	function readSessionParam(key: string): string | undefined {
		try {
			if (typeof sessionStorage === 'undefined') return undefined;
			const value = sessionStorage
				.getItem(
					key === DIRECTORY_PARAM
						? 'wf.directory'
						: key === TAB_LABEL_PARAM
							? 'wf.tabName'
							: 'wf.host',
				)
				?.trim();
			return value ? value : undefined;
		} catch {
			return undefined;
		}
	}

	// Plain anchor navigations drop the query string. The session entries
	// are the per-tab source of truth, so restore the workspace parameters
	// onto the address bar for shareability. Never overwrites values that
	// arrived with the URL (shared links win over sticky state).
	function ensureWorkspaceParams(): void {
		if (typeof window === 'undefined') return;
		try {
			const url = new URL(window.location.href);
			let changed = false;
			for (const [param, session] of [
				[DIRECTORY_PARAM, readSessionParam(DIRECTORY_PARAM)],
				[TAB_LABEL_PARAM, readSessionParam(TAB_LABEL_PARAM)],
				[HOST_PARAM, readSessionParam(HOST_PARAM)],
			] as const) {
				if (!url.searchParams.get(param) && session) {
					url.searchParams.set(param, session);
					changed = true;
				}
			}
			if (changed) {
				window.history.replaceState(
					null,
					'',
					`${url.pathname}?${url.searchParams.toString()}${url.hash}`,
				);
			}
		} catch {
			// Shareability is cosmetic; resolution already fell back to session.
		}
	}

	onMount(() => {
		applyDocumentTitle();
		ensureWorkspaceParams();
		const offDirectory = onDirectoryChange(() => {
			scopeRev += 1;
			applyDocumentTitle();
		});
		const offLabel = onWorkspaceLabelChange(() => applyDocumentTitle());
		return () => {
			offDirectory();
			offLabel();
		};
	});

	// Re-assert workspace parameters after in-app navigation.
	$effect(() => {
		void page.url.pathname;
		ensureWorkspaceParams();
	});

	// The shared kit renders links without knowing the router; the base path
	// configured for this app is the only thing it needs.
	setHrefResolver((route) => resolveAppPath(appPath(route)));

	$effect(() => {
		applyTheme(resolvedTheme());
		applyFontScale(preferences.fontScale);
		applyDensityScale(preferences.spacingScale);
	});

	$effect(() => listenToSystemTheme());
</script>

<AppShell>
	{#key scopeRev}
		{@render children()}
	{/key}
</AppShell>
<PtyHost />

<!-- AppShell exposes header/sidebar/aside/overlays slots (n8n BaseLayout
  pattern); this layout intentionally only injects children so every page
  keeps the default chrome. Pages needing immersive canvas or a third rail
  pass those slots instead of forking the shell. -->

{#if ui.commandOpen}
	{#await import('#lib/components/layout/CommandPalette.svelte') then module}
		<module.default />
	{/await}
{/if}
<Toaster />
