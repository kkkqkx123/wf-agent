<script lang="ts">
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import type { Checkpoint } from '$lib/types/models';
	import type { StateFileLink } from '$lib/services/checkpoints';
	import { formatBytes, formatRelativeTime } from '$lib/utils/format';

	let {
		chain,
		links = new Map(),
		busy,
		onrestore,
		onresume,
		ondelete,
		onpurge,
		ontag,
	}: {
		chain: Checkpoint[];
		links?: Map<string, StateFileLink>;
		busy: boolean;
		onrestore: (checkpoint: Checkpoint) => void;
		onresume: (checkpoint: Checkpoint) => void;
		ondelete: (checkpoint: Checkpoint) => void;
		onpurge: (checkpoint: Checkpoint) => void;
		ontag?: (checkpoint: Checkpoint, tags: string[]) => void;
	} = $props();

	let editingTagsFor = $state<string | null>(null);
	let tagDraft = $state('');
</script>

<div class="space-y-2">
	{#each chain as checkpoint (checkpoint.id)}
		<Card title="{checkpoint.kind} checkpoint · #{checkpoint.sequence}">
			{#snippet actions()}
				<Badge variant={checkpoint.restorable ? 'success' : 'neutral'}>
					{checkpoint.restorable ? 'restorable' : 'locked'}
				</Badge>
				{#if links.get(checkpoint.id)?.state === 'pending'}
					<span title="The file snapshot link write failed and awaits startup compensation">
						<Badge variant="warning">file link pending</Badge>
					</span>
				{:else if links.get(checkpoint.id)?.state === 'linked'}
					<span title="Linked to a file snapshot">
						<Badge variant="neutral">file linked</Badge>
					</span>
				{/if}
			{/snippet}
			<div
				class="flex flex-wrap items-center gap-x-3 gap-y-1 text-caption text-muted-foreground"
			>
				<span class="font-mono">{checkpoint.id}</span>
				<span>{checkpoint.actor}</span>
				<span class="tabular-nums">{formatBytes(checkpoint.sizeBytes)}</span>
				<span>{formatRelativeTime(checkpoint.createdAt)}</span>
			</div>
			{#if checkpoint.tags && checkpoint.tags.length > 0}
				<div class="mt-1 flex flex-wrap items-center gap-1">
					{#each checkpoint.tags as tag (tag)}
						<Badge variant="neutral">{tag}</Badge>
					{/each}
					{#if ontag}
						<Button
							variant="ghost"
							size="sm"
							disabled={busy}
							onclick={() => {
								editingTagsFor = checkpoint.id;
								tagDraft = (checkpoint.tags ?? []).join(', ');
							}}
						>
							Edit tags
						</Button>
					{/if}
				</div>
			{:else if ontag}
				<div class="mt-1">
					<Button
						variant="ghost"
						size="sm"
						disabled={busy}
						onclick={() => {
							editingTagsFor = checkpoint.id;
							tagDraft = '';
						}}
					>
						Add tags
					</Button>
				</div>
			{/if}
			{#if editingTagsFor === checkpoint.id}
				<div class="mt-1 flex items-center gap-2">
					<Input
						size="sm"
						value={tagDraft}
						oninput={(event: Event) =>
							(tagDraft = (event.target as HTMLInputElement).value)}
						placeholder="comma-separated tags"
					/>
					<Button
						size="sm"
						disabled={busy}
						onclick={() => {
							const tags = tagDraft
								.split(',')
								.map((t) => t.trim())
								.filter((t) => t.length > 0);
							ontag?.(checkpoint, tags);
							editingTagsFor = null;
						}}
					>
						Save
					</Button>
					<Button
						variant="ghost"
						size="sm"
						onclick={() => (editingTagsFor = null)}
					>
						Cancel
					</Button>
				</div>
			{/if}
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
					<Button
						variant="ghost"
						size="sm"
						disabled={busy}
						title="Delete every checkpoint of this entity"
						onclick={() => onpurge(checkpoint)}
					>
						Purge entity
					</Button>
				</div>
			{/snippet}
		</Card>
	{/each}
</div>
