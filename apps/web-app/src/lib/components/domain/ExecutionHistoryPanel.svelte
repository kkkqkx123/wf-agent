<script lang="ts">
	import type { ExecutionHistory } from '$lib/types/models';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import KeyValueList from './KeyValueList.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import Timeline from './Timeline.svelte';
	import TimelineOutline from './TimelineOutline.svelte';
	import {
		formatDateTime,
		formatDuration,
		formatNumber,
	} from '$lib/utils/format';
	import { cn } from '@wf-agent/ui/cn';

	interface Props {
		history: ExecutionHistory;
		/**
		 * Per-node records live in the trace tab, which loads them through the
		 * node-trace endpoint; this panel covers the sections that only the
		 * history endpoint reports.
		 */
		class?: string;
	}

	let { history, class: className = '' }: Props = $props();

	const isAgent = $derived(history.executionType === 'agent_loop');
	const sections = $derived([
		{ id: 'timeline', count: history.timeline.length },
		{ id: 'iterations', count: history.iterations.length },
		{ id: 'context', count: history.contextEvolution.length },
		{ id: 'transitions', count: history.statusTransitions.length },
		{ id: 'variables', count: history.variables.length },
	]);
	const empty = $derived(sections.every((section) => section.count === 0));
</script>

<div class={cn('space-y-4', className)}>
	{#if empty}
		<EmptyState
			icon="activity"
			title="No recorded history"
			description="Timeline events, iterations and transitions appear here once the execution records them."
			class="rounded-lg border border-border bg-card"
		/>
	{:else}
		{#if history.timeline.length > 0}
			<section class="space-y-2">
				<div class="flex items-baseline justify-between gap-2">
					<h3 class="text-body font-medium text-foreground">Timeline</h3>
					<span class="text-micro text-muted-foreground">
						{formatNumber(history.timeline.length)} event(s)
					</span>
				</div>
				<div class="grid gap-3 lg:grid-cols-[minmax(0,1fr)_12rem]">
					<Timeline entries={history.timeline} />
					<TimelineOutline entries={history.timeline} />
				</div>
			</section>
		{/if}

		{#if history.iterations.length > 0}
			<section class="space-y-2">
				<div class="flex items-baseline justify-between gap-2">
					<h3 class="text-body font-medium text-foreground">Iterations</h3>
					<span class="text-micro text-muted-foreground">
						{formatNumber(history.iterations.length)} iteration(s)
					</span>
				</div>
				<ul
					class="divide-y divide-border rounded-lg border border-border bg-card"
				>
					{#each history.iterations as iteration (iteration.iteration)}
						<li class="space-y-1 px-3 py-2">
							<div class="flex flex-wrap items-center gap-2">
								<span class="text-body font-medium text-foreground">
									Iteration {iteration.iteration}
								</span>
								<span class="text-micro tabular-nums text-muted-foreground">
									{formatDuration(iteration.durationMs)}
								</span>
								<span class="text-micro text-muted-foreground">
									{formatNumber(iteration.toolCallCount)} tool call(s)
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
			</section>
		{/if}

		{#if history.contextEvolution.length > 0}
			<section class="space-y-2">
				<div class="flex items-baseline justify-between gap-2">
					<h3 class="text-body font-medium text-foreground">Context</h3>
					<span class="text-micro text-muted-foreground">
						{formatNumber(history.contextEvolution.length)} step(s)
					</span>
				</div>
				<ol class="space-y-1.5 rounded-lg border border-border bg-card p-3">
					{#each history.contextEvolution as step, index (`${step.iteration}-${index}`)}
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
			</section>
		{/if}

		{#if history.statusTransitions.length > 0}
			<section class="space-y-2">
				<div class="flex items-baseline justify-between gap-2">
					<h3 class="text-body font-medium text-foreground">Transitions</h3>
					<span class="text-micro text-muted-foreground">
						{formatNumber(history.statusTransitions.length)} transition(s)
					</span>
				</div>
				<ol class="space-y-1 rounded-lg border border-border bg-card p-3">
					{#each history.statusTransitions as transition, index (`${transition.timestamp}-${index}`)}
						<li class="flex flex-wrap items-center gap-2">
							<span class="text-micro tabular-nums text-muted-foreground">
								{formatDateTime(transition.timestamp)}
							</span>
							<StatusBadge status={transition.from} size="sm" />
							<span aria-hidden="true" class="text-micro text-muted-foreground">
								→
							</span>
							<StatusBadge status={transition.to} size="sm" />
						</li>
					{/each}
				</ol>
			</section>
		{/if}

		{#if history.variables.length > 0}
			<section class="space-y-2">
				<div class="flex items-baseline justify-between gap-2">
					<h3 class="text-body font-medium text-foreground">Variables</h3>
					<span class="text-micro text-muted-foreground">
						{formatNumber(history.variables.length)} variable(s)
					</span>
				</div>
				<div class="rounded-lg border border-border bg-card p-3">
					<KeyValueList items={history.variables} dense />
				</div>
			</section>
		{/if}
	{/if}

	<p class="text-micro text-muted-foreground">
		{#if isAgent}
			Agent loop history; node-level records are on the Trace tab.
		{:else}
			Workflow history; iteration records only exist for agent loops.
		{/if}
	</p>
</div>
