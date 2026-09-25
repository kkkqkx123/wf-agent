<script lang="ts">
	import type { ExecutionDetail } from '$lib/types/models';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import StatusBadge from './StatusBadge.svelte';
	import KeyValueList from './KeyValueList.svelte';
	import Progress from '$lib/components/ui/Progress.svelte';
	import Timeline from './Timeline.svelte';
	import ToolCallCard from './ToolCallCard.svelte';
	import { streamErrorAnalysis } from '$lib/services/streaming';
	import type { ErrorRecord } from '$lib/services/streaming';

	import { formatDateTime, formatDuration, nodeCount } from '$lib/utils/format';
	import { statusTone } from '$lib/utils/status';
	import { workflowTitle } from '$lib/stores/workflow-titles.svelte';
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
	const nodes = $derived(nodeCount(execution.nodesDone, execution.nodesTotal));
	const failedNodes = $derived(execution.nodesFailed ?? 0);

	/** Error chain of this execution, filled frame by frame on demand. */
	let errorRecords = $state<ErrorRecord[]>([]);
	let analyzing = $state(false);
	let analysisError = $state<string | null>(null);
	let analysisRetryMs = $state<number | null>(null);
	let stopAnalysis: (() => void) | null = null;

	async function analyzeErrors(): Promise<void> {
		errorRecords = [];
		analysisError = null;
		analysisRetryMs = null;
		const controller = new AbortController();
		stopAnalysis = () => controller.abort();
		analyzing = true;
		await streamErrorAnalysis(
			execution.id,
			{
				onRecord: (record) => (errorRecords = [...errorRecords, record]),
				onError: (failure) => {
					analysisError = failure.message;
					analysisRetryMs = failure.retryAfterMs;
				},
			},
			controller.signal,
		);
		stopAnalysis = null;
		analyzing = false;
	}

	// Leaving the page abandons the analysis stream rather than orphaning it.
	$effect(() => () => stopAnalysis?.());
</script>

