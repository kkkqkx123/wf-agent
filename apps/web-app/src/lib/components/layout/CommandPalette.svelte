<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import Icon from '$lib/components/icons/Icon.svelte';
	import type { IconName } from '$lib/components/icons/paths';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import { NAV_ITEMS } from '$lib/config/navigation';
	import { unifiedSearch, type SearchHit } from '$lib/services/search';
	import { appPath } from '$lib/utils/route';
	import { ui } from '$lib/stores/ui.svelte';
	import { cn } from '$lib/utils/cn';

	interface CommandItem {
		id: string;
		label: string;
		group: string;
		icon: IconName;
		hint: string;
		run: () => void;
	}

	const TYPE_ICON: Record<string, IconName> = {
		workflow: 'workflow',
		execution: 'activity',
		agent_loop: 'loop',
		message: 'sparkles',
		checkpoint: 'archive',
	};

	let query = $state('');
	let rawIndex = $state(0);
	let searching = $state(false);
	let hits = $state<SearchHit[]>([]);

	let timer: ReturnType<typeof setTimeout> | null = null;
	let latest = 0;

	function navigate(href: string, label: string): void {
		ui.setCommandOpen(false);
		ui.recordVisit(href, label);
		// The rule cannot see that the path half of `href` is already resolved.
		// eslint-disable-next-line svelte/no-navigation-without-resolve
		void goto(href);
	}

	function hitHref(hit: SearchHit): string | null {
		switch (hit.type) {
			case 'workflow':
				return resolve('/workflows/[id]', { id: hit.id });
			case 'execution':
				return resolve('/executions/[id]', { id: hit.id });
			case 'agent_loop':
				return appPath(`/chat?id=${hit.id}`);
			case 'message':
				return hit.agentLoopId ? appPath(`/chat?id=${hit.agentLoopId}`) : null;
			case 'checkpoint':
				return hit.executionId
					? resolve('/executions/[id]', { id: hit.executionId })
					: resolve('/checkpoints');
			default:
				return null;
		}
	}

	function hitIcon(type: string): IconName {
		return TYPE_ICON[type] ?? 'search';
	}

	const items = $derived.by<CommandItem[]>(() => {
		const needle = query.trim().toLowerCase();
		const nav: CommandItem[] = NAV_ITEMS.filter(
			(item) =>
				!needle ||
				item.label.toLowerCase().includes(needle) ||
				item.description.toLowerCase().includes(needle),
		).map((item) => ({
			id: `nav:${item.href}`,
			label: item.label,
			group: 'Navigate',
			icon: item.icon,
			hint: item.description,
			run: () => navigate(resolve(item.href), item.label),
		}));

		const recent: CommandItem[] = ui.recent
			.filter(
				(entry) =>
					!needle ||
					entry.label.toLowerCase().includes(needle) ||
					entry.href.toLowerCase().includes(needle),
			)
			.map((entry) => ({
				id: `recent:${entry.href}`,
				label: entry.label,
				group: 'Recent',
				icon: 'history',
				hint: entry.href,
				run: () => navigate(entry.href, entry.label),
			}));

		const results: CommandItem[] = [];
		for (const hit of hits) {
			const href = hitHref(hit);
			if (!href) continue;
			results.push({
				id: `hit:${hit.type}:${hit.id}`,
				label: hit.label,
				group: 'Results',
				icon: hitIcon(hit.type),
				hint: `${hit.type} · ${hit.matches[0] ?? hit.id}`,
				run: () => navigate(href, hit.label),
			});
		}

		if (!needle) return [...nav, ...recent];
		return [...results, ...nav];
	});

	const grouped = $derived.by(() => {
		const groups: Array<{ name: string; entries: CommandItem[] }> = [];
		for (const item of items) {
			const existing = groups.find((group) => group.name === item.group);
			if (existing) {
				existing.entries.push(item);
			} else {
				groups.push({ name: item.group, entries: [item] });
			}
		}
		return groups;
	});

	const flat = $derived(grouped.flatMap((group) => group.entries));

	// Keeps the highlight inside range when the result set shrinks.
	const activeIndex = $derived(
		Math.min(rawIndex, Math.max(0, flat.length - 1)),
	);

	function onkeydown(event: KeyboardEvent): void {
		if (!ui.commandOpen) return;
		if (event.key === 'ArrowDown') {
			event.preventDefault();
			rawIndex = (activeIndex + 1) % Math.max(1, flat.length);
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			rawIndex = (activeIndex - 1 + flat.length) % Math.max(1, flat.length);
		} else if (event.key === 'Enter') {
			event.preventDefault();
			flat[activeIndex]?.run();
		}
	}

	function onwindowkeydown(event: KeyboardEvent): void {
		if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
			event.preventDefault();
			ui.toggleCommand();
		}
	}

	$effect(() => {
		if (!ui.commandOpen) {
			query = '';
			rawIndex = 0;
			hits = [];
			searching = false;
			return;
		}
		const needle = query.trim();
		if (timer) clearTimeout(timer);
		if (!needle) {
			hits = [];
			searching = false;
			return;
		}
		searching = true;
		const request = (latest += 1);
		timer = setTimeout(() => {
			void unifiedSearch({ q: needle, limit: 12 })
				.then((outcome) => {
					if (request !== latest) return;
					hits = outcome.items;
					searching = false;
				})
				.catch(() => {
					if (request !== latest) return;
					hits = [];
					searching = false;
				});
		}, 200);
		return () => {
			if (timer) clearTimeout(timer);
		};
	});
</script>

<svelte:window onkeydown={onwindowkeydown} />

<Dialog
	bind:open={ui.commandOpen}
	title="Command palette"
	description="Search navigation, sessions, workflows and executions"
	width="34rem"
>
	<div {onkeydown} role="presentation">
		<div class="relative">
			<Icon
				name="search"
				size={15}
				class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
			/>
			<input
				bind:value={query}
				placeholder="Type to search…"
				aria-label="Command palette search"
				class="h-9 w-full rounded-md border border-input bg-card pl-8 pr-2 text-body text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
			/>
		</div>

		<div class="mt-2 max-h-80 overflow-y-auto">
			{#if flat.length === 0}
				<p class="px-2 py-6 text-center text-caption text-muted-foreground">
					{searching ? 'Searching…' : 'No matches'}
				</p>
			{:else}
				{#each grouped as group (group.name)}
					<p
						class="px-2 py-1 text-micro uppercase tracking-wide text-muted-foreground"
					>
						{group.name}
					</p>
					{#each group.entries as item (item.id)}
						{@const index = flat.indexOf(item)}
						<button
							type="button"
							onclick={item.run}
							onmouseenter={() => (rawIndex = index)}
							class={cn(
								'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors',
								index === activeIndex
									? 'bg-accent text-accent-foreground'
									: 'hover:bg-accent/60',
							)}
						>
							<Icon
								name={item.icon}
								size={14}
								class="shrink-0 text-muted-foreground"
							/>
							<span class="min-w-0 flex-1 truncate text-body">{item.label}</span
							>
							<span class="shrink-0 truncate text-micro text-muted-foreground"
								>{item.hint}</span
							>
						</button>
					{/each}
				{/each}
			{/if}
		</div>
	</div>
</Dialog>
