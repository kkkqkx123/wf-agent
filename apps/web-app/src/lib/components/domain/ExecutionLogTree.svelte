<script lang="ts">
	import type { NodeTrace, TimelineEntry } from '#lib/types/models';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import JsonViewer from '@wf-agent/ui/components/JsonViewer.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import { SvelteSet, SvelteMap } from 'svelte/reactivity';
	import { cn } from '@wf-agent/ui/cn';
	import {
		formatDateTime,
		formatDuration,
		formatNumber,
	} from '#lib/utils/format';
	import { statusTone } from '@wf-agent/ui/status';
	import { t } from '#lib/i18n';

	interface Props {
		traces: NodeTrace[];
		timeline: TimelineEntry[];
		skipped?: number;
		loading?: boolean;
		error?: string | null;
		selectedNodeId?: string | null;
		onretry?: () => void;
		onlocate?: (nodeId: string) => void;
		class?: string;
	}

	let {
		traces,
		timeline,
		skipped = 0,
		loading = false,
		error = null,
		selectedNodeId = null,
		onretry,
		onlocate,
		class: className = '',
	}: Props = $props();

	let query = $state('');
	let expanded = new SvelteSet<string>();
	let focusId = $state<string | null>(null);

	interface LogRow {
		id: string;
		depth: number;
		trace: NodeTrace | null;
		entry: TimelineEntry | null;
		nodeId: string | null;
		title: string;
		status: string;
		at: string | null;
		durationMs: number | null;
		children: TimelineEntry[];
	}

	const filteredTraces = $derived.by(() => {
		const needle = query.trim().toLowerCase();
		if (!needle) return traces;
		return traces.filter(
			(trace) =>
				trace.nodeId.toLowerCase().includes(needle) ||
				trace.nodeName.toLowerCase().includes(needle) ||
				trace.nodeType.toLowerCase().includes(needle) ||
				trace.status.toLowerCase().includes(needle),
		);
	});

	const rows = $derived.by<LogRow[]>(() => {
		const byNode = new SvelteMap<string, TimelineEntry[]>();
		for (const entry of timeline) {
			if (!entry.nodeId) continue;
			const list = byNode.get(entry.nodeId) ?? [];
			list.push(entry);
			byNode.set(entry.nodeId, list);
		}
		const matched = new Set(filteredTraces.map((trace) => trace.nodeId));
		const needle = query.trim().toLowerCase();
		const out: LogRow[] = filteredTraces.map((trace) => ({
			id: `trace:${trace.nodeId}`,
			depth: 0,
			trace,
			entry: null,
			nodeId: trace.nodeId,
			title: trace.nodeName || trace.nodeId,
			status: trace.status,
			at: trace.startedAt,
			durationMs: trace.durationMs,
			children: byNode.get(trace.nodeId) ?? [],
		}));
		for (const entry of timeline) {
			const owner = entry.nodeId ?? null;
			if (owner && matched.has(owner)) continue;
			if (
				needle &&
				!entry.title.toLowerCase().includes(needle) &&
				!entry.kind.toLowerCase().includes(needle) &&
				!entry.status.toLowerCase().includes(needle)
			) {
				continue;
			}
			out.push({
				id: `event:${entry.id}`,
				depth: 0,
				trace: null,
				entry,
				nodeId: owner,
				title: entry.title || entry.kind,
				status: entry.status,
				at: entry.at,
				durationMs: null,
				children: [],
			});
		}
		return out;
	});

	const flatIds = $derived(rows.map((row) => row.id));

	function toggle(id: string): void {
		if (expanded.has(id)) expanded.delete(id);
		else expanded.add(id);
	}

	function moveFocus(delta: number): void {
		if (flatIds.length === 0) return;
		const index = focusId ? flatIds.indexOf(focusId) : -1;
		const next =
			flatIds[(index + delta + flatIds.length) % flatIds.length] ?? null;
		focusId = next;
		if (next) {
			document.getElementById(next)?.focus({ preventScroll: false });
		}
	}

	function onTreeKey(event: KeyboardEvent): void {
		if (event.key === 'j' || event.key === 'ArrowDown') {
			event.preventDefault();
			moveFocus(1);
		} else if (event.key === 'k' || event.key === 'ArrowUp') {
			event.preventDefault();
			moveFocus(-1);
		} else if (event.key === 'Home') {
			event.preventDefault();
			focusId = flatIds[0] ?? null;
			if (focusId) document.getElementById(focusId)?.focus();
		} else if (event.key === 'End') {
			event.preventDefault();
			focusId = flatIds[flatIds.length - 1] ?? null;
			if (focusId) document.getElementById(focusId)?.focus();
		} else if ((event.key === 'Enter' || event.key === ' ') && focusId) {
			const target = event.target as HTMLElement | null;
			if (target?.tagName === 'INPUT') return;
			event.preventDefault();
			toggle(focusId);
		}
	}

	$effect(() => {
		if (!selectedNodeId) return;
		const id = `trace:${selectedNodeId}`;
		if (rows.some((row) => row.id === id)) {
			expanded.add(id);
			focusId = id;
			queueMicrotask(() =>
				document.getElementById(id)?.scrollIntoView({ block: 'nearest' }),
			);
		}
	});
</script>

