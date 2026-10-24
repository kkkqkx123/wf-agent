<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import ChainPanel from '#lib/components/domain/checkpoints/ChainPanel.svelte';
	import AdvancedCheckpointCard from '#lib/components/domain/checkpoints/AdvancedCheckpointCard.svelte';
	import {
		createExecutionCheckpoint,
		deleteCheckpoint,
		deleteCheckpointsByEntity,
		gcExecutionCheckpoints,
		getStateFileLinks,
		previewRestoreCheckpoint,
		restoreCheckpoint,
		resumeFromCheckpoint,
		setCheckpointTags,
		type StateFileLink,
	} from '#lib/services/checkpoints';
	import type { Checkpoint } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { checkpoints as checkpointStore } from '#lib/stores/checkpoints.svelte';
	import { t } from '#lib/i18n';

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
			toasts.error(t('domain.checkpoint.executionIdRequired'));
			return;
		}
		checkpointStore.beginAction('create');
		try {
			const checkpointId = await createExecutionCheckpoint(id);
			toasts.success(t('domain.checkpoint.created'), checkpointId.slice(0, 10));
			checkpointStore.bump('chain');
		} catch (e) {
			console.error('Execution checkpoint creation failed:', e);
			toasts.error(
				t('domain.checkpoint.createFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runGcExecution(): Promise<void> {
		const id = execId.trim();
		if (!id) {
			toasts.error(t('domain.checkpoint.executionIdRequired'));
			return;
		}
		checkpointStore.beginAction('gc');
		try {
			const removed = await gcExecutionCheckpoints(id);
			gcOpen = false;
			toasts.success(
				t('domain.checkpoint.collected'),
				t('domain.checkpoint.removedCount', { count: removed }),
			);
			checkpointStore.bump('chain');
		} catch (e) {
			console.error('Execution checkpoint GC failed:', e);
			toasts.error(
				t('domain.checkpoint.gcFailed'),
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
				t('domain.checkpoint.purged'),
				t('domain.checkpoint.removedCount', { count: removed }),
			);
			checkpointStore.bump('chain');
		} catch (e) {
			console.error('Entity checkpoint delete failed:', e);
			toasts.error(
				t('domain.checkpoint.deleteFailed'),
				e instanceof Error ? e.message : undefined,
			);
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
	let restorePreview = $state<
		import('#lib/services/checkpoints').RestorePreview | null
	>(null);
	let restorePreviewLoading = $state(false);
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
				t('domain.loopCheckpoints.restored'),
				t('domain.checkpoint.restoreNote'),
			);
			checkpointStore.bump('chain');
		} catch (e) {
			console.error('Restore failed:', e);
			toasts.error(
				t('domain.checkpoint.restoreFailed'),
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
				t('domain.checkpoint.resumeRequested'),
				t('domain.checkpoint.resumeDetail', { id: target.id.slice(0, 10) }),
			);
		} catch (e) {
			console.error('Resume failed:', e);
			toasts.error(
				t('domain.checkpoint.resumeFailed'),
				e instanceof Error ? e.message : undefined,
			);
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
			toasts.success(t('domain.checkpoint.deleted'));
			checkpointStore.bump('chain');
		} catch (e) {
			console.error('Delete failed:', e);
			toasts.error(
				t('domain.checkpoint.deleteFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runSetTags(
		checkpoint: Checkpoint,
		tags: string[],
	): Promise<void> {
		checkpointStore.beginAction('tag');
		try {
			await setCheckpointTags(checkpoint.id, tags);
			toasts.success(t('domain.checkpoint.tagsUpdated'));
			checkpointStore.bump('chain');
		} catch (e) {
			console.error('Tag update failed:', e);
			toasts.error(
				t('domain.checkpoint.tagsUpdateFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}
</script>

<Card
	title={t('domain.checkpoint.chainTitle')}
	description={t('domain.checkpoint.chainDescription')}
>
	<div class="flex flex-wrap items-end gap-2">
		<div class="min-w-64 flex-1">
			<Input
				label={t('domain.checkpoint.executionId')}
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
			{t('domain.checkpoint.createCheckpoint')}
		</Button>
		<Button
			variant="outline"
			size="sm"
			disabled={checkpointStore.busy || !execId.trim()}
			onclick={() => (gcOpen = true)}
		>
			{t('domain.checkpoint.gc')}
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
		restorePreview = null;
		restorePreviewLoading = true;
		void (async () => {
			try {
				restorePreview = await previewRestoreCheckpoint(checkpoint.id);
			} catch (e) {
				console.error('Restore preview failed:', e);
			} finally {
				restorePreviewLoading = false;
			}
		})();
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
	ontag={(checkpoint: Checkpoint, tags: string[]) =>
		void runSetTags(checkpoint, tags)}
/>

<Dialog
	bind:open={gcOpen}
	title={t('domain.checkpoint.gcTitle')}
	description={t('domain.checkpoint.gcDescription')}
	onclose={() => (gcOpen = false)}
>
	<p class="text-caption">
		{t('domain.checkpoint.gcConfirm', { id: execId.trim() })}
	</p>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (gcOpen = false)}>
			{t('common.cancel')}
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void runGcExecution()}
		>
			{t('domain.checkpoint.collect')}
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={purgeOpen}
	title={t('domain.checkpoint.purgeTitleDialog')}
	description={t('domain.checkpoint.purgeDescription')}
	onclose={() => (purgeTarget = null)}
>
	{#if purgeTarget}
		<p class="text-caption">
			{t('domain.checkpoint.purgeConfirm', {
				id: purgeTarget.executionId,
				type: purgeTarget.entityType,
			})}
		</p>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (purgeOpen = false)}>
			{t('common.cancel')}
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void runPurge()}
		>
			{t('domain.checkpoint.deleteAll')}
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={restoreOpen}
	title={t('domain.checkpoint.restoreDialogTitle')}
	description={t('domain.checkpoint.restoreDialogDescription')}
	onclose={() => {
		restoreTarget = null;
		restorePreview = null;
	}}
>
	{#if restoreTarget}
		<p class="text-caption">
			{t('domain.checkpoint.restoreConfirm', {
				id: restoreTarget.id,
				kind: restoreTarget.kind,
				actor: restoreTarget.actor,
			})}
		</p>
		{#if restorePreviewLoading}
			<p class="text-caption text-muted-foreground">{t('common.loading')}</p>
		{:else if restorePreview}
			<div class="mt-2 rounded border border-border p-2 text-caption">
				<p class="font-medium">{t('domain.checkpoint.restorePreview')}</p>
				<p>
					{t('domain.checkpoint.executionLabel')}:
					<span class="font-mono">{restorePreview.executionId}</span>
				</p>
				<p>{t('domain.graph.status')}: {restorePreview.status}</p>
				{#if restorePreview.currentNodeId}
					<p>
						{t('domain.inspector.currentNode')}:
						<span class="font-mono">{restorePreview.currentNodeId}</span>
					</p>
				{/if}
				{#if restorePreview.variableKeys.length > 0}
					<p>
						{t('domain.history.variables')} ({restorePreview.variableKeys
							.length}): {restorePreview.variableKeys.join(', ')}
					</p>
				{/if}
				{#if restorePreview.nodeResultKeys.length > 0}
					<p>
						{t('domain.checkpoint.nodeResults')} ({restorePreview.nodeResultKeys
							.length}): {restorePreview.nodeResultKeys.join(', ')}
					</p>
				{/if}
			</div>
		{/if}
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (restoreOpen = false)}>
			{t('common.cancel')}
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void runRestore()}
		>
			{t('domain.checkpoint.restoreAsCommit')}
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={deleteOpen}
	title={t('domain.checkpoint.deleteDialogTitle')}
	description={t('domain.checkpoint.deleteDialogDescription')}
	onclose={() => (deleteTarget = null)}
>
	{#if deleteTarget}
		<p class="text-caption">
			{t('domain.checkpoint.deleteConfirm', { id: deleteTarget.id })}
		</p>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (deleteOpen = false)}>
			{t('common.cancel')}
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void runDelete()}
		>
			{t('common.delete')}
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={resumeOpen}
	title={t('domain.checkpoint.resumeDialogTitle')}
	description={t('domain.checkpoint.resumeDialogDescription')}
	onclose={() => (resumeTarget = null)}
>
	{#if resumeTarget}
		<p class="text-caption">
			{t('domain.checkpoint.resumeConfirm', {
				id: resumeTarget.id,
				kind: resumeTarget.kind,
				actor: resumeTarget.actor,
			})}
		</p>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (resumeOpen = false)}>
			{t('common.cancel')}
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void runResume()}
		>
			{t('domain.checkpoint.resume')}
		</Button>
	{/snippet}
</Dialog>
