<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import ExecutionInspector from '$lib/components/domain/ExecutionInspector.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import { getExecution, listToolCalls, listTimeline } from '$lib/services/executions';
	import type { ExecutionDetail, ToolCallEntry, TimelineEntry } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDuration } from '$lib/utils/format';

	let execution = $state<ExecutionDetail | null>(null);

	const safeExecution = $derived(execution ?? ({} as ExecutionDetail));
	// eslint-disable-next-line @typescript-eslint/no-unused-vars
	let toolCalls = $state<ToolCallEntry[]>([]);
	// eslint-disable-next-line @typescript-eslint/no-unused-vars
	let timeline = $state<TimelineEntry[]>([]);

	onMount(async () => {
		const id = page.params.id as string;
		const [detailRes, tcRes, tlRes] = await Promise.allSettled([
			getExecution(id),
			listToolCalls(id),
			listTimeline(id),
		]);
		if (detailRes.status === 'fulfilled') execution = detailRes.value;
		if (tcRes.status === 'fulfilled') toolCalls = tcRes.value;
		if (tlRes.status === 'fulfilled') timeline = tlRes.value;
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={execution?.workflowName ?? "Execution detail"}
		description="Full execution detail with state, timeline and analysis."
	>
		{#snippet meta()}
			<StatusBadge status={execution?.status ?? ""} />
			<Badge variant="outline">{formatDuration(execution?.durationMs)}</Badge>
			<span class="font-mono text-caption text-muted-foreground"
				>{execution?.id}</span
			>
			{#if execution?.trigger}
				<span class="text-caption text-muted-foreground"
					>{execution?.trigger}</span
				>
			{/if}
		{/snippet}
		{#snippet actions()}
			<IconButton
				icon="pause"
				label="Pause execution"
				onclick={() => toasts.warning('Pause queued')}
			/>
			<IconButton
				icon="play"
				label="Resume execution"
				onclick={() => toasts.info('Resume queued')}
			/>
			<Button
				variant="outline"
				size="sm"
				onclick={() => toasts.error('Cancel requires confirmation')}
			>
				<Icon name="square" size={13} />
				Cancel
			</Button>
			<Button variant="outline" size="sm" href="/executions">
				<Icon name="arrow-left" size={13} />
				Back to workbench
			</Button>
		{/snippet}
	</PageHeader>

	<div class="min-h-0 flex-1 overflow-hidden px-4 pb-4">
		<div class="h-full overflow-hidden rounded-lg border border-border bg-card">
			<ExecutionInspector execution={safeExecution} />
		</div>
	</div>
</div>
