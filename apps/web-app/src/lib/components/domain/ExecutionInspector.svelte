<script lang="ts">
	import type { ExecutionDetail } from '$lib/types/models';
	import Card from '$lib/components/ui/Card.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import StatusBadge from './StatusBadge.svelte';
	import KeyValueList from './KeyValueList.svelte';
	import Progress from '$lib/components/ui/Progress.svelte';
	import Timeline from './Timeline.svelte';
	import ToolCallCard from './ToolCallCard.svelte';

	import {
		formatBytes,
		formatDateTime,
		formatDuration,
		formatNumber,
		shortId,
	} from '$lib/utils/format';
	import { statusTone } from '$lib/utils/status';
	import { cn } from '$lib/utils/cn';

	import type { ToolCallEntry, TimelineEntry } from '$lib/types/models';

	interface Props {
		execution: ExecutionDetail;
		toolCalls?: ToolCallEntry[];
		timeline?: TimelineEntry[];
		class?: string;
	}

	let {
		execution,
		toolCalls = [],
		timeline = [],
		class: className = '',
	}: Props = $props();

	const TABS = [
		{ id: 'overview', label: 'Overview' },
		{ id: 'timeline', label: 'Timeline' },
		{ id: 'tools', label: 'Tools' },
		{ id: 'analysis', label: 'Analysis' },
		{ id: 'state', label: 'State' },
	];

	let tab = $state('overview');

	const tone = $derived(statusTone(execution.status));
	const progressTone = $derived(
		tone === 'danger'
			? 'danger'
			: tone === 'success'
				? 'success'
				: tone === 'running'
					? 'running'
					: 'default',
	);
</script>

