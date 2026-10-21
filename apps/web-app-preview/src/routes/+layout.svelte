<script lang="ts">
	import '../app.css';
	import type { Snippet } from 'svelte';
	import { resolve } from '$app/paths';
	import { setHrefResolver } from '@wf-agent/ui/link';
	import { appPath } from '#lib/utils/route';
	import AppShell from '#lib/components/layout/AppShell.svelte';
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

	// The shared kit renders links without knowing the router; the base path
	// configured for this app is the only thing it needs.
	setHrefResolver((route) => resolve(appPath(route) as any));

	$effect(() => {
		applyTheme(resolvedTheme());
		applyFontScale(preferences.fontScale);
		applyDensityScale(preferences.spacingScale);
	});

	$effect(() => listenToSystemTheme());
</script>

<AppShell>
	{@render children()}
</AppShell>

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
