<script lang="ts">
	import type {
		ContextEvolutionStep,
		ExecutionHistory,
		ExecutionKind,
		IterationRecord,
		KeyValue,
		StatusTransition,
		TimelineEntry,
	} from '#lib/types/models';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import KeyValueList from './KeyValueList.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import Timeline from './Timeline.svelte';
	import TimelineOutline from './TimelineOutline.svelte';
	import { getExecutionHistory } from '#lib/services/executions';
	import {
		formatDateTime,
		formatDuration,
		formatNumber,
	} from '#lib/utils/format';
	import { cn } from '@wf-agent/ui/cn';
	import { t } from '#lib/i18n';

	interface Props {
		/**
		 * The panel loads its own sections: each one is fetched the first
		 * time it is expanded, so a section nobody opens never costs a read.
		 */
		executionId: string;
		class?: string;
	}

	let { executionId, class: className = '' }: Props = $props();

	/** Sections this panel renders. Node records live on the Trace tab. */
	type PanelSection =
		'timeline' | 'iterations' | 'context' | 'transitions' | 'variables';

	/** Section copy resolves through `t` inside `$derived`, so labels follow
	 * the active locale reactively. */
	const SECTIONS = $derived<
		Array<{ id: PanelSection; title: string; unit: string; empty: string }>
	>([
		{
			id: 'timeline',
			title: t('domain.history.timeline'),
			unit: t('domain.history.timelineUnit'),
			empty: t('domain.history.timelineEmpty'),
		},
		{
			id: 'iterations',
			title: t('domain.history.iterations'),
			unit: t('domain.history.iterationsUnit'),
			empty: t('domain.history.iterationsEmpty'),
		},
		{
			id: 'context',
			title: t('domain.history.context'),
			unit: t('domain.history.contextUnit'),
			empty: t('domain.history.contextEmpty'),
		},
		{
			id: 'transitions',
			title: t('domain.history.transitions'),
			unit: t('domain.history.transitionsUnit'),
			empty: t('domain.history.transitionsEmpty'),
		},
		{
			id: 'variables',
			title: t('domain.history.variables'),
			unit: t('domain.history.variablesUnit'),
			empty: t('domain.history.variablesEmpty'),
		},
	]);

	let executionType = $state<ExecutionKind | null>(null);
	let timeline = $state<TimelineEntry[]>([]);
	let timelineLimit = $state(0);
	let iterations = $state<IterationRecord[]>([]);
	let contextEvolution = $state<ContextEvolutionStep[]>([]);
	let statusTransitions = $state<StatusTransition[]>([]);
	let variables = $state<KeyValue[]>([]);
	let loaded = $state<PanelSection[]>([]);
	let expanded = $state<PanelSection[]>(['timeline']);
	let loading = $state<PanelSection | null>(null);
	let failed = $state<Partial<Record<PanelSection, string>>>({});

	const counts = $derived<Record<PanelSection, number>>({
		timeline: timeline.length,
		iterations: iterations.length,
		context: contextEvolution.length,
		transitions: statusTransitions.length,
		variables: variables.length,
	});

	/**
	 * Fold one sectioned read into what earlier expands already collected.
	 * The response answers only for the section that was asked for, so the
	 * other sections arrive empty and must not erase fetched data.
	 */
	function apply(section: PanelSection, fresh: ExecutionHistory): void {
		executionType = fresh.executionType;
		switch (section) {
			case 'timeline':
				timeline = fresh.timeline;
				timelineLimit = fresh.timelineLimit;
				return;
			case 'iterations':
				iterations = fresh.iterations;
				return;
			case 'context':
				contextEvolution = fresh.contextEvolution;
				return;
			case 'transitions':
				statusTransitions = fresh.statusTransitions;
				return;
			case 'variables':
				variables = fresh.variables;
				return;
		}
	}

	async function ensureLoaded(section: PanelSection): Promise<void> {
		if (loaded.includes(section) || loading === section) return;
		loading = section;
		try {
			const fresh = await getExecutionHistory(executionId, [section]);
			apply(section, fresh);
			loaded = [...loaded, section];
			failed = { ...failed, [section]: undefined };
		} catch (e: unknown) {
			failed = {
				...failed,
				[section]: e instanceof Error
					? e.message
					: t('domain.history.loadFailed'),
			};
		} finally {
			loading = null;
		}
	}

	function toggle(section: PanelSection): void {
		if (expanded.includes(section)) {
			expanded = expanded.filter((id) => id !== section);
			return;
		}
		expanded = [...expanded, section];
		void ensureLoaded(section);
	}

	// The panel opens on the timeline, so it loads with the component; the
	// remaining sections load the first time they are expanded.
	void ensureLoaded('timeline');
</script>

