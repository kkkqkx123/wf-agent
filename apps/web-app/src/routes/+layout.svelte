<script lang="ts">
	import '../app.css';
	import type { Snippet } from 'svelte';
	import AppShell from '$lib/components/layout/AppShell.svelte';
	import CommandPalette from '$lib/components/layout/CommandPalette.svelte';
	import Toaster from '$lib/components/layout/Toaster.svelte';
	import { behavior } from '$lib/stores/behavior.svelte';
	import { preferences } from '$lib/stores/preferences.svelte';
	import {
		applyFontScale,
		applyTheme,
		listenToSystemTheme,
		resolvedTheme,
	} from '$lib/stores/theme.svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();

	$effect(() => {
		applyTheme(resolvedTheme());
		applyFontScale(preferences.fontScale);
	});

	$effect(() => listenToSystemTheme());

	$effect(() => {
		void behavior.load();
	});

	$effect(() => {
		document.documentElement.classList.toggle(
			'reduce-motion',
			behavior.reduceMotion,
		);
	});
</script>

<AppShell>
	{@render children()}
</AppShell>

<CommandPalette />
<Toaster />
