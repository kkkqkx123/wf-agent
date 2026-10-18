<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Separator from '@wf-agent/ui/components/Separator.svelte';
	import TerminalInstance from './TerminalInstance.svelte';
	import {
		ptyStoreFor,
		type PtySessionStore,
	} from '$lib/stores/pty-session.svelte';
	import { getPty } from '$lib/services/pty';

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

	onMount(() => {
		if (!autoSpawned && store.sessions.length === 0) {
			autoSpawned = true;
			void store.spawn().then((session) => {
				if (session) instance?.focus();
			});
		}
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
		title="Terminal"
		description="Repo-locked interactive shell; output streams over a persistent PTY."
	>
		<div class="mb-2 flex flex-wrap items-center gap-1">
			{#each store.sessions as session (session.id)}
				<div
					class="flex items-center gap-0.5 rounded-md border border-border px-1.5 py-0.5 text-micro {store.activeId ===
					session.id
						? 'border-primary bg-primary/10'
						: 'bg-muted/40'}"
				>
					<button
						type="button"
						class="cursor-pointer px-1 py-0.5 hover:text-foreground"
						onclick={() => {
							store.setActive(session.id);
							instance?.focus();
						}}
					>
						{session.title}
						{#if session.status === 'exited'}
							<span class="text-muted-foreground">(exited)</span>
						{/if}
					</button>
					<IconButton
						icon="chevron-left"
						label="Move left"
						compact
						variant="ghost"
						onclick={() => store.move(session.id, -1)}
					/>
					<IconButton
						icon="chevron-right"
						label="Move right"
						compact
						variant="ghost"
						onclick={() => store.move(session.id, 1)}
					/>
					<IconButton
						icon="x"
						label="Close terminal"
						compact
						variant="ghost"
						onclick={() => void store.remove(session.id)}
					/>
				</div>
			{/each}
			<IconButton
				icon="plus"
				label="New terminal"
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
			<div class="mb-2 flex items-center gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1 text-micro">
				<span>Terminal disconnected. The session may have ended.</span>
				<Button size="sm" variant="outline" onclick={() => void restartExited(failedId!)}>
					Restart
				</Button>
				<Button size="sm" variant="ghost" onclick={clearFailure}>Dismiss</Button>
			</div>
		{/if}

		<div class="h-72 overflow-hidden rounded-md border border-border bg-background">
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
				Close active
			</Button>
			<Button size="sm" variant="ghost" onclick={closePanel}>
				Hide panel
			</Button>
		</div>
	</Card>
{/if}
