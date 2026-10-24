<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import FilterBar from '@wf-agent/ui/components/FilterBar.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import { toneText } from '@wf-agent/ui/components/variants';
	import { listUnifiedExecutions } from '#lib/services/executions';
	import type { DefinitionKind, Execution } from '#lib/types/models';
	import {
		formatDateTime,
		formatDuration,
		formatNumber,
	} from '#lib/utils/format';
	import { statusTone, toneColorVar } from '@wf-agent/ui/status';
	import { cn } from '@wf-agent/ui/cn';
	import { t } from '#lib/i18n';
	import { resolve } from '$app/paths';
	import { watchListInvalidation } from '#lib/services/list-invalidation';

	interface Props {
		/** The kind of definition the runs belong to. */
		kind: DefinitionKind;
		/** The definition the runs were made against. */
		definitionId: string;
	}

	let { kind, definitionId }: Props = $props();

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
	let runsDirty = $state(false);
	let stopRunsLive: (() => void) | null = null;

	async function loadRuns(): Promise<void> {
		runsLoading = true;
		runsError = null;
		runsDirty = false;
		try {
			const result = await listUnifiedExecutions({
				limit: 50,
				status: status || undefined,
				definitionId,
			});
			runs = result.items;
		} catch (e) {
			runsError = e instanceof Error ? e.message : t('domain.runs.loadFailed');
		} finally {
			runsLoading = false;
		}
	}

	// Runs stay read-only here; creation lives in the execution workbench.
	// Each row links to its execution detail, where re-run and live
	// controls are available.

	// The status travels to the server; the free-text query only narrows the
	// page already fetched, which is why it filters ids instead of payloads.
	const visibleRuns = $derived.by(() => {
		const needle = query.trim().toLowerCase();
		if (!needle) return runs;
		return runs.filter((run) => run.id.toLowerCase().includes(needle));
	});

	onMount(() => {
		void loadRuns();
		stopRunsLive = watchListInvalidation({
			onDirty: () => {
				if (runsLoading) {
					runsDirty = true;
					return;
				}
				if (typeof document !== 'undefined' && document.hidden) {
					runsDirty = true;
					return;
				}
				void loadRuns();
			},
		});
	});

	onDestroy(() => {
		stopRunsLive?.();
		stopRunsLive = null;
	});
</script>

<div class="space-y-2.5">
	<p class="text-micro text-muted-foreground">
		Execution domain · runs are read-only here; new runs start in the execution
		workbench and re-runs live on the execution detail page.
	</p>
	{#if runsDirty}
		<div
			class="flex items-center justify-between gap-2 rounded-lg border border-border bg-card px-3 py-2"
		>
			<span class="text-caption text-muted-foreground">New runs arrived.</span>
			<Button size="sm" variant="outline" onclick={() => void loadRuns()}>
				Refresh
			</Button>
		</div>
	{/if}
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

	{#if runsLoading && runs.length === 0}
		<Skeleton lines={4} class="rounded-lg border border-border bg-card p-4" />
	{:else if runsError && runs.length === 0}
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
			{#snippet actions()}
				<Button
					variant="ghost"
					size="sm"
					href={`/executions?definition=${encodeURIComponent(`${kind}:${definitionId}`)}`}
				>
					<Icon name="play" size={13} />
					{t('page.executions.newTask')}
				</Button>
			{/snippet}
			<ul class="divide-y divide-border" aria-busy={runsLoading}>
				{#each visibleRuns as run (run.id)}
					{@const tone = statusTone(run.status)}
					<li
						class={cn(
							'flex items-start gap-2 border-l-2 px-3 py-2',
							tone === 'danger' && 'bg-destructive/[0.05]',
						)}
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
					</li>
				{/each}
				{#if runsLoading}
					<li class="px-3 py-2" aria-label={t('common.loading')}>
						<Skeleton lines={2} />
					</li>
				{/if}
			</ul>
		</Card>
	{/if}
</div>
