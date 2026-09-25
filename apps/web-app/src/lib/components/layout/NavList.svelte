<script lang="ts">
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Tooltip from '$lib/components/ui/Tooltip.svelte';
	import { NAV_GROUPS, navItemFor } from '$lib/config/navigation';
	import { cn } from '$lib/utils/cn';

	interface Props {
		/** `labels` shows group headings and names, `icons` is the collapsed rail. */
		variant?: 'labels' | 'icons';
		/** Colour set for the surface the list sits on. */
		tone?: 'sidebar' | 'popover';
		class?: string;
		onnavigate?: () => void;
	}

	let {
		variant = 'labels',
		tone = 'sidebar',
		class: className = '',
		onnavigate,
	}: Props = $props();

	const ITEM_BASE = 'flex items-center gap-2 rounded-md transition-colors';
	const LABEL_ITEM = 'px-2 py-1.5 text-body';
	const ICON_ITEM = 'h-8 w-full justify-center';
	const TONE = {
		sidebar: {
			active: 'bg-sidebar-accent font-medium text-foreground',
			idle: 'text-sidebar-foreground/80 hover:bg-sidebar-accent hover:text-foreground',
		},
		popover: {
			active: 'bg-accent font-medium text-accent-foreground',
			idle: 'text-foreground hover:bg-accent hover:text-accent-foreground',
		},
	} as const;

	const activeHref = $derived(navItemFor(page.url.pathname)?.href);

	function isActive(href: string): boolean {
		const current = activeHref;
		return !!current && (current === href || current.startsWith(href));
	}
</script>

<nav class={cn('space-y-3', className)}>
	{#each NAV_GROUPS as group (group.id)}
		{#if variant === 'icons'}
			<div class="w-full px-1.5">
				<div class="mb-1 h-px bg-sidebar-border"></div>
				{#each group.items as item (item.href)}
					<Tooltip text={item.label} class="w-full">
						<a
							href={resolve(item.href)}
							onclick={onnavigate}
							aria-current={isActive(item.href) ? 'page' : undefined}
							class={cn(
								ITEM_BASE,
								ICON_ITEM,
								isActive(item.href)
									? TONE.sidebar.active
									: 'text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-foreground',
							)}
						>
							<Icon name={item.icon} size={17} />
						</a>
					</Tooltip>
				{/each}
			</div>
		{:else}
			<div>
				<p
					class="px-2 pb-1 text-micro uppercase tracking-wide text-muted-foreground"
				>
					{group.label}
				</p>
				<div class="space-y-0.5">
					{#each group.items as item (item.href)}
						<a
							href={resolve(item.href)}
							onclick={onnavigate}
							aria-current={isActive(item.href) ? 'page' : undefined}
							class={cn(
								ITEM_BASE,
								LABEL_ITEM,
								isActive(item.href) ? TONE[tone].active : TONE[tone].idle,
							)}
						>
							<Icon name={item.icon} size={15} class="shrink-0" />
							<span class="truncate">{item.label}</span>
						</a>
					{/each}
				</div>
			</div>
		{/if}
	{/each}
</nav>