<div class={cn('flex h-full min-h-0 flex-col', className)}>
	<div class="border-b border-border px-3 py-3">
		<div class="flex items-start justify-between gap-2">
			<div class="min-w-0">
				<h2 class="truncate text-title font-semibold">
					{execution.workflowName}
				</h2>
				<p class="mt-0.5 font-mono text-micro text-muted-foreground">
					{execution.id}
				</p>
			</div>
			<StatusBadge status={execution.status} />
		</div>

		<div class="mt-3 space-y-1.5">
			<div
				class="flex items-center justify-between text-micro text-muted-foreground"
			>
				<span>Progress</span>
				<span class="tabular-nums"
					>{execution.tasksDone}/{execution.tasksTotal} tasks</span
				>
			</div>
			<Progress value={execution.progress} tone={progressTone} />
		</div>

		<dl class="mt-3 grid grid-cols-2 gap-x-3 gap-y-2">
			<div>
				<dt class="text-micro text-muted-foreground">Started</dt>
				<dd class="text-caption tabular-nums">
					{formatDateTime(execution.startedAt)}
				</dd>
			</div>
			<div>
				<dt class="text-micro text-muted-foreground">Duration</dt>
				<dd class="text-caption tabular-nums">
					{formatDuration(execution.durationMs)}
				</dd>
			</div>
			<div>
				<dt class="text-micro text-muted-foreground">Memory peak</dt>
				<dd class="text-caption tabular-nums">
					{formatBytes(execution.memoryPeakBytes)}
				</dd>
			</div>
			<div>
				<dt class="text-micro text-muted-foreground">Trigger</dt>
				<dd class="truncate text-caption">{execution.trigger ?? '—'}</dd>
			</div>
		</dl>
	</div>

	<Segmented items={TABS} bind:value={tab} size="sm" class="px-2" />

	<div class="min-h-0 flex-1 overflow-y-auto px-3 py-3">
		{#if tab === 'overview'}
			<div class="space-y-3">
				<Card title="Context">
					<KeyValueList items={execution.context} dense />
				</Card>
				<Card title="Current position">
					<p class="text-body">{execution.currentNode ?? 'No active node'}</p>
					<p class="mt-1 text-caption text-muted-foreground">
						{execution.failedNodes} failed nodes across {formatNumber(
							execution.tasksTotal,
						)} tasks
					</p>
				</Card>
				<Card title="Status migration">
					<ol class="space-y-1.5">
						{#each execution.migration as entry, index (index)}
							<li class="flex items-start gap-2 text-caption">
								<time class="shrink-0 tabular-nums text-muted-foreground">
									{formatDateTime(entry.at)}
								</time>
								<span class="min-w-0">
									<span class="text-foreground">{entry.from}</span>
									<span class="mx-1 text-muted-foreground">→</span>
									<span class="text-foreground">{entry.to}</span>
									<span class="block text-micro text-muted-foreground"
										>{entry.reason}</span
									>
								</span>
							</li>
						{/each}
					</ol>
				</Card>
			</div>
		{:else if tab === 'timeline'}
			<Timeline entries={timeline} />
		{:else if tab === 'tools'}
			<div class="space-y-2">
				{#each toolCalls as entry (entry.id)}
					<ToolCallCard {entry} />
				{/each}
			</div>
		{:else if tab === 'analysis'}
			<div class="space-y-3">
				<Card title="Slow nodes">
					<ul class="space-y-1.5">
						{#each execution.analysis.slowNodes as node (node.node)}
							<li class="flex items-center justify-between gap-2 text-caption">
								<span class="truncate font-mono">{node.node}</span>
								<span class="shrink-0 tabular-nums text-muted-foreground">
									{formatDuration(node.durationMs)}
								</span>
							</li>
						{/each}
					</ul>
				</Card>
				<Card title="Critical path">
					<ol class="flex flex-wrap items-center gap-1.5">
						{#each execution.analysis.criticalPath as node, index (node)}
							<li class="flex items-center gap-1.5">
								<span
									class="rounded border border-border px-1.5 py-0.5 font-mono text-micro"
								>
									{node}
								</span>
								{#if index < execution.analysis.criticalPath.length - 1}
									<span class="text-micro text-muted-foreground">→</span>
								{/if}
							</li>
						{/each}
					</ol>
				</Card>
				<Card title="Decision points">
					<div class="flex flex-wrap gap-1.5">
						{#each execution.analysis.decisionPoints as node (node)}
							<span
								class="rounded-full border border-border px-2 py-0.5 font-mono text-micro"
							>
								{node}
							</span>
						{/each}
					</div>
					<p class="mt-2 text-caption text-muted-foreground">
						{formatNumber(execution.analysis.iterations)} iterations recorded
					</p>
				</Card>
			</div>
		{:else}
			<div class="space-y-3">
				<Card title="Variables">
					<KeyValueList
						items={execution.variables.map((item) => ({
							key: item.key,
							value: item.value,
						}))}
						dense
					/>
				</Card>
				<Card title="Call stack">
					<ol class="space-y-2">
						{#each execution.callStack as frame (frame.node)}
							<li
								class="flex items-start justify-between gap-2 border-b border-border/60 pb-2 last:border-0 last:pb-0"
							>
								<span class="min-w-0">
									<span class="block truncate font-mono text-caption"
										>{frame.node}</span
									>
									<span class="text-micro text-muted-foreground">
										depth {frame.depth} · {formatDateTime(frame.enteredAt)}
									</span>
								</span>
								<StatusBadge status={frame.status} size="sm" dot={false} />
							</li>
						{/each}
					</ol>
				</Card>
				<Card title="Memory">
					<div class="space-y-2">
						<div>
							<div class="flex items-center justify-between text-caption">
								<span class="text-muted-foreground">Current</span>
								<span class="tabular-nums"
									>{formatBytes(execution.memory.currentBytes)}</span
								>
							</div>
							<Progress
								value={execution.memory.currentBytes /
									Math.max(1, execution.memory.peakBytes)}
								tone="default"
								class="mt-1"
							/>
						</div>
						<p class="text-caption text-muted-foreground">
							Peak {formatBytes(execution.memory.peakBytes)} · snapshot {shortId(
								execution.id,
								10,
							)}
						</p>
					</div>
				</Card>
			</div>
		{/if}
	</div>
</div>
