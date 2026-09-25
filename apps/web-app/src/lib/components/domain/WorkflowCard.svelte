<script lang="ts">
	import type { Workflow } from '$lib/types/models';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import StatusBadge from './StatusBadge.svelte';
	import { formatPercent, formatRelativeTime } from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';

	interface Props {
		workflow: Workflow;
		selected?: boolean;
		class?: string;
		onselect?: (workflow: Workflow) => void;
	}

	let {
		workflow,
		selected = false,
		class: className = '',
		onselect,
	}: Props = $props();
</script>

<button
	type="button"
	onclick={() => onselect?.(workflow)}
	class={cn(
		'flex h-full flex-col rounded-lg border px-3 py-2.5 text-left transition-colors duration-150',
		selected
			? 'border-ring bg-accent'
			: 'border-border bg-card hover:border-ring/40 hover:bg-accent/40',
		className,
	)}
>
	<div class="flex items-start justify-between gap-2">
		<h3 class="truncate text-body font-medium text-foreground">
			{workflow.name}
		</h3>
		<StatusBadge status={workflow.status} size="sm" />
	</div>

	<p class="mt-1 line-clamp-2 text-caption text-muted-foreground">
		{workflow.description}
	</p>

	<div class="mt-2 flex flex-wrap items-center gap-1">
		{#each workflow.tags.slice(0, 3) as tag (tag)}
			<Badge variant="outline" size="sm">{tag}</Badge>
		{/each}
		{#if workflow.tags.length > 3}
			<span class="text-micro text-muted-foreground"
				>+{workflow.tags.length - 3}</span
			>
		{/if}
	</div>

	<div
		class="mt-2.5 flex items-center justify-between gap-2 border-t border-border pt-2 text-micro text-muted-foreground"
	>
		<span class="flex items-center gap-1">
			<Icon name="workflow" size={11} />
			v{workflow.version} · {workflow.nodeCount} nodes
		</span>
		<span class="flex items-center gap-2 tabular-nums">
			{#if workflow.successRate !== null}
				<span>{formatPercent(workflow.successRate, 0)} ok</span>
			{/if}
			<span>{formatRelativeTime(workflow.updatedAt)}</span>
		</span>
	</div>
</button>
