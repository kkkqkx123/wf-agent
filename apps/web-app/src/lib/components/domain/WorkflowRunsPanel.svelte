<script lang="ts">
	import { onMount } from 'svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import FilterBar from '@wf-agent/ui/components/FilterBar.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import { toneText } from '@wf-agent/ui/components/variants';
	import { listExecutions } from '$lib/services/executions';
	import { executeWorkflow } from '$lib/services/workflows';
	import type { Execution } from '$lib/types/models';
	import {
		formatDateTime,
		formatDuration,
		formatNumber,
	} from '$lib/utils/format';
	import { statusTone, toneColorVar } from '@wf-agent/ui/status';
	import { toasts } from '$lib/stores/toast.svelte';
	import { resolve } from '$app/paths';

	interface Props {
		workflowId: string;
	}

	let { workflowId }: Props = $props();

	/** Wire values of the backend execution status enum; the list endpoint
	 * matches them with exact equality, so aliasing them would filter to
	 * nothing. */
	const STATUS_OPTIONS = [
		{ value: 'created', label: 'Created' },
		{ value: 'running', label: 'Running' },
		{ value: 'paused', label: 'Paused' },
		{ value: 'completed', label: 'Completed' },
		{ value: 'failed', label: 'Failed' },
		{ value: 'stopped', label: 'Stopped' },
		{ value: 'cancelled', label: 'Cancelled' },
		{ value: 'timeout', label: 'Timeout' },
	];

	let runs = $state<Execution[]>([]);
	let runsError = $state<string | null>(null);
	let runsLoading = $state(false);
	let query = $state('');
	let status = $state('');
	let rerunId = $state<string | null>(null);

	async function loadRuns(): Promise<void> {
		runsLoading = true;
		runsError = null;
		try {
			const result = await listExecutions({
				workflowId,
				limit: 50,
				status: status || undefined,
			});
			runs = result.items;
		} catch (e) {
			runsError = e instanceof Error ? e.message : 'Runs failed to load.';
		} finally {
			runsLoading = false;
		}
	}

	/** Re-runs the current workflow definition with the input recorded on the
	 * run. The execute entry point has no version parameter, so a run made
	 * against an older version replays against the definition live now. */
	async function rerun(run: Execution): Promise<void> {
		rerunId = run.id;
		try {
			const executionId = await executeWorkflow(workflowId, run.input ?? null);
			toasts.success('Re-run started', executionId);
			await loadRuns();
		} catch (e) {
			toasts.error('Re-run failed', e instanceof Error ? e.message : undefined);
		} finally {
			rerunId = null;
		}
	}

	// The status travels to the server; the free-text query only narrows the
	// page already fetched, which is why it filters ids instead of payloads.
	const visibleRuns = $derived.by(() => {
		const needle = query.trim().toLowerCase();
		if (!needle) return runs;
		return runs.filter((run) => run.id.toLowerCase().includes(needle));
	});

	onMount(() => {
		void loadRuns();
	});
</script>

<div class="space-y-2.5">
	<FilterBar
		bind:query
		bind:status
		statusOptions={STATUS_OPTIONS}
		placeholder="Filter by execution id…"
		onstatuschange={() => void loadRuns()}
	>
		{#snippet trailing()}
			<Button
				variant="ghost"
				size="sm"
				disabled={runsLoading}
				onclick={() => void loadRuns()}
			>
				<Icon name="refresh" size={13} />
				Refresh
			</Button>
		{/snippet}
	</FilterBar>

	{#if runsLoading}
		<Skeleton lines={4} class="rounded-lg border border-border bg-card p-4" />
	{:else if runsError}
		<ErrorState
			title="Runs failed to load"
			description={runsError}
			onretry={() => void loadRuns()}
			class="rounded-lg border border-border bg-card"
		/>
	{:else if visibleRuns.length === 0}
		<EmptyState
			icon="activity"
			title={runs.length === 0 ? 'No runs yet' : 'No matching runs'}
			description={runs.length === 0
				? 'Executions of this workflow appear here once it runs.'
				: 'No execution matches the current filter.'}
			class="rounded-lg border border-border bg-card"
		/>
	{:else}
		<Card title="Runs" bodyClass="p-0">
			<ul class="divide-y divide-border">
				{#each visibleRuns as run (run.id)}
					{@const tone = statusTone(run.status)}
					<li
						class="flex items-start gap-2 border-l-2 px-3 py-2"
						style:border-left-color={toneColorVar(tone)}
					>
						<div class="min-w-0 flex-1">
							<div class="flex items-center gap-2">
								<a
									href={resolve('/executions/[id]', { id: run.id })}
									class="truncate font-mono text-caption underline-offset-2 hover:underline"
								>
									{run.id}
								</a>
								<StatusBadge status={run.status} size="sm" dot={false} />
							</div>
							<p class="mt-0.5 text-micro text-muted-foreground">
								<span class="tabular-nums">{formatDateTime(run.startedAt)}</span
								>
								{#if run.durationMs !== null}
									· <span class="tabular-nums"
										>{formatDuration(run.durationMs)}</span
									>
								{/if}
								{#if run.failedNodes > 0}
									· <span class={toneText('danger', 'text-muted-foreground')}
										>{formatNumber(run.failedNodes)} failed node(s)</span
									>
								{/if}
							</p>
							{#if run.error}
								<p
									class="mt-0.5 truncate font-mono text-micro text-destructive"
									title={run.error}
								>
									{run.error}
								</p>
							{/if}
						</div>
						<Button
							variant="ghost"
							size="sm"
							class="shrink-0"
							disabled={rerunId === run.id}
							title="Re-run with the input recorded on this run"
							onclick={() => void rerun(run)}
						>
							<Icon name="refresh" size={13} />
							{rerunId === run.id ? 'Re-running…' : 'Re-run'}
						</Button>
					</li>
				{/each}
			</ul>
		</Card>
	{/if}
</div>
