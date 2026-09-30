<script lang="ts">
	import type {
		LlmReasoningStep,
		NodeInputContext,
		NodeTrace,
	} from '$lib/types/models';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import Select from '@wf-agent/ui/components/Select.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import NodeTraceCard from './NodeTraceCard.svelte';
	import {
		NODE_TRACE_STATUS_FILTERS,
		filterNodeTraces,
		getNodeInputContext,
		getNodeLlmReasoning,
		summarizeNodeTraces,
		type NodeTraceStatusFilter,
	} from '$lib/services/node-trace';
	import { formatNumber } from '$lib/utils/format';
	import { toasts } from '$lib/stores/toast.svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { cn } from '@wf-agent/ui/cn';

	interface Props {
		executionId: string;
		traces: NodeTrace[];
		skipped?: number;
		loading?: boolean;
		error?: string | null;
		selectedNodeId?: string | null;
		onretry?: () => void;
		onlocate?: (nodeId: string) => void;
		class?: string;
	}

	let {
		executionId,
		traces,
		skipped = 0,
		loading = false,
		error = null,
		selectedNodeId = null,
		onretry,
		onlocate,
		class: className = '',
	}: Props = $props();

	let search = $state('');
	let status = $state<NodeTraceStatusFilter | string>('all');
	let expanded = new SvelteSet<string>();
	let details = $state<Record<string, NodeInputContext | null>>({});
	let reasoning = $state<Record<string, LlmReasoningStep[]>>({});
	let pending = new SvelteSet<string>();

	const visible = $derived(filterNodeTraces(traces, status, search));
	const summary = $derived(summarizeNodeTraces(traces));

	const statusOptions = $derived(
		NODE_TRACE_STATUS_FILTERS.map((entry) => ({ ...entry })),
	);

	function toggle(nodeId: string): void {
		const opens = !expanded.has(nodeId);
		if (opens) expanded.add(nodeId);
		else expanded.delete(nodeId);
		if (opens) void loadDetail(nodeId);
	}

	async function loadDetail(nodeId: string): Promise<void> {
		if (details[nodeId] !== undefined || reasoning[nodeId] !== undefined)
			return;
		pending.add(nodeId);
		try {
			const [context, steps] = await Promise.all([
				getNodeInputContext(executionId, nodeId),
				getNodeLlmReasoning(executionId, nodeId),
			]);
			details = { ...details, [nodeId]: context };
			reasoning = { ...reasoning, [nodeId]: steps };
		} catch (e) {
			toasts.error(
				'Node context failed to load',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			pending.delete(nodeId);
		}
	}

	// A graph selection drives the list: the matching row opens and scrolls
	// into view so the two views stay in sync without a second click.
	$effect(() => {
		const nodeId = selectedNodeId;
		if (!nodeId) return;
		if (!traces.some((trace) => trace.nodeId === nodeId)) return;
		if (!expanded.has(nodeId)) {
			expanded.add(nodeId);
			void loadDetail(nodeId);
		}
		queueMicrotask(() =>
			document
				.getElementById(`node-trace-${CSS.escape(nodeId)}`)
				?.scrollIntoView({ block: 'nearest' }),
		);
	});
</script>

<div class={cn('space-y-2.5', className)}>
	{#if loading}
		<Skeleton lines={5} class="rounded-lg border border-border bg-card p-4" />
	{:else if error}
		<ErrorState
			title="Node traces failed to load"
			description={error}
			onretry={() => onretry?.()}
			class="rounded-lg border border-border bg-card"
		/>
	{:else if traces.length === 0}
		<EmptyState
			icon="activity"
			title="No node traces"
			description="Node-level records appear once the workflow has executed a node."
			class="rounded-lg border border-border bg-card"
		/>
	{:else}
		<div class="flex flex-wrap items-center gap-2">
			<div class="relative min-w-40 flex-1">
				<Icon
					name="search"
					size={14}
					class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					bind:value={search}
					placeholder="Search node id, name or type…"
					class="pl-8"
				/>
			</div>
			<Select
				bind:value={status}
				options={statusOptions}
				placeholder="All nodes"
				class="w-36"
			/>
		</div>

		<p class="text-micro text-muted-foreground">
			{formatNumber(summary.total)} nodes · {formatNumber(summary.failed)} failed
			·
			{formatNumber(summary.retries)} retries
			{#if visible.length !== traces.length}
				· showing {formatNumber(visible.length)}
			{/if}
			{#if skipped > 0}
				· {formatNumber(skipped)} unaddressable row(s) dropped
			{/if}
		</p>

		{#each visible as trace (trace.nodeId)}
			<div id={`node-trace-${trace.nodeId}`}>
				<NodeTraceCard
					{trace}
					expanded={expanded.has(trace.nodeId)}
					highlighted={selectedNodeId === trace.nodeId}
					detail={details[trace.nodeId] ?? null}
					reasoning={reasoning[trace.nodeId] ?? null}
					loading={pending.has(trace.nodeId)}
					ontoggle={() => toggle(trace.nodeId)}
					onlocate={() => onlocate?.(trace.nodeId)}
				/>
			</div>
		{:else}
			<p class="text-caption text-muted-foreground">
				No node matches the current filter.
			</p>
		{/each}
	{/if}
</div>
