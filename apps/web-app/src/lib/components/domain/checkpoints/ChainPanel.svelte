<script lang="ts">
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import type { Checkpoint } from '$lib/types/models';
	import { formatBytes, formatRelativeTime } from '$lib/utils/format';

	let {
		chain,
		busy,
		onrestore,
		onresume,
		ondelete,
	}: {
		chain: Checkpoint[];
		busy: boolean;
		onrestore: (checkpoint: Checkpoint) => void;
		onresume: (checkpoint: Checkpoint) => void;
		ondelete: (checkpoint: Checkpoint) => void;
	} = $props();
</script>

<div class="space-y-2">
	{#each chain as checkpoint (checkpoint.id)}
		<Card title="{checkpoint.kind} checkpoint · #{checkpoint.sequence}">
			{#snippet actions()}
				<Badge variant={checkpoint.restorable ? 'success' : 'neutral'}>
					{checkpoint.restorable ? 'restorable' : 'locked'}
				</Badge>
			{/snippet}
			<div
				class="flex flex-wrap items-center gap-x-3 gap-y-1 text-caption text-muted-foreground"
			>
				<span class="font-mono">{checkpoint.id}</span>
				<span>{checkpoint.actor}</span>
				<span class="tabular-nums">{formatBytes(checkpoint.sizeBytes)}</span>
				<span>{formatRelativeTime(checkpoint.createdAt)}</span>
			</div>
			<p class="mt-1.5 text-body">{checkpoint.note}</p>
			{#snippet footer()}
				<div class="flex flex-wrap items-center gap-2">
					<Button
						size="sm"
						disabled={!checkpoint.restorable || busy}
						title={checkpoint.restorable
							? 'Restore this checkpoint'
							: 'This checkpoint is locked and cannot be restored'}
						onclick={() => onrestore(checkpoint)}
					>
						Restore
					</Button>
					<Button
						variant="ghost"
						size="sm"
						disabled={busy || checkpoint.entityType === 'agent_loop'}
						title={checkpoint.entityType === 'agent_loop'
							? 'Resume is only available for workflow checkpoints'
							: 'Resume execution from here'}
						onclick={() => onresume(checkpoint)}
					>
						Resume from here
					</Button>
					<Button
						variant="ghost"
						size="sm"
						disabled={busy}
						onclick={() => ondelete(checkpoint)}
					>
						Delete
					</Button>
				</div>
			{/snippet}
		</Card>
	{/each}
</div>
