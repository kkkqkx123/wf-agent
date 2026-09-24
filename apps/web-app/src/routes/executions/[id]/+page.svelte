<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import ExecutionInspector from '$lib/components/domain/ExecutionInspector.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import type { PageData } from './$types';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDuration } from '$lib/utils/format';

	let { data }: { data: PageData } = $props();
	let { detail } = $derived(data);
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={detail.workflowName}
		description="Full execution detail with state, timeline and analysis."
	>
		{#snippet meta()}
			<StatusBadge status={detail.status} />
			<Badge variant="outline">{formatDuration(detail.durationMs)}</Badge>
			<span class="font-mono text-caption text-muted-foreground"
				>{detail.id}</span
			>
			{#if detail.trigger}
				<span class="text-caption text-muted-foreground"
					>{detail.trigger}</span
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
			<ExecutionInspector execution={detail} />
		</div>
	</div>
</div>
