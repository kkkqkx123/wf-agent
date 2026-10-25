<script lang="ts">
	import Badge from '#lib/ui/components/Badge.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import type { Checkpoint } from '#lib/types/models';
	import type { StateFileLink } from '#lib/services/checkpoints';
	import { formatBytes, formatRelativeTime } from '#lib/utils/format';
	import { t } from '#lib/i18n';

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
					{checkpoint.restorable
						? t('domain.loopCheckpoints.restorable')
						: t('domain.loopCheckpoints.locked')}
				</Badge>
				{#if links.get(checkpoint.id)?.state === 'pending'}
					<span title={t('domain.checkpoint.linkPendingTitle')}>
						<Badge variant="warning">{t('domain.checkpoint.linkPending')}</Badge>
					</span>
				{:else if links.get(checkpoint.id)?.state === 'linked'}
					<span title={t('domain.checkpoint.linkedTitle')}>
						<Badge variant="neutral">{t('domain.checkpoint.linked')}</Badge>
					</span>
				{/if}
			{/snippet}
			<div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-caption text-muted-foreground">
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
							{t('domain.checkpoint.editTags')}
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
						{t('domain.checkpoint.addTags')}
					</Button>
				</div>
			{/if}
			{#if editingTagsFor === checkpoint.id}
				<div class="mt-1 flex items-center gap-2">
					<Input
						size="sm"
						value={tagDraft}
						oninput={(event: Event) => (tagDraft = (event.target as HTMLInputElement).value)}
						placeholder={t('domain.checkpoint.tagsPlaceholder')}
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
						{t('common.save')}
					</Button>
					<Button variant="ghost" size="sm" onclick={() => (editingTagsFor = null)}>
						{t('common.cancel')}
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
							? t('domain.loopCheckpoints.restoreTitle')
							: t('domain.loopCheckpoints.lockedTitle')}
						onclick={() => onrestore(checkpoint)}
					>
						{t('domain.loopCheckpoints.restore')}
					</Button>
					<Button
						variant="ghost"
						size="sm"
						disabled={busy || checkpoint.entityType === 'agent_loop'}
						title={checkpoint.entityType === 'agent_loop'
							? t('domain.checkpoint.resumeWorkflowOnly')
							: t('domain.checkpoint.resumeTitle')}
						onclick={() => onresume(checkpoint)}
					>
						{t('domain.checkpoint.resume')}
					</Button>
					<Button variant="ghost" size="sm" disabled={busy} onclick={() => ondelete(checkpoint)}>
						{t('common.delete')}
					</Button>
					<Button
						variant="ghost"
						size="sm"
						disabled={busy}
						title={t('domain.checkpoint.purgeTitle')}
						onclick={() => onpurge(checkpoint)}
					>
						{t('domain.checkpoint.purge')}
					</Button>
				</div>
			{/snippet}
		</Card>
	{/each}
</div>
