<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import ExecutionInspector from '$lib/components/domain/ExecutionInspector.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import { getExecutionDetail } from '$lib/services/executions';
	import type { ExecutionDetail } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDuration } from '$lib/utils/format';

	let execution = $state<ExecutionDetail | null>(null);

	onMount(() => {
		void getExecutionDetail(page.params.id)
			.then((row) => {
				execution = row;
			})
			.catch((e) => {
				console.error('Failed to load execution:', e);
			});
	});

	$effect(() => {
		const id = page.params.id;
		if (!id) return;
		if (execution && execution.id === id) return;
		void getExecutionDetail(id)
			.then((row) => {
				execution = row;
			})
			.catch((e) => {
				console.error('Failed to load execution:', e);
			});
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={execution?.workflowName ?? 'Execution detail'}
		description="Full execution detail with state, timeline and analysis."
	>
		{#snippet meta()}
			{#if execution}
				<StatusBadge status={execution.status} />
				<Badge variant="outline">{formatDuration(execution.durationMs)}</Badge>
				<span class="font-mono text-caption text-muted-foreground"
					>{execution.id}</span
				>
				{#if execution.trigger}
					<span class="text-caption text-muted-foreground"
						>{execution.trigger}</span
					>
				{/if}
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
			{#if execution}
				<ExecutionInspector execution={execution} />
			{/if}
		</div>
	</div>
</div>
