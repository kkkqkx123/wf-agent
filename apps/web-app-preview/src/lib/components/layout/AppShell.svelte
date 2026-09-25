<script lang="ts">
	import type { Snippet } from 'svelte';
	import { page } from '$app/state';
	import Sidebar from './Sidebar.svelte';
	import SessionNav from './SessionNav.svelte';
	import NavList from './NavList.svelte';
	import TopBar from './TopBar.svelte';
	import Sheet from '$lib/components/ui/Sheet.svelte';
	import { ui } from '$lib/stores/ui.svelte';
	import { sessions } from '$lib/stores/sessions.svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();

	// The shell outlives every route, so it owns the single session refresh.
	$effect(() => {
		void sessions.refresh();
	});

	const selectedId = $derived(page.url.searchParams.get('id'));
</script>

<div class="flex h-screen w-full overflow-hidden bg-background text-foreground">
	<div class="hidden lg:flex">
		<Sidebar />
	</div>

	<div class="flex min-w-0 flex-1 flex-col">
		<TopBar />
		<main class="min-h-0 flex-1 overflow-hidden">
			{@render children()}
		</main>
	</div>
</div>

<!-- Narrow viewports swap the rail for a drawer. -->
<div class="lg:hidden">
	<Sheet
		bind:open={ui.mobileNavOpen}
		title="Navigation"
		side="left"
		width="17rem"
	>
		<SessionNav
			{selectedId}
			class="max-h-[45%] shrink-0 border-b border-border"
		/>
		<NavList tone="popover" onnavigate={() => ui.closeMobileNav()} />
	</Sheet>
</div>
