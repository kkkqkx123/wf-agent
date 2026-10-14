<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import {
		listEditSessions,
		beginEditSession,
		rollbackEditSession,
		type EditSession,
	} from '$lib/services/checkpoints';
	import { toasts } from '$lib/stores/toast.svelte';
	import { checkpoints as store } from '$lib/stores/checkpoints.svelte';
	import { fileWorkspace as workspace } from '$lib/stores/fileWorkspace.svelte';

	let sessions = $state<EditSession[]>([]);

	// This card owns the session list: every files-domain bump reloads it.
	$effect(() => {
		void store.filesVersion;
		void reload();
	});

	async function reload(): Promise<void> {
		try {
			sessions = await listEditSessions();
		} catch (e) {
			console.error('Failed to load edit sessions:', e);
		}
	}

	async function runBeginSession(): Promise<void> {
		workspace.beginAction('begin-session');
		try {
			const id = await beginEditSession();
			toasts.success('Edit session started', id);
			store.bump('files');
		} catch (e) {
			console.error('Begin session failed:', e);
			workspace.endAction('Begin session failed');
			toasts.error(
				'Begin session failed',
				e instanceof Error ? e.message : undefined,
			);
			return;
		}
		workspace.endAction();
	}

	async function runRollbackSession(session: EditSession): Promise<void> {
		const actor = workspace.actor.trim();
		if (!actor) {
			toasts.error('Enter an actor to roll back the session');
			return;
		}
		workspace.beginAction(`rollback-${session.id}`);
		try {
			const snapshot = await rollbackEditSession(session.id, actor);
			toasts.success(
				'Session rolled back',
				`Snapshot ${snapshot.slice(0, 10)}`,
			);
			store.bump('files');
		} catch (e) {
			console.error('Session rollback failed:', e);
			toasts.error(
				'Session rollback failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			workspace.endAction();
		}
	}
</script>

<Card
	title="Edit sessions"
	description="Staged multi-file operations; rollback appends a revert commit"
>
	{#snippet actions()}
		<Button
			variant="outline"
			size="sm"
			disabled={workspace.busy}
			onclick={() => void runBeginSession()}
		>
			<Icon name="plus" size={13} />
			New session
		</Button>
	{/snippet}
	<div class="mb-2">
		<Input
			label="Rollback actor"
			size="sm"
			value={workspace.actor}
			oninput={(event: Event) =>
				workspace.setActor((event.target as HTMLInputElement).value)}
			placeholder="Actor owning the session partition"
		/>
	</div>
	{#if workspace.actionError}
		<p class="mb-2 text-caption text-destructive">{workspace.actionError}</p>
	{/if}
	{#if sessions.length === 0}
		<p class="text-caption text-muted-foreground">No staged edit sessions.</p>
	{:else}
		<ul class="space-y-2">
			{#each sessions as session (session.id)}
				<li class="flex flex-wrap items-center gap-x-2 gap-y-1 text-caption">
					<span class="min-w-0 flex-1 truncate font-mono">{session.id}</span>
					{#if session.label}
						<span class="text-micro text-muted-foreground">{session.label}</span
						>
					{/if}
					<Button
						variant="outline"
						size="sm"
						disabled={workspace.busy || !workspace.actor.trim()}
						title="Roll back this session on the actor partition"
						onclick={() => void runRollbackSession(session)}
					>
						Rollback
					</Button>
				</li>
			{/each}
		</ul>
	{/if}
</Card>
