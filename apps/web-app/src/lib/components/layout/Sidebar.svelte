<script lang="ts">
	import { resolve } from '$app/paths';
	import Icon from '$lib/components/icons/Icon.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import NavList from './NavList.svelte';
	import SessionNav from './SessionNav.svelte';
	import { page } from '$app/state';
	import { preferences } from '$lib/stores/preferences.svelte';
	import { health } from '$lib/stores/health.svelte';
	import { ui } from '$lib/stores/ui.svelte';
	import { cn } from '$lib/utils/cn';

	const RAIL_WIDTH = '3.5rem';

	const selectedId = $derived(page.url.searchParams.get('id'));

	$effect(() => {
		void health.refresh();
		const timer = setInterval(() => void health.refresh(), 30_000);
		return () => clearInterval(timer);
	});

	function navigate(): void {
		ui.closeMobileNav();
	}
</script>

{#if preferences.sidebarCollapsed}
	<!-- Collapsed rail expands on hover so the labels stay reachable. -->
	<aside
		class="group/rail relative z-30 flex shrink-0 flex-col border-r border-sidebar-border bg-sidebar"
		style:width={RAIL_WIDTH}
	>
		<div
			class="flex h-12 items-center justify-center border-b border-sidebar-border"
		>
			<IconButton
				icon="panel-left"
				label="Expand sidebar"
				onclick={() => preferences.toggleSidebar()}
			/>
		</div>

		<NavList
			variant="icons"
			onnavigate={navigate}
			class="flex flex-1 flex-col items-center gap-1 overflow-y-auto py-2 scrollbar-none"
		/>

		<div
			class="invisible absolute left-full top-0 z-40 ml-1 hidden h-full w-64 flex-col rounded-r-lg border border-border bg-popover shadow-popover group-hover/rail:visible lg:group-hover/rail:flex"
		>
			<SessionNav {selectedId} class="h-1/2 shrink-0 border-b border-border" />
			<NavList
				tone="popover"
				onnavigate={navigate}
				class="min-h-0 flex-1 overflow-y-auto p-2"
			/>
		</div>
	</aside>
{:else}
	<aside
		class="flex shrink-0 flex-col border-r border-sidebar-border bg-sidebar"
		style:width="{preferences.sidebarWidth}px"
	>
		<div
			class="flex h-12 shrink-0 items-center justify-between gap-2 border-b border-sidebar-border px-3"
		>
			<a
				href={resolve('/')}
				class="flex min-w-0 items-center gap-2"
				onclick={navigate}
			>
				<span
					class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-primary text-primary-foreground"
				>
					<Icon name="workflow" size={14} />
				</span>
				<span class="truncate text-title font-semibold text-sidebar-foreground"
					>wf-agent</span
				>
			</a>
			<IconButton
				icon="panel-left"
				label="Collapse sidebar"
				compact
				onclick={() => preferences.toggleSidebar()}
			/>
		</div>

		<SessionNav
			{selectedId}
			class="max-h-[45%] min-h-0 shrink-0 border-b border-sidebar-border"
		/>

		<NavList
			onnavigate={navigate}
			class="min-h-0 flex-1 overflow-y-auto px-2 py-2"
		/>

		<div class="shrink-0 border-t border-sidebar-border px-3 py-2">
			<div class="flex items-center gap-2 text-micro text-muted-foreground">
				<span
					class={cn(
						'h-1.5 w-1.5 rounded-full',
						health.status === 'reachable' && 'bg-success',
						health.status === 'unreachable' && 'bg-destructive',
						health.status === 'checking' && 'bg-muted-foreground',
					)}
				></span>
				<span
					>{health.status === 'checking'
						? 'checking API'
						: `API ${health.status}`}</span
				>
			</div>
		</div>
	</aside>
{/if}
