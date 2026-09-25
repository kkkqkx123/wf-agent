<script lang="ts">
	import type { Execution } from '$lib/types/models';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Progress from '$lib/components/ui/Progress.svelte';
	import StatusBadge from './StatusBadge.svelte';
	import { statusTone } from '$lib/utils/status';
	import { workflowTitle } from '$lib/stores/workflow-titles.svelte';
	import {
		formatDuration,
		formatRelativeTime,
		nodeCount,
		shortId,
	} from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';

	interface Props {
		execution: Execution;
		selected?: boolean;
		class?: string;
		onselect?: (execution: Execution) => void;
	}

	let {
		execution,
		selected = false,
		class: className = '',
		onselect,
	}: Props = $props();

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
</script>

<button
	type="button"
	onclick={() => onselect?.(execution)}
	class={cn(
		'w-full rounded-lg border px-3 py-2.5 text-left transition-colors duration-150',
		selected
			? 'border-ring bg-accent'
			: 'border-border bg-card hover:border-ring/40 hover:bg-accent/40',
		className,
	)}
>
	<div class="flex items-center justify-between gap-2">
		<span class="truncate text-body font-medium text-foreground"
			>{workflowTitle(execution.workflowId)}</span
		>
		<StatusBadge status={execution.status} size="sm" />
	</div>

	<div class="mt-1 flex items-center gap-2 text-micro text-muted-foreground">
		<span class="font-mono">{shortId(execution.id, 12)}</span>
		<span aria-hidden="true">·</span>
		<span>{formatRelativeTime(execution.startedAt)}</span>
		{#if execution.durationMs !== null}
			<span aria-hidden="true">·</span>
			<span class="tabular-nums">{formatDuration(execution.durationMs)}</span>
		{/if}
	</div>

	{#if execution.progress !== null}
		<div class="mt-2">
			<Progress value={execution.progress} tone={progressTone} />
		</div>
	{/if}

	<div
		class="mt-1.5 flex items-center justify-between gap-2 text-micro text-muted-foreground"
	>
		{#if nodes}
			<span class="tabular-nums">{nodes}</span>
		{/if}
		<span class="flex min-w-0 items-center gap-1.5">
			{#if (execution.nodesFailed ?? 0) > 0}
				<span class="flex items-center gap-1 text-destructive">
					<Icon name="alert-triangle" size={11} />
					{execution.nodesFailed} failed
				</span>
			{/if}
			{#if execution.currentNodeId}
				<span class="truncate font-mono">{execution.currentNodeId}</span>
			{/if}
		</span>
	</div>
</button>
