<script lang="ts">
	import type { Snippet } from 'svelte';
	import { browser } from '$app/environment';
	import Sidebar from './Sidebar.svelte';
	import TopBar from './TopBar.svelte';
	import HelpModal from './HelpModal.svelte';
	import Sheet from '@wf-agent/ui/components/Sheet.svelte';
	import { NAV_GROUPS, navItemFor } from '$lib/config/navigation';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import { ui } from '$lib/stores/ui.svelte';
	import { cn } from '@wf-agent/ui/cn';

	interface Props {
		children: Snippet;
		/** Named slots mirroring n8n BaseLayout. Absent slots fall back
		 * to the default chrome; pass an empty snippet to hide one
		 * (e.g. immersive canvas). `overlays` is additive: HelpModal and
		 * the mobile nav sheet always render. */
		header?: Snippet;
		sidebar?: Snippet;
		aside?: Snippet;
		overlays?: Snippet;
	}

	let { children, header, sidebar, aside, overlays }: Props = $props();

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

	function isTypingTarget(target: EventTarget | null): boolean {
		if (!(target instanceof HTMLElement)) return false;
		return (
			target.tagName === 'INPUT' ||
			target.tagName === 'TEXTAREA' ||
			target.tagName === 'SELECT' ||
			target.isContentEditable
		);
	}

	function onwindowkeydown(event: KeyboardEvent): void {
		if (event.key === 'F1') {
			event.preventDefault();
			ui.toggleHelp();
			return;
		}
		if (event.key === '?' && !isTypingTarget(event.target)) {
			event.preventDefault();
			ui.toggleHelp();
		}
	}
</script>

<svelte:window onkeydown={onwindowkeydown} />

<div class="flex h-screen w-full overflow-hidden bg-background text-foreground">
	{#if sidebar}
		{@render sidebar()}
	{:else}
		<div class="hidden lg:flex">
			<Sidebar />
		</div>
	{/if}

	<div class="flex min-w-0 flex-1 flex-col">
		{#if header}
			{@render header()}
		{:else}
			<TopBar />
		{/if}
		<div class="flex min-h-0 flex-1">
			<main class="min-h-0 min-w-0 flex-1 overflow-hidden">
				{@render children()}
			</main>
			{#if aside}
				<aside
					class="hidden w-80 shrink-0 flex-col overflow-hidden border-l border-border bg-card lg:flex xl:w-96"
				>
					<div class="min-h-0 flex-1 overflow-y-auto">
						{@render aside()}
					</div>
				</aside>
			{/if}
		</div>
	</div>
</div>

<!-- Narrow viewports swap the rail for a drawer. -->
<HelpModal />
{#if overlays}
	{@render overlays()}
{/if}
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
