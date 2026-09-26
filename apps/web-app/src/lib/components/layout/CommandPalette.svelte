<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import Icon from '$lib/components/icons/Icon.svelte';
	import type { IconName } from '$lib/components/icons/paths';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import { NAV_ITEMS } from '$lib/config/navigation';
	import { executions } from '$lib/fixtures/executions';
	import { workflows } from '$lib/fixtures/workflows';
	import { agentLoops } from '$lib/fixtures/agentLoops';
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

	let query = $state('');
	let rawIndex = $state(0);

	const items = $derived.by<CommandItem[]>(() => {
		const nav: CommandItem[] = NAV_ITEMS.map((item) => ({
			id: `nav:${item.href}`,
			label: item.label,
			group: 'Navigate',
			icon: item.icon,
			hint: item.description,
			run: () => {
				ui.setCommandOpen(false);
				void goto(resolve(item.href));
			},
		}));

		const execs: CommandItem[] = executions.map((execution) => ({
			id: `exec:${execution.id}`,
			label: execution.workflowName,
			group: 'Executions',
			icon: 'activity',
			hint: `${execution.id} · ${execution.status}`,
			run: () => {
				ui.setCommandOpen(false);
				void goto(resolve('/executions/[id]', { id: execution.id }));
			},
		}));

		const flows: CommandItem[] = workflows.map((workflow) => ({
			id: `wf:${workflow.id}`,
			label: workflow.name,
			group: 'Workflows',
			icon: 'workflow',
			hint: `v${workflow.version} · ${workflow.category}`,
			run: () => {
				ui.setCommandOpen(false);
				void goto(resolve('/workflows/[id]', { id: workflow.id }));
			},
		}));

		const loops: CommandItem[] = agentLoops.map((loop) => ({
			id: `loop:${loop.id}`,
			label: loop.name,
			group: 'Agent loops',
			icon: 'loop',
			hint: `${loop.id} · ${loop.status}`,
			run: () => {
				ui.setCommandOpen(false);
				void goto(resolve('/agent-loops/[id]', { id: loop.id }));
			},
		}));

		const all = [...nav, ...execs, ...flows, ...loops];
		const needle = query.trim().toLowerCase();
		if (!needle) return all.filter((item) => item.group === 'Navigate');
		return all.filter(
			(item) =>
				item.label.toLowerCase().includes(needle) ||
				item.hint.toLowerCase().includes(needle),
		);
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
		}
	});
</script>

<svelte:window onkeydown={onwindowkeydown} />

<Dialog
	bind:open={ui.commandOpen}
	title="Command palette"
	description="Search navigation, executions, workflows and loops"
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
				class="h-9 w-full rounded-md border border-input bg-card pl-8 pr-2 text-body text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[hsl(var(--ring))]"
			/>
		</div>

		<div class="mt-2 max-h-80 overflow-y-auto">
			{#if flat.length === 0}
				<p class="px-2 py-6 text-center text-caption text-muted-foreground">
					No matches
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
