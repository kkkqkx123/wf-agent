<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import {
		undoEdit,
		redoEdit,
		getActorWorkspace,
		getIndexStatus,
		rebuildIndex,
	} from '$lib/services/checkpoints';
	import { toasts } from '$lib/stores/toast.svelte';
	import { checkpoints as store } from '$lib/stores/checkpoints.svelte';
	import { fileWorkspace as workspace } from '$lib/stores/fileWorkspace.svelte';

	async function loadWorkspace(): Promise<void> {
		if (!workspace.actor.trim()) {
			toasts.error('Enter an actor to inspect the workspace');
			return;
		}
		try {
			workspace.workspaceFiles = await getActorWorkspace(
				workspace.actor.trim(),
			);
		} catch (e) {
			console.error('Failed to load actor workspace:', e);
			toasts.error(
				'Workspace unavailable',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function runUndo(): Promise<void> {
		const actor = workspace.actor.trim();
		if (!actor) {
			toasts.error('Enter an actor for undo');
			return;
		}
		workspace.beginAction('undo');
		try {
			const snapshot = await undoEdit(actor);
			toasts.success('Undo recorded', `Snapshot ${snapshot.slice(0, 10)}`);
			store.bump('files');
		} catch (e) {
			console.error('Undo failed:', e);
			toasts.error('Undo failed', e instanceof Error ? e.message : undefined);
		} finally {
			workspace.endAction();
		}
	}

	async function runRedo(): Promise<void> {
		const actor = workspace.actor.trim();
		if (!actor) {
			toasts.error('Enter an actor for redo');
			return;
		}
		workspace.beginAction('redo');
		try {
			const snapshot = await redoEdit(actor);
			toasts.success('Redo recorded', `Snapshot ${snapshot.slice(0, 10)}`);
			store.bump('files');
		} catch (e) {
			console.error('Redo failed:', e);
			toasts.error('Redo failed', e instanceof Error ? e.message : undefined);
		} finally {
			workspace.endAction();
		}
	}

	async function runRebuildIndex(): Promise<void> {
		workspace.beginAction('rebuild');
		try {
			const { indexedCommits } = await rebuildIndex();
			workspace.indexStatus = await getIndexStatus().catch(() => null);
			toasts.success(
				'Source index rebuilt',
				`${indexedCommits} commits indexed`,
			);
		} catch (e) {
			console.error('Index rebuild failed:', e);
			toasts.error(
				'Index rebuild failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			workspace.endAction();
		}
	}
</script>

<Card
	title="Workspace & index"
	description="Actor undo/redo, workspace inspection and source-index health"
>
	<div class="mb-2">
		<Input
			label="Actor"
			size="sm"
			value={workspace.actor}
			oninput={(event: Event) =>
				workspace.setActor((event.target as HTMLInputElement).value)}
			placeholder="Actor partition"
		/>
	</div>
	<div class="flex flex-wrap gap-2">
		<Button
			variant="outline"
			size="sm"
			disabled={workspace.busy}
			onclick={() => void runUndo()}
		>
			<Icon name="arrow-left" size={13} />
			Undo
		</Button>
		<Button
			variant="outline"
			size="sm"
			disabled={workspace.busy}
			onclick={() => void runRedo()}
		>
			Redo
		</Button>
		<Button
			variant="outline"
			size="sm"
			disabled={workspace.busy}
			onclick={() => void loadWorkspace()}
		>
			Inspect workspace
		</Button>
		<Button
			variant="outline"
			size="sm"
			disabled={workspace.busy}
			onclick={() => void runRebuildIndex()}
		>
			<Icon name="database" size={13} />
			Rebuild index
		</Button>
	</div>
	{#if workspace.indexStatus}
		<p class="mt-2 text-caption tabular-nums text-muted-foreground">
			Index: {workspace.indexStatus.indexedCommits}/{workspace.indexStatus
				.totalCommits} commits{workspace.indexStatus.needsRebuild
				? ' · rebuild recommended'
				: ' · healthy'}
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
				Showing 20 of {workspace.workspaceFiles.length} files.
			</p>
		{/if}
	{/if}
</Card>
