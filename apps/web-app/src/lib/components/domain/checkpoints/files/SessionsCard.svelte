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
	import { t } from '$lib/i18n';

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
			toasts.success(t('domain.files.sessionStarted'), id);
			store.bump('files');
		} catch (e) {
			console.error('Begin session failed:', e);
			toasts.error(
				t('domain.files.sessionStartFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			workspace.endAction();
		}
	}

	async function runRollbackSession(session: EditSession): Promise<void> {
		const actor = workspace.actor.trim();
		if (!actor) {
			toasts.error(t('domain.files.actorRequiredRollbackSession'));
			return;
		}
		workspace.beginAction(`rollback-${session.id}`);
		try {
			const snapshot = await rollbackEditSession(session.id, actor);
			toasts.success(
				t('domain.files.sessionRolledBack'),
				t('domain.files.snapshot', { id: snapshot.slice(0, 10) }),
			);
			store.bump('files');
		} catch (e) {
			console.error('Session rollback failed:', e);
			toasts.error(
				t('domain.files.sessionRollbackFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			workspace.endAction();
		}
	}
</script>

<Card
	title={t('domain.files.sessionsTitle')}
	description={t('domain.files.sessionsDescription')}
>
	{#snippet actions()}
		<Button
			variant="outline"
			size="sm"
			disabled={workspace.busy}
			onclick={() => void runBeginSession()}
		>
			<Icon name="plus" size={13} />
			{t('domain.files.newSession')}
		</Button>
	{/snippet}
	<div class="mb-2">
		<Input
			label={t('domain.files.rollbackActor')}
			size="sm"
			value={workspace.actor}
			oninput={(event: Event) =>
				workspace.setActor((event.target as HTMLInputElement).value)}
			placeholder={t('domain.files.actorPartitionOwner')}
		/>
	</div>
	{#if sessions.length === 0}
		<p class="text-caption text-muted-foreground">
			{t('domain.files.noSessions')}
		</p>
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
						title={t('domain.files.rollbackSessionTitle')}
						onclick={() => void runRollbackSession(session)}
					>
						{t('domain.files.rollback')}
					</Button>
				</li>
			{/each}
		</ul>
	{/if}
</Card>
