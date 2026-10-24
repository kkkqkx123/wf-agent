<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Separator from '#lib/ui/components/Separator.svelte';
	import TerminalInstance from './TerminalInstance.svelte';
	import {
		ptyStoreFor,
		type PtySessionStore,
	} from '#lib/stores/pty-session.svelte';
	import { getPty } from '#lib/services/pty';
	import { t } from '#lib/i18n';

	interface Props {
		/** Repo root; scopes the session store and persistence. */
		repo: string;
		open: boolean;
		onClose: () => void;
	}

	let { repo, open, onClose }: Props = $props();

	const store: PtySessionStore = $derived(ptyStoreFor(repo));

	let autoSpawned = false;
	let failedId = $state<string | null>(null);
	let instance: TerminalInstance | undefined = $state();
	let editingId = $state<string | null>(null);
	let editingTitle = $state('');

	function startRename(id: string, title: string): void {
		editingId = id;
		editingTitle = title;
	}

	function commitRename(id: string): void {
		if (editingId !== id) return;
		editingId = null;
		store.rename(id, editingTitle);
	}

	function cancelRename(): void {
		editingId = null;
	}

	onMount(() => {
		void store.reconcile().then(() => {
			if (!autoSpawned && store.sessions.length === 0) {
				autoSpawned = true;
				void store.spawn().then((session) => {
					if (session) instance?.focus();
				});
			}
		});
	});

	onDestroy(() => {
		store.close();
	});

	function closePanel(): void {
		onClose();
	}

	async function handleConnect(id: string): Promise<void> {
		try {
			const view = await getPty(id);
			store.syncFromServer(id, view);
		} catch {
			// Session vanished; adopt() on next mount drops it.
		}
	}

	function handleSessionExited(id: string): void {
		failedId = id;
	}

	function handleConnectFailed(id: string): void {
		failedId = id;
	}

	function clearFailure(): void {
		failedId = null;
	}

	async function restartExited(id: string): Promise<void> {
		clearFailure();
		await store.clone(id);
		instance?.focus();
	}
</script>

{#if open && store.sessions.length > 0}
	<Card
		title={t('pty.ptyPanel.title')}
		description={t('pty.ptyPanel.description')}
	>
		<div class="mb-2 flex flex-wrap items-center gap-1">
			{#each store.sessions as session (session.id)}
				<div
					class="flex items-center gap-0.5 rounded-md border border-border px-1.5 py-0.5 text-micro {store.activeId ===
					session.id
						? 'border-primary bg-primary/10'
						: 'bg-muted/40'}"
				>
					{#if editingId === session.id}
						<input
							class="w-24 bg-transparent px-1 py-0.5 outline-none"
							bind:value={editingTitle}
							onblur={() => commitRename(session.id)}
							onkeydown={(e) => {
								if (e.key === 'Enter') commitRename(session.id);
								else if (e.key === 'Escape') cancelRename();
							}}
							onclick={(e) => e.stopPropagation()}
						/>
					{:else}
						<button
							type="button"
							class="cursor-pointer px-1 py-0.5 hover:text-foreground"
							title={t('pty.ptyPanel.renameHint')}
							onclick={() => {
								store.setActive(session.id);
								instance?.focus();
							}}
							ondblclick={() => startRename(session.id, session.title)}
						>
							{session.title}
							{#if session.status === 'exited'}
								<span class="text-muted-foreground"
									>({t('pty.ptyPanel.exited')})</span
								>
							{/if}
						</button>
					{/if}
					<IconButton
						icon="chevron-left"
						label={t('pty.ptyPanel.moveLeft')}
						compact
						variant="ghost"
						onclick={() => store.move(session.id, -1)}
					/>
					<IconButton
						icon="chevron-right"
						label={t('pty.ptyPanel.moveRight')}
						compact
						variant="ghost"
						onclick={() => store.move(session.id, 1)}
					/>
					<IconButton
						icon="x"
						label={t('pty.ptyPanel.closeTerminal')}
						compact
						variant="ghost"
						onclick={() => void store.remove(session.id)}
					/>
				</div>
			{/each}
			<IconButton
				icon="plus"
				label={t('pty.ptyPanel.newTerminal')}
				compact
				variant="ghost"
				onclick={() => {
					void store.spawn().then((session) => {
						if (session) instance?.focus();
					});
				}}
			/>
		</div>

		{#if failedId}
			<div
				class="mb-2 flex items-center gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1 text-micro"
			>
				<span>{t('pty.ptyPanel.disconnected')}</span>
				<Button
					size="sm"
					variant="outline"
					onclick={() => void restartExited(failedId!)}
				>
					{t('pty.ptyPanel.restart')}
				</Button>
				<Button size="sm" variant="ghost" onclick={clearFailure}>
					{t('pty.ptyPanel.dismiss')}
				</Button>
			</div>
		{/if}

		<div
			class="h-72 overflow-hidden rounded-md border border-border bg-background"
		>
			{#key store.activeId}
				{#if store.active()}
					<TerminalInstance
						bind:this={instance}
						session={store.active()!}
						{store}
						onConnect={(id) => void handleConnect(id)}
						onSessionExited={handleSessionExited}
						onConnectFailed={handleConnectFailed}
					/>
				{/if}
			{/key}
		</div>

		<Separator class="my-2" />

		<div class="flex gap-2">
			<Button
				size="sm"
				variant="ghost"
				onclick={() => void store.remove(store.activeId!)}
				disabled={!store.activeId}
			>
				{t('pty.ptyPanel.closeActive')}
			</Button>
			<Button size="sm" variant="ghost" onclick={closePanel}>
				{t('pty.ptyPanel.hidePanel')}
			</Button>
		</div>
	</Card>
{/if}
