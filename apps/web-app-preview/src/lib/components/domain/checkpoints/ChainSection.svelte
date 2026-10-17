<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Dialog from '@wf-agent/ui/components/Dialog.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import ChainPanel from '$lib/components/domain/checkpoints/ChainPanel.svelte';
	import AdvancedCheckpointCard from '$lib/components/domain/checkpoints/AdvancedCheckpointCard.svelte';
	import {
		createExecutionCheckpoint,
		deleteCheckpoint,
		deleteCheckpointsByEntity,
		gcExecutionCheckpoints,
		getStateFileLinks,
		restoreCheckpoint,
		resumeFromCheckpoint,
		type StateFileLink,
	} from '$lib/services/checkpoints';
	import type { Checkpoint } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { checkpoints as checkpointStore } from '$lib/stores/checkpoints.svelte';

	// No reload prop: the refetch is driven by `bump('chain')` notifying the
	// route's live subscription, which reloads this tab.
	let { chain }: { chain: Checkpoint[] } = $props();

	// Execution id used by the create/GC controls above the chain list.
	let execId = $state('');
	let gcOpen = $state(false);
	let purgeOpen = $state(false);
	let purgeTarget = $state<Checkpoint | null>(null);

	async function runCreateExecution(): Promise<void> {
		const id = execId.trim();
		if (!id) {
			toasts.error('Enter an execution id');
			return;
		}
		checkpointStore.beginAction('create');
		try {
			const checkpointId = await createExecutionCheckpoint(id);
			toasts.success(
				'Execution checkpoint created',
				checkpointId.slice(0, 10),
			);
			checkpointStore.bump('chain');
		} catch (e) {
			console.error('Execution checkpoint creation failed:', e);
			toasts.error(
				'Checkpoint creation failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runGcExecution(): Promise<void> {
		const id = execId.trim();
		if (!id) {
			toasts.error('Enter an execution id');
			return;
		}
		checkpointStore.beginAction('gc');
		try {
			const removed = await gcExecutionCheckpoints(id);
			gcOpen = false;
			toasts.success(
				'Execution checkpoints collected',
				`${removed} checkpoint(s) removed`,
			);
			checkpointStore.bump('chain');
		} catch (e) {
			console.error('Execution checkpoint GC failed:', e);
			toasts.error(
				'Checkpoint GC failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runPurge(): Promise<void> {
		const target = purgeTarget;
		if (!target) return;
		checkpointStore.beginAction('delete');
		try {
			const removed = await deleteCheckpointsByEntity(
				target.executionId,
				target.entityType ?? 'checkpoint',
			);
			purgeOpen = false;
			purgeTarget = null;
			toasts.success(
				'Entity checkpoints deleted',
				`${removed} checkpoint(s) removed`,
			);
			checkpointStore.bump('chain');
		} catch (e) {
			console.error('Entity checkpoint delete failed:', e);
			toasts.error('Delete failed', e instanceof Error ? e.message : undefined);
		} finally {
			checkpointStore.endAction();
		}
	}

	// Health of the state-to-file association, keyed by checkpoint id. The
	// whole page is queried in one call; cards without a link simply show no
	// badge.
	let links = $state<Map<string, StateFileLink>>(new Map());
	$effect(() => {
		const ids = chain.map((checkpoint) => checkpoint.id);
		void loadLinks(ids);
	});
	async function loadLinks(ids: string[]): Promise<void> {
		try {
			links = await getStateFileLinks(ids);
		} catch (e) {
			console.error('Failed to load state-file links:', e);
		}
	}

	// ── dialogs ──
	let restoreOpen = $state(false);
	let restoreTarget = $state<Checkpoint | null>(null);
	let deleteOpen = $state(false);
	let deleteTarget = $state<Checkpoint | null>(null);
	let resumeOpen = $state(false);
	let resumeTarget = $state<Checkpoint | null>(null);

	async function runRestore(): Promise<void> {
		const target = restoreTarget;
		if (!target) return;
		checkpointStore.beginAction('restore');
		try {
			await restoreCheckpoint(target);
			restoreOpen = false;
			restoreTarget = null;
			toasts.success(
				'Checkpoint restored',
				'Restore landed as a new commit; history was not rewritten.',
			);
			checkpointStore.bump('chain');
		} catch (e) {
			console.error('Restore failed:', e);
			toasts.error(
				'Restore failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runResume(): Promise<void> {
		const target = resumeTarget;
		if (!target) return;
		checkpointStore.beginAction('resume');
		try {
			await resumeFromCheckpoint(target.id);
			resumeOpen = false;
			resumeTarget = null;
			toasts.success(
				'Resume requested',
				`Execution continues from ${target.id.slice(0, 10)}`,
			);
		} catch (e) {
			console.error('Resume failed:', e);
			toasts.error('Resume failed', e instanceof Error ? e.message : undefined);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runDelete(): Promise<void> {
		const target = deleteTarget;
		if (!target) return;
		checkpointStore.beginAction('delete');
		try {
			await deleteCheckpoint(target.id);
			deleteOpen = false;
			deleteTarget = null;
			toasts.success('Checkpoint deleted');
			checkpointStore.bump('chain');
		} catch (e) {
			console.error('Delete failed:', e);
			toasts.error('Delete failed', e instanceof Error ? e.message : undefined);
		} finally {
			checkpointStore.endAction();
		}
	}
</script>

<Card
	title="Execution checkpoints"
	description="Create or garbage-collect workflow execution checkpoints by execution id"
>
	<div class="flex flex-wrap items-end gap-2">
		<div class="min-w-64 flex-1">
			<Input
				label="Execution id"
				size="sm"
				value={execId}
				oninput={(event: Event) =>
					(execId = (event.target as HTMLInputElement).value)}
				placeholder="workflow execution id"
			/>
		</div>
		<Button
			variant="outline"
			size="sm"
			disabled={checkpointStore.busy || !execId.trim()}
			onclick={() => void runCreateExecution()}
		>
			Create checkpoint
		</Button>
		<Button
			variant="outline"
			size="sm"
			disabled={checkpointStore.busy || !execId.trim()}
			onclick={() => (gcOpen = true)}
		>
			GC execution
		</Button>
	</div>
</Card>

<AdvancedCheckpointCard />

<ChainPanel
	{chain}
	{links}
	busy={checkpointStore.busy}
	onrestore={(checkpoint: Checkpoint) => {
		restoreTarget = checkpoint;
		restoreOpen = true;
	}}
	onresume={(checkpoint: Checkpoint) => {
		resumeTarget = checkpoint;
		resumeOpen = true;
	}}
	ondelete={(checkpoint: Checkpoint) => {
		deleteTarget = checkpoint;
		deleteOpen = true;
	}}
	onpurge={(checkpoint: Checkpoint) => {
		purgeTarget = checkpoint;
		purgeOpen = true;
	}}
/>

<Dialog
	bind:open={gcOpen}
	title="Garbage-collect execution checkpoints"
	description="Every checkpoint of the execution is removed. Files in the object store are reclaimed by the file-checkpoint GC."
	onclose={() => (gcOpen = false)}
>
	<p class="text-caption">
		Remove all checkpoints of <span class="font-mono">{execId.trim()}</span>?
		This cannot be undone.
	</p>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (gcOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void runGcExecution()}
		>
			Collect
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={purgeOpen}
	title="Delete entity checkpoints"
	description="Every checkpoint of the selected row's entity is removed, scoped by entity type."
	onclose={() => (purgeTarget = null)}
>
	{#if purgeTarget}
		<p class="text-caption">
			Delete all checkpoints of
			<span class="font-mono">{purgeTarget.executionId}</span>
			({purgeTarget.entityType})? This cannot be undone.
		</p>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (purgeOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void runPurge()}
		>
			Delete all
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={restoreOpen}
	title="Restore checkpoint"
	description="Restore lands as a new commit on top of history. Pointers are never moved and other branches are untouched."
	onclose={() => (restoreTarget = null)}
>
	{#if restoreTarget}
		<p class="text-caption">
			Restore <span class="font-mono">{restoreTarget.id}</span>
			({restoreTarget.kind}
			· {restoreTarget.actor}). Files covered by this checkpoint return to the
			recorded state; everything else stays as-is.
		</p>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (restoreOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void runRestore()}
		>
			Restore as new commit
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={deleteOpen}
	title="Delete checkpoint"
	description="Delete removes the checkpoint record. File history in the object store is reclaimed by garbage collection."
	onclose={() => (deleteTarget = null)}
>
	{#if deleteTarget}
		<p class="text-caption">
			Delete <span class="font-mono">{deleteTarget.id}</span>? This cannot be
			undone from this page.
		</p>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (deleteOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void runDelete()}
		>
			Delete
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={resumeOpen}
	title="Resume execution"
	description="The execution continues from this checkpoint. Completed steps are not replayed."
	onclose={() => (resumeTarget = null)}
>
	{#if resumeTarget}
		<p class="text-caption">
			Resume <span class="font-mono">{resumeTarget.id}</span>
			({resumeTarget.kind} · {resumeTarget.actor}).
		</p>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (resumeOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void runResume()}
		>
			Resume from here
		</Button>
	{/snippet}
</Dialog>
