<script lang="ts">
	import Icon from '#lib/ui/icons/Icon.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import {
		undoEdit,
		redoEdit,
		getActorWorkspace,
		getIndexStatus,
		rebuildIndex,
		renameFile,
	} from '#lib/services/checkpoints';
	import { toasts } from '#lib/stores/toast.svelte';
	import { checkpoints as store } from '#lib/stores/checkpoints.svelte';
	import { fileWorkspace as workspace } from '#lib/stores/fileWorkspace.svelte';
	import { t } from '#lib/i18n';

	let renameOpen = $state(false);
	let renameFrom = $state('');
	let renameTo = $state('');

	async function runRename(): Promise<void> {
		const actor = workspace.actor.trim();
		const from = renameFrom.trim();
		const to = renameTo.trim();
		if (!actor || !from || !to) {
			toasts.error(t('domain.files.renameInputRequired'));
			return;
		}
		workspace.beginAction('rename');
		try {
			const snapshot = await renameFile(actor, from, to);
			renameOpen = false;
			renameFrom = '';
			renameTo = '';
			toasts.success(t('domain.files.renamed'), `${from} → ${to} (${snapshot.slice(0, 10)})`);
			store.bump('files');
		} catch (e) {
			console.error('Rename failed:', e);
			toasts.error(t('domain.files.renameFailed'), e instanceof Error ? e.message : undefined);
		} finally {
			workspace.endAction();
		}
	}

	async function loadWorkspace(): Promise<void> {
		if (!workspace.actor.trim()) {
			toasts.error(t('domain.files.actorRequiredInspect'));
			return;
		}
		try {
			workspace.workspaceFiles = await getActorWorkspace(workspace.actor.trim());
		} catch (e) {
			console.error('Failed to load actor workspace:', e);
			toasts.error('Workspace unavailable', e instanceof Error ? e.message : undefined);
		}
	}

	async function runUndo(): Promise<void> {
		const actor = workspace.actor.trim();
		if (!actor) {
			toasts.error(t('domain.files.actorRequiredUndo'));
			return;
		}
		workspace.beginAction('undo');
		try {
			const snapshot = await undoEdit(actor);
			toasts.success(
				t('domain.files.undoRecorded'),
				t('domain.files.snapshot', { id: snapshot.slice(0, 10) }),
			);
			store.bump('files');
		} catch (e) {
			console.error('Undo failed:', e);
			toasts.error(t('domain.files.undoFailed'), e instanceof Error ? e.message : undefined);
		} finally {
			workspace.endAction();
		}
	}

	async function runRedo(): Promise<void> {
		const actor = workspace.actor.trim();
		if (!actor) {
			toasts.error(t('domain.files.actorRequiredRedo'));
			return;
		}
		workspace.beginAction('redo');
		try {
			const snapshot = await redoEdit(actor);
			toasts.success(
				t('domain.files.redoRecorded'),
				t('domain.files.snapshot', { id: snapshot.slice(0, 10) }),
			);
			store.bump('files');
		} catch (e) {
			console.error('Redo failed:', e);
			toasts.error(t('domain.files.redoFailed'), e instanceof Error ? e.message : undefined);
		} finally {
			workspace.endAction();
		}
	}

	async function runRebuildIndex(): Promise<void> {
		workspace.beginAction('rebuild');
		try {
			const { indexedCommits } = await rebuildIndex();
			workspace.indexStatus = await getIndexStatus().catch(() => null);
			toasts.success('Source index rebuilt', `${indexedCommits} commits indexed`);
		} catch (e) {
			console.error('Index rebuild failed:', e);
			toasts.error('Index rebuild failed', e instanceof Error ? e.message : undefined);
		} finally {
			workspace.endAction();
		}
	}
</script>

<Card title={t('domain.files.workspaceTitle')} description={t('domain.files.workspaceDescription')}>
	<div class="mb-2">
		<Input
			label={t('domain.files.actor')}
			size="sm"
			value={workspace.actor}
			oninput={(event: Event) => workspace.setActor((event.target as HTMLInputElement).value)}
			placeholder={t('domain.files.actorPartition')}
		/>
	</div>
	<div class="flex flex-wrap gap-2">
		<Button variant="outline" size="sm" disabled={workspace.busy} onclick={() => void runUndo()}>
			<Icon name="arrow-left" size={13} />
			{t('domain.graph.undo')}
		</Button>
		<Button variant="outline" size="sm" disabled={workspace.busy} onclick={() => void runRedo()}>
			{t('domain.graph.redo')}
		</Button>
		<Button
			variant="outline"
			size="sm"
			disabled={workspace.busy}
			onclick={() => void loadWorkspace()}
		>
			{t('domain.files.inspectWorkspace')}
		</Button>
		<Button
			variant="outline"
			size="sm"
			disabled={workspace.busy}
			onclick={() => void runRebuildIndex()}
		>
			<Icon name="database" size={13} />
			{t('domain.files.rebuildIndex')}
		</Button>
		<Button
			variant="outline"
			size="sm"
			disabled={workspace.busy}
			onclick={() => (renameOpen = true)}
		>
			{t('domain.files.renameFile')}
		</Button>
	</div>
	{#if workspace.indexStatus}
		<p class="mt-2 text-caption tabular-nums text-muted-foreground">
			{t('domain.files.indexStatus', {
				indexed: workspace.indexStatus.indexedCommits,
				total: workspace.indexStatus.totalCommits,
				state: workspace.indexStatus.needsRebuild
					? t('domain.files.rebuildRecommended')
					: t('domain.files.healthy'),
			})}
		</p>
	{/if}
	{#if workspace.workspaceFiles.length > 0}
		<ul class="mt-2 space-y-1">
			{#each workspace.workspaceFiles.slice(0, 20) as file (file.path)}
				<li class="flex items-center justify-between gap-2 text-caption">
					<span class="min-w-0 flex-1 truncate font-mono">{file.path}</span>
					<span class="shrink-0 font-mono text-micro text-muted-foreground"
						>{file.hash.slice(0, 10)}</span
					>
				</li>
			{/each}
		</ul>
		{#if workspace.workspaceFiles.length > 20}
			<p class="mt-1 text-micro text-muted-foreground">
				{t('domain.files.showingFiles', {
					total: workspace.workspaceFiles.length,
				})}
			</p>
		{/if}
	{/if}
</Card>

<Dialog
	bind:open={renameOpen}
	title={t('domain.files.renameFile')}
	description={t('domain.files.renameDescription')}
	onclose={() => (renameOpen = false)}
>
	<div class="space-y-2">
		<Input
			label={t('domain.files.fromPath')}
			size="sm"
			value={renameFrom}
			oninput={(event: Event) => (renameFrom = (event.target as HTMLInputElement).value)}
			placeholder="src/old.rs"
		/>
		<Input
			label={t('domain.files.toPath')}
			size="sm"
			value={renameTo}
			oninput={(event: Event) => (renameTo = (event.target as HTMLInputElement).value)}
			placeholder="src/new.rs"
		/>
	</div>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (renameOpen = false)}>
			{t('common.cancel')}
		</Button>
		<Button
			size="sm"
			disabled={workspace.busy || !renameFrom.trim() || !renameTo.trim()}
			onclick={() => void runRename()}
		>
			{t('domain.files.rename')}
		</Button>
	{/snippet}
</Dialog>