<div class={cn('flex h-full min-h-0 flex-col', className)}>
	<div class="border-b border-border px-3 py-3">
		<div class="flex items-start justify-between gap-2">
			<div class="min-w-0">
				<h2 class="truncate text-title font-semibold">
					{workflowTitle(execution.workflowId)}
				</h2>
				<p class="mt-0.5 font-mono text-micro text-muted-foreground">
					{execution.id}
				</p>
			</div>
			<StatusBadge status={execution.status} />
		</div>

		{#if execution.progress !== null}
			<div class="mt-3 space-y-1.5">
				<div
					class="flex items-center justify-between text-micro text-muted-foreground"
				>
					<span>Progress</span>
					<span class="tabular-nums">{nodes}</span>
				</div>
				<Progress value={execution.progress} tone={progressTone} />
			</div>
		{/if}

		<dl class="mt-3 grid grid-cols-2 gap-x-3 gap-y-2">
			<div>
				<dt class="text-micro text-muted-foreground">Started</dt>
				<dd class="text-caption tabular-nums">
					{formatDateTime(execution.startedAt)}
				</dd>
			</div>
			{#if execution.durationMs !== null}
				<div>
					<dt class="text-micro text-muted-foreground">Duration</dt>
					<dd class="text-caption tabular-nums">
						{formatDuration(execution.durationMs)}
					</dd>
				</div>
			{/if}
			{#if execution.progress === null && nodes}
				<div>
					<dt class="text-micro text-muted-foreground">Nodes</dt>
					<dd class="text-caption tabular-nums">{nodes}</dd>
				</div>
			{/if}
			{#if execution.executionType}
				<div>
					<dt class="text-micro text-muted-foreground">Type</dt>
					<dd class="truncate text-caption">{execution.executionType}</dd>
				</div>
			{/if}
		</dl>
	</div>

	<Segmented items={TABS} bind:value={tab} size="sm" class="px-2" />

	<div class="min-h-0 flex-1 overflow-y-auto px-3 py-3">
		{#if tab === 'overview'}
			<div class="space-y-3">
				{#if execution.errorMessage}
					<Card title="Failure">
						<p class="text-caption text-destructive">
							{execution.errorMessage}
						</p>
					</Card>
				{/if}
				<Card title="Current position">
					<p class="font-mono text-body">
						{execution.currentNodeId ?? 'No active node'}
					</p>
					{#if nodes}
						<p class="mt-1 text-caption text-muted-foreground">
							{nodes} recorded
							{#if failedNodes > 0}· {failedNodes} failed{/if}
						</p>
					{/if}
				</Card>
				<Card title="Input">
					{#if execution.input}
						<pre
							class="max-h-48 overflow-auto rounded-md bg-muted px-2 py-1.5 font-mono text-micro whitespace-pre-wrap break-all">{execution.input}</pre>
					{:else}
						<p class="text-caption text-muted-foreground">No input recorded.</p>
					{/if}
				</Card>
				<Card title="Output">
					{#if execution.output}
						<pre
							class="max-h-48 overflow-auto rounded-md bg-muted px-2 py-1.5 font-mono text-micro whitespace-pre-wrap break-all">{execution.output}</pre>
					{:else}
						<p class="text-caption text-muted-foreground">
							No output recorded.
						</p>
					{/if}
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
				{#if execution.failures.length > 0}
					<Card title="Node failures">
						<ul class="space-y-1.5">
							{#each execution.failures as message, index (index)}
								<li class="text-caption text-destructive">{message}</li>
							{/each}
						</ul>
					</Card>
				{/if}
				<Card title="Error chain">
					{#snippet actions()}
						{#if analyzing}
							<Button
								variant="ghost"
								size="sm"
								onclick={() => stopAnalysis?.()}
							>
								<Icon name="square" size={13} />
								Stop
							</Button>
						{:else}
							<Button
								variant="ghost"
								size="sm"
								onclick={() => void analyzeErrors()}
							>
								<Icon name="search" size={13} />
								Analyze
							</Button>
						{/if}
					{/snippet}
					{#if analysisError}
						<p
							class="rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5 text-caption text-destructive"
						>
							{analysisError}
							{#if analysisRetryMs !== null}
								<span class="mt-1 block text-muted-foreground">
									Rate limited — retry in {formatDuration(analysisRetryMs)}.
								</span>
							{/if}
						</p>
					{:else if errorRecords.length > 0}
						<ol class="space-y-2">
							{#each errorRecords as record (record.id)}
								<li
									class="border-b border-border/60 pb-2 last:border-0 last:pb-0"
								>
									<div class="flex items-center justify-between gap-2">
										<span class="truncate font-mono text-caption">
											{record.nodeId ?? record.id}
										</span>
										<span
											class="flex shrink-0 items-center gap-1.5 text-micro text-muted-foreground"
										>
											{#if record.id === record.rootCauseId}
												<Badge variant="danger" size="sm">root</Badge>
											{/if}
											{record.errorType ?? ''}
										</span>
									</div>
									<p class="mt-1 text-caption">{record.error}</p>
									<p class="mt-0.5 text-micro text-muted-foreground">
										{formatDateTime(record.at)}
										{#if record.recoveryAction}
											· {record.recoveryAction}
										{:else if !record.isRecoverable}
											· not recoverable
										{/if}
									</p>
								</li>
							{/each}
						</ol>
						{#if analyzing}
							<p class="mt-2 text-caption text-muted-foreground">
								Streaming more records…
							</p>
						{/if}
					{:else if analyzing}
						<p class="text-caption text-muted-foreground">
							Waiting for the root cause…
						</p>
					{:else}
						<p class="text-caption text-muted-foreground">
							Run the analysis to read the error chain from its root cause.
						</p>
					{/if}
				</Card>
			</div>
		{:else}
			<div class="space-y-3">
				<Card title="Variables">
					{#if execution.variables.length > 0}
						<KeyValueList items={execution.variables} dense />
					{:else}
						<p class="text-caption text-muted-foreground">
							No variables recorded.
						</p>
					{/if}
				</Card>
			</div>
		{/if}
	</div>
</div>