<div class={cn('space-y-2', className)}>
	{#if loading}
		<Skeleton lines={5} class="rounded-lg border border-border bg-card p-4" />
	{:else if error}
		<ErrorState
			title={t('domain.nodeTrace.failedTitle')}
			description={error}
			onretry={() => onretry?.()}
			class="rounded-lg border border-border bg-card"
		/>
	{:else if traces.length === 0 && timeline.length === 0}
		<EmptyState
			icon="activity"
			title={t('domain.nodeTrace.emptyTitle')}
			description={t('domain.nodeTrace.emptyDescription')}
			class="rounded-lg border border-border bg-card"
		/>
	{:else}
		<div class="relative min-w-40">
			<Icon
				name="search"
				size={14}
				class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
			/>
			<Input
				bind:value={query}
				placeholder={t('domain.nodeTrace.searchPlaceholder')}
				class="pl-8"
			/>
		</div>
		<p class="text-micro text-muted-foreground">
			{formatNumber(filteredTraces.length)} nodes · {formatNumber(
				timeline.length,
			)} events{#if skipped > 0}
				· {formatNumber(skipped)} skipped{/if}
		</p>
		<div
			role="tree"
			aria-label={t('domain.inspector.sections')}
			tabindex="-1"
			onkeydown={onTreeKey}
		>
			{#each rows as row (row.id)}
				{@const tone = statusTone(row.status)}
				<div
					id={row.id}
					role="treeitem"
					aria-level={row.depth + 1}
					aria-expanded={row.children.length > 0 || row.trace
						? expanded.has(row.id)
						: undefined}
					aria-selected={selectedNodeId === row.nodeId}
					tabindex={focusId === row.id || (!focusId && row === rows[0])
						? 0
						: -1}
					onfocus={() => (focusId = row.id)}
					class={cn(
						'overflow-hidden rounded-lg border bg-card transition-colors',
						selectedNodeId === row.nodeId ? 'border-ring/60' : 'border-border',
						tone === 'danger' && 'border-destructive/40 bg-destructive/[0.04]',
					)}
				>
					<button
						type="button"
						onclick={() => {
							focusId = row.id;
							toggle(row.id);
						}}
						aria-expanded={expanded.has(row.id)}
						class="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-accent/50"
					>
						<span
							aria-hidden="true"
							class="font-mono text-micro text-muted-foreground"
						>
							{expanded.has(row.id) ? '└─' : '├─'}
						</span>
						<span class="min-w-0 flex-1">
							<span
								class="block truncate font-mono text-caption text-foreground"
							>
								{row.title}
							</span>
							<span class="block truncate text-micro text-muted-foreground">
								{#if row.trace}
									{row.trace.nodeType} ·
									{row.at ? formatDateTime(row.at) : '—'}
								{:else if row.entry}
									{row.entry.kind} ·
									{row.at ? formatDateTime(row.at) : '—'}
								{/if}
							</span>
						</span>
						{#if row.durationMs !== null}
							<span
								class="shrink-0 text-micro tabular-nums text-muted-foreground"
							>
								{formatDuration(row.durationMs)}
							</span>
						{/if}
						<StatusBadge status={row.status} size="sm" />
						<Icon
							name="chevron-down"
							size={14}
							class={cn(
								'shrink-0 text-muted-foreground transition-transform duration-150',
								expanded.has(row.id) && 'rotate-180',
							)}
						/>
					</button>
					{#if expanded.has(row.id)}
						<div
							role="group"
							class="animate-panel-in space-y-2 border-t border-border px-3 py-2.5"
						>
							{#if row.trace}
								{#if row.trace.error}
									<p
										class="rounded-md border border-destructive/25 bg-destructive/10 px-2 py-1 font-mono text-micro break-words whitespace-pre-wrap text-foreground"
									>
										{row.trace.error}
									</p>
								{/if}
								<details>
									<summary
										class="cursor-pointer text-micro text-muted-foreground"
									>
										Input / Output
									</summary>
									<div class="mt-1.5 space-y-2">
										<JsonViewer value={row.trace.input} collapsed />
										<JsonViewer value={row.trace.output} collapsed />
									</div>
								</details>
								{#if row.children.length > 0}
									<ol class="relative space-y-2 border-l border-border pl-3">
										{#each row.children as child (child.id)}
											<li class="text-caption">
												<span class="font-medium text-foreground"
													>{child.title}</span
												>
												<span class="ml-1.5 text-micro text-muted-foreground">
													{child.kind} ·
													{child.at ? formatDateTime(child.at) : '—'}
												</span>
												<p class="text-micro text-muted-foreground">
													{child.detail}
												</p>
											</li>
										{/each}
									</ol>
								{/if}
								{#if row.nodeId && onlocate}
									<button
										type="button"
										class="text-micro text-foreground underline-offset-2 hover:underline"
										onclick={() => row.nodeId && onlocate(row.nodeId)}
									>
										{t('domain.inspector.locateOnGraph')}
									</button>
								{/if}
							{:else if row.entry}
								<p
									class="text-caption break-words whitespace-pre-wrap text-foreground"
								>
									{row.entry.detail}
								</p>
								{#if row.nodeId && onlocate}
									<button
										type="button"
										class="text-micro text-foreground underline-offset-2 hover:underline"
										onclick={() => row.nodeId && onlocate(row.nodeId)}
									>
										{t('domain.inspector.locateOnGraph')}
									</button>
								{/if}
							{/if}
						</div>
					{/if}
				</div>
			{:else}
				<p class="text-caption text-muted-foreground">
					{t('domain.nodeTrace.noFilterMatch')}
				</p>
			{/each}
		</div>
	{/if}
</div>
