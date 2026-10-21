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
	import { listExecutions } from '#lib/services/executions';
	import { executeWorkflow } from '#lib/services/workflows';
	import type { Execution } from '#lib/types/models';
	import {
		formatDateTime,
		formatDuration,
		formatNumber,
	} from '#lib/utils/format';
	import { statusTone, toneColorVar } from '@wf-agent/ui/status';
	import { toasts } from '#lib/stores/toast.svelte';
	import { t } from '#lib/i18n';
	import { resolve } from '$app/paths';

	interface Props {
		workflowId: string;
	}

	let { workflowId }: Props = $props();

	/** Wire values of the backend execution status enum; the list endpoint
	 * matches them with exact equality, so aliasing them would filter to
	 * nothing. */
	const STATUS_OPTIONS = $derived([
		{ value: 'created', label: t('domain.runs.statusCreated') },
		{ value: 'running', label: t('domain.runs.statusRunning') },
		{ value: 'paused', label: t('domain.runs.statusPaused') },
		{ value: 'completed', label: t('domain.runs.statusCompleted') },
		{ value: 'failed', label: t('domain.runs.statusFailed') },
		{ value: 'stopped', label: t('domain.runs.statusStopped') },
		{ value: 'cancelled', label: t('domain.runs.statusCancelled') },
		{ value: 'timeout', label: t('domain.runs.statusTimeout') },
	]);

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
			runsError =
				e instanceof Error ? e.message : t('domain.runs.loadFailed');
		} finally {
			runsLoading = false;
		}
	}

	/** Re-runs the recorded input against the recorded version when the
	 * run carries one, so a run made against an older version replays
	 * that version instead of the definition live now. */
	async function rerun(run: Execution): Promise<void> {
		rerunId = run.id;
		try {
			const executionId = await executeWorkflow(
				workflowId,
				run.input ?? null,
				undefined,
				run.version ?? null,
			);
			toasts.success(t('domain.runs.rerunStarted'), executionId);
			await loadRuns();
		} catch (e) {
			toasts.error(
				t('domain.runs.rerunFailed'),
				e instanceof Error ? e.message : undefined,
			);
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
		placeholder={t('domain.runs.filterPlaceholder')}
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
				{t('common.refresh')}
			</Button>
		{/snippet}
	</FilterBar>

	{#if runsLoading}
		<Skeleton lines={4} class="rounded-lg border border-border bg-card p-4" />
	{:else if runsError}
		<ErrorState
			title={t('domain.runs.failedTitle')}
			description={runsError}
			onretry={() => void loadRuns()}
			class="rounded-lg border border-border bg-card"
		/>
	{:else if visibleRuns.length === 0}
		<EmptyState
			icon="activity"
			title={runs.length === 0
				? t('domain.runs.emptyTitle')
				: t('domain.runs.noMatchTitle')}
			description={runs.length === 0
				? t('domain.runs.emptyDescription')
				: t('domain.runs.noMatchDescription')}
			class="rounded-lg border border-border bg-card"
		/>
	{:else}
		<Card title={t('domain.runs.title')} bodyClass="p-0">
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
								{#if run.version}
									<span
										class="rounded border border-border px-1 font-mono text-micro text-muted-foreground"
										title={t('domain.runs.versionTitle')}
									>
										v{run.version}
									</span>
								{/if}
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
										>{t('domain.runs.failedNodes', {
											count: formatNumber(run.failedNodes),
										})}</span
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
							title={run.version
								? t('domain.runs.rerunVersionTitle', { version: run.version })
								: t('domain.runs.rerunTitle')}
							onclick={() => void rerun(run)}
						>
							<Icon name="refresh" size={13} />
							{rerunId === run.id
								? t('domain.runs.rerunning')
								: t('domain.runs.rerun')}
						</Button>
					</li>
				{/each}
			</ul>
		</Card>
	{/if}
</div>
