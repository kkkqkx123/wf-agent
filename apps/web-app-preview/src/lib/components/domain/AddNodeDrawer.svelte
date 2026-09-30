<script lang="ts">
	import Dialog from '@wf-agent/ui/components/Dialog.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import type { CanvasPosition } from '$lib/components/domain/GraphCanvas.svelte';
	import {
		DEFAULT_NODE_TYPE,
		isBuiltinNodeType,
		normalizeNodeType,
	} from '$lib/graph/node-kind';
	import {
		listNodeTemplates,
		type NodeTemplateSummary,
	} from '$lib/services/node-templates';
	import { toasts } from '$lib/stores/toast.svelte';
	import { cn } from '@wf-agent/ui/cn';

	interface Props {
		open: boolean;
		/** Drop coordinates the chosen node will land on, if known. */
		position?: CanvasPosition | null;
		/**
		 * Node type plus label; a null label means the caller names the node.
		 * The type is already normalised, so it is either a builtin name or a
		 * plugin-contributed one.
		 */
		onselect: (nodeType: string, name: string | null) => void;
		onclose?: () => void;
	}

	interface Choice {
		key: string;
		nodeType: string;
		/** Template name, or null for the plain untemplated node. */
		name: string | null;
		note: string;
	}

	let {
		open = $bindable(false),
		position = null,
		onselect,
		onclose,
	}: Props = $props();

	let query = $state('');
	let rawIndex = $state(0);
	let templates = $state<NodeTemplateSummary[]>([]);
	let skippedCount = $state(0);
	let hasMore = $state(false);
	let loading = $state(false);

	$effect(() => {
		if (!open || templates.length > 0) return;
		void load();
	});

	async function load(): Promise<void> {
		loading = true;
		try {
			const page = await listNodeTemplates();
			// Every non-blank node_type is insertable: builtin names resolve to
			// their variant, anything else is kept as a plugin-contributed type.
			templates = page.items.filter(
				(template) => normalizeNodeType(template.nodeType) !== null,
			);
			skippedCount = page.skipped;
			hasMore = page.hasMore;
		} catch (e) {
			toasts.error(
				'Failed to load node templates',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			loading = false;
		}
	}

	const filtered = $derived.by(() => {
		const needle = query.trim().toLowerCase();
		if (!needle) return templates;
		return templates.filter(
			(t) =>
				t.name.toLowerCase().includes(needle) ||
				t.description.toLowerCase().includes(needle) ||
				t.nodeType.toLowerCase().includes(needle),
		);
	});

	const grouped = $derived.by(() => {
		type Group = { name: string; plugin: boolean; entries: Choice[] };
		const groups: Group[] = [
			{
				name: 'Blank',
				plugin: false,
				entries: [
					{
						key: 'blank',
						nodeType: DEFAULT_NODE_TYPE,
						name: null,
						note: 'Unnamed step',
					},
				],
			},
		];
		for (const t of filtered) {
			const nodeType = normalizeNodeType(t.nodeType);
			if (nodeType === null) continue;
			const existing = groups.find((group) => group.name === nodeType);
			const entry: Choice = {
				key: t.id,
				nodeType,
				name: t.name,
				note: t.description || nodeType,
			};
			if (existing) existing.entries.push(entry);
			else
				groups.push({
					name: nodeType,
					plugin: !isBuiltinNodeType(nodeType),
					entries: [entry],
				});
		}
		return groups;
	});

	const flat = $derived(grouped.flatMap((group) => group.entries));
	const activeIndex = $derived(
		Math.min(rawIndex, Math.max(0, flat.length - 1)),
	);

	$effect(() => {
		if (!open) {
			query = '';
			rawIndex = 0;
		}
	});

	function onkeydown(event: KeyboardEvent): void {
		if (!open) return;
		if (event.key === 'ArrowDown') {
			event.preventDefault();
			rawIndex = (activeIndex + 1) % flat.length;
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			rawIndex = (activeIndex - 1 + flat.length) % flat.length;
		} else if (event.key === 'Enter') {
			event.preventDefault();
			const selected = flat[activeIndex];
			if (selected) choose(selected);
		}
	}

	function choose(choice: Choice): void {
		onselect(choice.nodeType, choice.name);
		open = false;
	}
</script>

<Dialog
	bind:open
	title="Add node"
	description="Pick a node type to drop onto the canvas"
	width="34rem"
	{onclose}
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
				placeholder="Search node templates…"
				aria-label="Search node templates"
				class="h-9 w-full rounded-md border border-input bg-card pl-8 pr-2 text-body text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[hsl(var(--ring))]"
			/>
		</div>
		{#if position}
			<p class="mt-1 text-micro text-muted-foreground">
				Drops at ({position.x}, {position.y})
			</p>
		{/if}
		<div class="mt-2 max-h-80 overflow-y-auto">
			{#if loading}
				<p class="px-2 py-6 text-center text-caption text-muted-foreground">
					Loading templates…
				</p>
			{:else}
				{#each grouped as group (group.name)}
					<div class="flex items-center gap-1.5 px-2 py-1">
						<p class="text-micro uppercase tracking-wide text-muted-foreground">
							{group.name}
						</p>
						{#if group.plugin}
							<span
								class="rounded border border-border px-1 text-micro text-muted-foreground"
							>
								plugin
							</span>
						{/if}
					</div>
					{#each group.entries as choice (choice.key)}
						{@const index = flat.indexOf(choice)}
						<button
							type="button"
							onclick={() => choose(choice)}
							onmouseenter={() => (rawIndex = index)}
							class={cn(
								'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors',
								index === activeIndex
									? 'bg-accent text-accent-foreground'
									: 'hover:bg-accent/60',
							)}
						>
							<Icon
								name="blocks"
								size={14}
								class="shrink-0 text-muted-foreground"
							/>
							<span class="min-w-0 flex-1 truncate text-body">
								{choice.name ?? 'Blank step'}
							</span>
							<span class="shrink-0 truncate text-micro text-muted-foreground">
								{choice.name ? choice.note : choice.nodeType}
							</span>
						</button>
					{/each}
				{/each}
			{/if}
		</div>
		{#if !loading && (skippedCount > 0 || hasMore)}
			<p class="mt-2 px-2 text-micro text-muted-foreground">
				{#if skippedCount > 0}
					{skippedCount} row(s) skipped: incomplete template data.
				{/if}
				{#if hasMore}
					Showing the first page only; search to narrow down.
				{/if}
			</p>
		{/if}
		{#if !loading && grouped.some((group) => group.plugin)}
			<p class="mt-1 px-2 text-micro text-muted-foreground">
				Plugin node types run only once the plugin that owns them is installed.
			</p>
		{/if}
	</div>
</Dialog>
