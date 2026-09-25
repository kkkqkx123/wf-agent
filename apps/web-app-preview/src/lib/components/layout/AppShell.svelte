<script lang="ts">
	import type { Snippet } from 'svelte';
	import { browser } from '$app/environment';
	import Sidebar from './Sidebar.svelte';
	import TopBar from './TopBar.svelte';
	import Sheet from '$lib/components/ui/Sheet.svelte';
	import { NAV_GROUPS, navItemFor } from '$lib/config/navigation';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import Icon from '$lib/components/icons/Icon.svelte';
	import { ui } from '$lib/stores/ui.svelte';
	import { cn } from '$lib/utils/cn';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();

	const pathname = $derived(page.url.pathname);

	$effect(() => {
		if (!browser) return;
		const onresize = (): void => ui.syncViewport(window.innerWidth);
		onresize();
		window.addEventListener('resize', onresize);
		return () => window.removeEventListener('resize', onresize);
	});

	function isActive(href: string): boolean {
		const item = navItemFor(pathname);
		return !!item && (item.href === href || item.href.startsWith(href));
	}
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
<Sheet
	bind:open={ui.mobileNavOpen}
	title="Navigation"
	side="left"
	width="17rem"
>
	<nav class="space-y-3">
		{#each NAV_GROUPS as group (group.id)}
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
							onclick={() => ui.closeMobileNav()}
							aria-current={isActive(item.href) ? 'page' : undefined}
							class={cn(
								'flex items-center gap-2 rounded-md px-2 py-1.5 text-body transition-colors',
								isActive(item.href)
									? 'bg-accent font-medium text-accent-foreground'
									: 'text-foreground hover:bg-accent',
							)}
						>
							<Icon name={item.icon} size={15} />
							<span class="truncate">{item.label}</span>
						</a>
					{/each}
				</div>
			</div>
		{/each}
	</nav>
</Sheet>
