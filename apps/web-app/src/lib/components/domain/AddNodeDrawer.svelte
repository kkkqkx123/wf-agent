<script lang="ts">
	import Dialog from '@wf-agent/ui/components/Dialog.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import type { CanvasPosition } from '#lib/components/domain/GraphCanvas.svelte';
	import {
		DEFAULT_NODE_TYPE,
		isBuiltinNodeType,
		normalizeNodeType,
	} from '#lib/graph/node-kind';
	import {
		listNodeTemplates,
		type NodeTemplateSummary,
	} from '#lib/services/node-templates';
	import { toasts } from '#lib/stores/toast.svelte';
	import { t } from '#lib/i18n';
	import { cn } from '@wf-agent/ui/cn';

	interface Props {
		open: boolean;
		/** Drop coordinates the chosen node will land on, if known. */
		position?: CanvasPosition | null;
		/**
		 * Pointer location that opened the picker. On wide viewports the
		 * picker anchors near it instead of opening a centered dialog;
		 * narrow viewports always fall back to the dialog.
		 */
		anchor?: { x: number; y: number } | null;
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
		anchor = null,
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
				t('domain.addNode.loadError'),
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
				name: t('domain.addNode.blankGroup'),
				plugin: false,
				entries: [
					{
						key: 'blank',
						nodeType: DEFAULT_NODE_TYPE,
						name: null,
						note: t('domain.addNode.unnamedStep'),
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

	// Wide viewports anchor the picker near the pointer that opened it;
	// narrow viewports and keyboard-only opens keep the centered dialog.
	const anchored = $derived.by(() => {
		if (!open || !anchor) return null;
		if (typeof window === 'undefined') return null;
		if (!window.matchMedia('(min-width: 1024px)').matches) return null;
		const width = 544;
		const left = Math.max(8, Math.min(anchor.x, window.innerWidth - width - 8));
		const top = Math.max(8, Math.min(anchor.y + 8, window.innerHeight - 480));
		return { left, top, width };
	});

	let searchInput: HTMLInputElement | null = $state(null);

	$effect(() => {
		if (open) searchInput?.focus();
	});
</script>

{#snippet picker()}
	<div
		onkeydown={(event) => {
			onkeydown(event);
			if (event.key === 'Escape') {
				event.stopPropagation();
				open = false;
				onclose?.();
			}
		}}
		role="presentation"
	>
		<div class="relative">
			<Icon
				name="search"
				size={15}
				class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
			/>
			<input
				bind:this={searchInput}
				bind:value={query}
				placeholder={t('domain.addNode.searchPlaceholder')}
				aria-label={t('domain.addNode.searchPlaceholder')}
				class="h-9 w-full rounded-md border border-input bg-card pl-8 pr-2 text-body text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[hsl(var(--ring))]"
			/>
		</div>
		{#if position}
			<p class="mt-1 text-micro text-muted-foreground">
				{t('domain.addNode.dropsAt', { x: position.x, y: position.y })}
			</p>
		{/if}
		<div class="mt-2 max-h-80 overflow-y-auto">
			{#if loading}
				<p class="px-2 py-6 text-center text-caption text-muted-foreground">
					{t('common.loading')}
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
								{t('domain.addNode.plugin')}
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
								{choice.name ?? t('domain.addNode.blankStep')}
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
					{t('domain.addNode.skipped', { count: skippedCount })}
				{/if}
				{#if hasMore}
					{t('domain.addNode.firstPageOnly')}
				{/if}
			</p>
		{/if}
		{#if !loading && grouped.some((group) => group.plugin)}
			<p class="mt-1 px-2 text-micro text-muted-foreground">
				{t('domain.addNode.pluginHint')}
			</p>
		{/if}
	</div>
{/snippet}

{#if open && anchored}
	<div class="fixed inset-0 z-50" role="presentation">
		<button
			type="button"
			aria-label={t('common.close')}
			class="absolute inset-0 bg-[hsl(var(--overlay))]"
			onclick={() => {
				open = false;
				onclose?.();
			}}
		></button>
		<div
			role="dialog"
			aria-modal="true"
			aria-label={t('domain.addNode.title')}
			class="absolute overflow-hidden rounded-xl border border-border bg-popover text-popover-foreground shadow-popover"
			style:left={`${anchored.left}px`}
			style:top={`${anchored.top}px`}
			style:width={`${anchored.width}px`}
		>
			<div class="border-b border-border px-4 py-3">
				<p class="text-title font-semibold">{t('domain.addNode.title')}</p>
				<p class="mt-0.5 text-caption text-muted-foreground">
					{t('domain.addNode.description')}
				</p>
			</div>
			<div class="max-h-[60vh] overflow-y-auto px-4 py-3">
				{@render picker()}
			</div>
		</div>
	</div>
{:else}
	<Dialog
		bind:open
		title={t('domain.addNode.title')}
		description={t('domain.addNode.description')}
		width="34rem"
		{onclose}
	>
		{@render picker()}
	</Dialog>
{/if}
