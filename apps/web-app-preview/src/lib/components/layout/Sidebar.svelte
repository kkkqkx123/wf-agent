<script lang="ts">
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import Icon from '$lib/components/icons/Icon.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Tooltip from '$lib/components/ui/Tooltip.svelte';
	import { NAV_GROUPS, navItemFor } from '$lib/config/navigation';
	import { preferences } from '$lib/stores/preferences.svelte';
	import { ui } from '$lib/stores/ui.svelte';
	import { cn } from '$lib/utils/cn';

	const RAIL_WIDTH = '3.5rem';

	const activeItem = $derived(navItemFor(page.url.pathname));

	function isActive(href: string): boolean {
		const item = activeItem;
		return !!item && (item.href === href || item.href.startsWith(href));
	}

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

		<nav
			class="flex flex-1 flex-col items-center gap-1 overflow-y-auto py-2 scrollbar-none"
		>
			{#each NAV_GROUPS as group (group.id)}
				<div class="mb-1 w-full px-1.5">
					<div class="mb-1 h-px bg-sidebar-border"></div>
					{#each group.items as item (item.href)}
						<Tooltip text={item.label} class="w-full">
							<a
								href={resolve(item.href)}
								onclick={navigate}
								aria-current={isActive(item.href) ? 'page' : undefined}
								class={cn(
									'flex h-8 w-full items-center justify-center rounded-md transition-colors',
									isActive(item.href)
										? 'bg-sidebar-accent text-foreground'
										: 'text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-foreground',
								)}
							>
								<Icon name={item.icon} size={17} />
							</a>
						</Tooltip>
					{/each}
				</div>
			{/each}
		</nav>

		<div
			class="invisible absolute left-full top-0 z-40 ml-1 hidden h-full w-60 rounded-r-lg border border-border bg-popover p-2 shadow-popover group-hover/rail:visible lg:group-hover/rail:block"
		>
			{#each NAV_GROUPS as group (group.id)}
				<p
					class="px-2 py-1 text-micro uppercase tracking-wide text-muted-foreground"
				>
					{group.label}
				</p>
				{#each group.items as item (item.href)}
					<a
						href={resolve(item.href)}
						onclick={navigate}
						class={cn(
							'flex items-center gap-2 rounded-md px-2 py-1.5 text-body transition-colors',
							isActive(item.href)
								? 'bg-accent text-accent-foreground'
								: 'text-foreground hover:bg-accent',
						)}
					>
						<Icon name={item.icon} size={15} />
						<span class="truncate">{item.label}</span>
					</a>
				{/each}
			{/each}
		</div>
	</aside>
{:else}
	<aside
		class="flex shrink-0 flex-col border-r border-sidebar-border bg-sidebar"
		style:width="{preferences.sidebarWidth}px"
	>
		<div
			class="flex h-12 items-center justify-between gap-2 border-b border-sidebar-border px-3"
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

		<nav class="flex-1 overflow-y-auto px-2 py-2">
			{#each NAV_GROUPS as group (group.id)}
				<div class="mb-3">
					<p
						class="px-2 pb-1 text-micro uppercase tracking-wide text-muted-foreground"
					>
						{group.label}
					</p>
					<div class="space-y-0.5">
						{#each group.items as item (item.href)}
							<a
								href={resolve(item.href)}
								onclick={navigate}
								aria-current={isActive(item.href) ? 'page' : undefined}
								class={cn(
									'flex items-center gap-2 rounded-md px-2 py-1.5 text-body transition-colors',
									isActive(item.href)
										? 'bg-sidebar-accent font-medium text-foreground'
										: 'text-sidebar-foreground/80 hover:bg-sidebar-accent hover:text-foreground',
								)}
							>
								<Icon name={item.icon} size={15} class="shrink-0" />
								<span class="truncate">{item.label}</span>
							</a>
						{/each}
					</div>
				</div>
			{/each}
		</nav>

		<div class="border-t border-sidebar-border px-3 py-2">
			<div class="flex items-center gap-2 text-micro text-muted-foreground">
				<span class="h-1.5 w-1.5 rounded-full bg-success"></span>
				<span>API reachable</span>
			</div>
		</div>
	</aside>
{/if}
