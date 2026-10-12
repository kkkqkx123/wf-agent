<script lang="ts">
	import {
		createAgentLoopCheckpoint,
		restoreAgentLoopCheckpoint,
	} from '$lib/services/agent-loops';
	import { listLoopCheckpoints } from '$lib/services/checkpoints';
	import type { Checkpoint } from '$lib/types/models';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDateTime } from '$lib/utils/format';

	/** Loop checkpoint list with restore; owns its own loading keyed by id. */
	let { id }: { id: string } = $props();

	let checkpoints = $state<Checkpoint[]>([]);
	let error = $state<string | null>(null);
	let seen = $state('');
	let busy = $state(false);

	$effect(() => {
		const loopId = id;
		if (seen === loopId) return;
		seen = loopId;
		void load(loopId);
	});

	async function load(loopId: string): Promise<void> {
		error = null;
		try {
			checkpoints = await listLoopCheckpoints(loopId);
		} catch (e) {
			seen = '';
			checkpoints = [];
			error = e instanceof Error ? e.message : 'Checkpoints failed.';
		}
	}

	async function createCheckpoint(): Promise<void> {
		busy = true;
		try {
			await createAgentLoopCheckpoint(id);
			toasts.success('Checkpoint created');
			await load(id);
		} catch (e) {
			toasts.error(
				'Checkpoint failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			busy = false;
		}
	}

	async function restore(checkpointId: string): Promise<void> {
		busy = true;
		try {
			await restoreAgentLoopCheckpoint(id, checkpointId);
			toasts.success('Checkpoint restored');
			await load(id);
		} catch (e) {
			toasts.error(
				'Restore failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			busy = false;
		}
	}
</script>

{#if error}
	<ErrorState
		title="Checkpoints failed to load"
		description={error}
		onretry={() => void load(id)}
		class="rounded-lg border border-border bg-card"
	/>
{:else}
	<div class="mb-2 flex justify-end">
		<Button
			variant="outline"
			size="sm"
			disabled={busy}
			onclick={() => void createCheckpoint()}
		>
			New checkpoint
		</Button>
	</div>
	<div class="space-y-2">
		{#each checkpoints as checkpoint (checkpoint.id)}
			<Card title="{checkpoint.kind} · #{checkpoint.sequence}">
				{#snippet actions()}
					<Badge variant={checkpoint.restorable ? 'success' : 'neutral'}>
						{checkpoint.restorable ? 'restorable' : 'locked'}
					</Badge>
				{/snippet}
				<p class="text-caption text-muted-foreground">
					{checkpoint.note}
				</p>
				<p class="mt-1 text-micro text-muted-foreground">
					{checkpoint.actor} · {formatDateTime(checkpoint.createdAt)}
				</p>
				{#snippet footer()}
					<Button
						variant="ghost"
						size="sm"
						disabled={!checkpoint.restorable || busy}
						title={checkpoint.restorable
							? 'Restore this checkpoint'
							: 'This checkpoint is locked and cannot be restored'}
						onclick={() => void restore(checkpoint.id)}
					>
						Restore
					</Button>
				{/snippet}
			</Card>
		{:else}
			<Card>
				<p class="text-caption text-muted-foreground">
					No checkpoints recorded for this loop.
				</p>
			</Card>
		{/each}
	</div>
{/if}