<div class={cn('space-y-4', className)}>
	{#each SECTIONS as section (section.id)}
		{@const isExpanded = expanded.includes(section.id)}
		<section class="overflow-hidden rounded-lg border border-border bg-card">
			<button
				type="button"
				onclick={() => toggle(section.id)}
				aria-expanded={isExpanded}
				class="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-accent/50"
			>
				<span class="min-w-0 flex-1 text-body font-medium text-foreground">
					{section.title}
				</span>
				{#if loaded.includes(section.id)}
					<span class="shrink-0 text-micro text-muted-foreground">
						{formatNumber(counts[section.id])}
						{section.unit}
					</span>
				{/if}
				<Icon
					name="chevron-down"
					size={14}
					class={cn(
						'shrink-0 text-muted-foreground transition-transform duration-150',
						isExpanded && 'rotate-180',
					)}
				/>
			</button>

			{#if isExpanded}
				<div class="animate-panel-in border-t border-border p-3">
					{#if failed[section.id]}
						<div class="flex flex-wrap items-center justify-between gap-2">
							<p class="text-caption text-destructive">
								{failed[section.id]}
							</p>
							<Button
								variant="outline"
								size="sm"
								onclick={() => void ensureLoaded(section.id)}
							>
								{t('common.retry')}
							</Button>
						</div>
					{:else if !loaded.includes(section.id)}
						<Skeleton lines={3} />
					{:else if counts[section.id] === 0}
						<p class="text-caption text-muted-foreground">
							{section.empty}
						</p>
					{:else if section.id === 'timeline'}
						<p class="mb-2 text-micro text-muted-foreground">
							{#if timeline.length >= timelineLimit}
								{t('domain.history.timelineCap', {
									count: formatNumber(timelineLimit),
								})}
							{:else}
								{t('domain.history.readLimit', {
									count: formatNumber(timelineLimit),
								})}
							{/if}
						</p>
						<div class="grid gap-3 lg:grid-cols-[minmax(0,1fr)_12rem]">
							<Timeline entries={timeline} />
							<TimelineOutline entries={timeline} />
						</div>
					{:else if section.id === 'iterations'}
						<ul
							class="divide-y divide-border rounded-lg border border-border bg-card"
						>
							{#each iterations as iteration (iteration.iteration)}
								<li class="space-y-1 px-3 py-2">
									<div class="flex flex-wrap items-center gap-2">
										<span class="text-body font-medium text-foreground">
											{t('domain.history.iteration', {
												number: iteration.iteration,
											})}
										</span>
										<span class="text-micro tabular-nums text-muted-foreground">
											{formatDuration(iteration.durationMs)}
										</span>
										<span class="text-micro text-muted-foreground">
											{t('domain.history.toolCalls', {
												count: formatNumber(iteration.toolCallCount),
											})}
										</span>
									</div>
									{#if iteration.responseContent}
										<p class="text-caption text-muted-foreground">
											{iteration.responseContent}
										</p>
									{/if}
									{#if iteration.toolCalls.length > 0}
										<ul class="flex flex-wrap gap-1.5 pt-0.5">
											{#each iteration.toolCalls as tool (tool.name)}
												<li
													class="rounded border border-border px-1.5 text-micro text-muted-foreground"
												>
													{tool.name} · {formatDuration(tool.durationMs)}
												</li>
											{/each}
										</ul>
									{/if}
								</li>
							{/each}
						</ul>
					{:else if section.id === 'context'}
						<ol class="space-y-1.5 rounded-lg border border-border bg-card p-3">
							{#each contextEvolution as step, index (`${step.iteration}-${index}`)}
								<li class="flex flex-wrap items-baseline gap-2">
									<span class="text-micro tabular-nums text-muted-foreground">
										{formatDateTime(step.timestamp)}
									</span>
									<span class="text-caption text-foreground">
										{step.description}
									</span>
									<StatusBadge status={step.status} size="sm" />
								</li>
							{/each}
						</ol>
					{:else if section.id === 'transitions'}
						<ol class="space-y-1 rounded-lg border border-border bg-card p-3">
							{#each statusTransitions as transition, index (`${transition.timestamp}-${index}`)}
								<li class="flex flex-wrap items-center gap-2">
									<span class="text-micro tabular-nums text-muted-foreground">
										{formatDateTime(transition.timestamp)}
									</span>
									<StatusBadge status={transition.from} size="sm" />
									<span
										aria-hidden="true"
										class="text-micro text-muted-foreground"
									>
										→
									</span>
									<StatusBadge status={transition.to} size="sm" />
								</li>
							{/each}
						</ol>
					{:else}
						<div class="rounded-lg border border-border bg-card p-3">
							<KeyValueList items={variables} dense />
						</div>
					{/if}
				</div>
			{/if}
		</section>
	{/each}

	{#if executionType}
		<p class="text-micro text-muted-foreground">
			{#if executionType === 'agent_loop'}
				{t('domain.history.agentLoopHint')}
			{:else}
				{t('domain.history.workflowHint')}
			{/if}
		</p>
	{/if}
</div>
