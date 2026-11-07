<script lang="ts">
	import { onDestroy, tick, untrack } from 'svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import Separator from '#lib/ui/components/Separator.svelte';
	import type TerminalInstance from './TerminalInstance.svelte';
	import type { TerminalApi } from './TerminalInstance.svelte';
	import {
		allPtyStores,
		discoverForeignPtyStores,
		normalizePtyRepoKey,
		ptyStoreFor,
		type LocalPtySession,
		type PtySessionStore,
	} from '#lib/stores/pty-session.svelte';
	import {
		refitPtyInstance,
		focusPtyInstance,
		ptyInstanceFor,
		registerPtyInstance,
		unregisterPtyInstance,
	} from '#lib/stores/pty-instances.svelte';
	import { getPty } from '#lib/services/pty';
	import { toasts } from '#lib/stores/toast.svelte';
	import { t } from '#lib/i18n';

	interface Props {
		/** Repo root; scopes the session store and persistence. */
		repo: string;
		/** Working directory for newly spawned sessions. Falls back to server default when unset. */
		cwd?: string;
		/** Environment overrides for newly spawned sessions (applied as-is server-side). */
		env?: Record<string, string>;
		/** Inline height for the terminal viewport; overrides the default fixed height. */
		viewportStyle?: string;
		/** Stretch the card and viewport to fill the parent height (full-page hosts). */
		fill?: boolean;
		/** Minimal inline chrome for IDE embedding (no card title or footer). */
		embedded?: boolean;
		/** One-click session starters (label shown, command spawned in panel cwd). */
		quickCommands?: Array<{ label: string; command: string }>;
		open: boolean;
		onClose?: () => void;
	}

	let {
		repo,
		cwd,
		env,
		viewportStyle,
		fill = false,
		embedded = false,
		quickCommands = [],
		open,
		onClose,
	}: Props = $props();

	const normalizedRepo = $derived(normalizePtyRepoKey(repo));
	// The session registry creates and hydrates the store on first use, which
	// counts as a state write. Resolving it untracked keeps the derived pure
	// so a first-seen repo key never throws state_unsafe_mutation; the key
	// itself stays tracked, so repo switches still swap stores.
	const store: PtySessionStore = $derived.by(() => {
		const key = normalizedRepo;
		return untrack(() => ptyStoreFor(key));
	});

	let ensuredKey: string | null = $state(null);
	let failedId = $state<string | null>(null);
	let attachOpen = $state(false);
	let closeConfirmId = $state<string | null>(null);
	let closeConfirmOpen = $state(false);

	/** Visible tabs; detached sessions hide from every panel but keep running. */
	const attached = $derived(store.attachedSessions());
	const detachedMine = $derived(store.detachedSessions());
	/**
	 * Foreign session groups for the attach dialog, excluding this panel
	 * store. Discovery opens persisted-but-unmounted stores on dialog open
	 * so renamed roots and preview demo seeds show up as well; the registry
	 * and each session list stay tracked, so the groups refresh on change.
	 */
	const foreignGroups = $derived(
		allPtyStores()
			.filter((s) => s !== store && s.repo && s.sessions.length > 0)
			.map((s) => ({ repo: s.repo, sessions: [...s.sessions] })),
	);
	const foreignCount = $derived(foreignGroups.reduce((n, g) => n + g.sessions.length, 0));
	const attachableCount = $derived(detachedMine.length + foreignCount);

	function registerInstance(id: string, api: TerminalApi): void {
		registerPtyInstance(id, api);
		if (id === store.activeId) void tick().then(() => api.refit());
	}

	function unregisterInstance(id: string): void {
		unregisterPtyInstance(id);
	}

	function focusInstance(id: string | null): void {
		focusPtyInstance(id);
	}
	// The terminal view (and the xterm bundle) loads on demand so the route
	// chunks stay light and a load failure degrades to panel retry.
	let terminalView: typeof TerminalInstance | null = $state(null);
	let terminalViewFailed = $state(false);

	function loadTerminalView(): void {
		terminalViewFailed = false;
		terminalView = null;
		void import('./TerminalInstance.svelte').then(
			(module) => {
				terminalView = module.default;
			},
			() => {
				terminalViewFailed = true;
			},
		);
	}

	loadTerminalView();
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

	$effect(() => {
		const key = normalizedRepo;
		const isOpen = open;
		if (!isOpen || key === '') {
			return;
		}
		if (ensuredKey === key) return;
		ensuredKey = key;
		const current = store;
		void current.reconcile().then(() => {
			if (!open) return;
			if (normalizePtyRepoKey(repo) !== key) return;
			// Restored sessions may all be exited (stale ids after a reload):
			// keep their scrollback tabs and ensure one live session so the
			// panel never shows a prompt-looking dead terminal. A persisted
			// empty list means the user closed all, so stay empty instead of
			// resurrecting. Only a fresh repo without any stored record
			// auto-creates its first session. Detached tabs never count: an
			// all-detached panel stays empty with an attach entry point.
			if (current.attachedSessions().length === 0) {
				if (current.sessions.length === 0 && !current.hasStoredRecord) {
					void spawnAndFocus();
				}
			} else if (!current.hasAttachedLive()) {
				void spawnAndFocus();
			}
		});
	});

	onDestroy(() => {
		store.close();
	});

	function closePanel(): void {
		onClose?.();
	}

	/** Focus the active terminal instance (used when the host panel opens). */
	export function focusActive(): void {
		focusInstance(store.active()?.id ?? null);
	}

	// Hidden tabs report zero size until shown; re-measure the newly
	// visible instance after each switch so fit matches the viewport.
	$effect(() => {
		const active = store.activeId;
		if (!active) return;
		void tick().then(() => refitPtyInstance(active));
	});

	async function handleConnect(id: string): Promise<void> {
		if (!store.byId(id)) return;
		store.reportSocketOpen(id);
		try {
			const view = await getPty(id);
			if (!store.byId(id)) return;
			store.syncMetaFromServer(id, view);
			ptyInstanceFor(id)?.adoptServerSize(id, view.rows, view.cols);
		} catch {
			// Session vanished; adopt() on next mount drops it.
		}
	}

	function handleSessionExited(id: string): void {
		if (!store.byId(id)) return;
		store.markExited(id);
		failedId = id;
	}

	function handleConnectFailed(id: string): void {
		if (!store.byId(id)) return;
		store.markConnectFailed(id);
		failedId = id;
	}

	function clearFailure(): void {
		failedId = null;
	}

	function removeSession(id: string | null | undefined, owner: PtySessionStore = store): void {
		if (!id) return;
		if (failedId === id) failedId = null;
		void owner.remove(id);
	}

	/** Detach hides the tab and keeps the server process; reattach from the dialog. */
	function detachSession(id: string | null | undefined): void {
		if (!id || !store.detach(id)) return;
		void tick().then(() => focusInstance(store.activeId));
		toasts.push({
			title: t('pty.ptyPanel.detachedToastTitle'),
			description: t('pty.ptyPanel.detachedToastDescription'),
			tone: 'info',
			action: {
				label: t('pty.ptyPanel.detachedToastAction'),
				run: () => attachMine(id),
			},
		});
	}

	/** Close ends the process; running sessions confirm so detach stays the quick path. */
	function requestClose(id: string | null | undefined): void {
		if (!id) return;
		const session = ownerStoreOf(id)?.byId(id);
		if (!session) return;
		if (session.status === 'exited') {
			removeSession(id);
			return;
		}
		closeConfirmId = id;
		closeConfirmOpen = true;
	}

	function cancelClose(): void {
		closeConfirmId = null;
		closeConfirmOpen = false;
	}

	function confirmClose(): void {
		const id = closeConfirmId;
		const owner = id ? (ownerStoreOf(id) ?? store) : store;
		closeConfirmId = null;
		closeConfirmOpen = false;
		removeSession(id, owner);
	}

	function ownerStoreOf(id: string): PtySessionStore | undefined {
		if (store.byId(id)) return store;
		return allPtyStores().find((s) => s.byId(id));
	}

	function openAttach(): void {
		discoverForeignPtyStores(store);
		attachOpen = true;
	}

	function attachMine(id: string): void {
		store.attach(id);
		attachOpen = false;
		void tick().then(() => focusInstance(id));
	}

	/** Adopt a foreign session: move its record over, the server process is untouched. */
	function attachForeign(id: string): void {
		const source = ownerStoreOf(id);
		if (!source) return;
		if (source === store) {
			attachMine(id);
			return;
		}
		const ejected = source.eject(id);
		if (!ejected) return;
		if (!store.ingest(ejected.record, ejected.detached)) {
			source.ingest(ejected.record, ejected.detached);
			toasts.error(t('pty.ptyPanel.attach'), t('pty.ptyPanel.targetFull'));
			return;
		}
		attachOpen = false;
		void tick().then(() => focusInstance(id));
	}

	function restartRow(id: string): void {
		const source = ownerStoreOf(id);
		if (!source) return;
		if (source !== store) {
			const ejected = source.eject(id);
			if (!ejected) return;
			if (!store.ingest(ejected.record, ejected.detached)) {
				source.ingest(ejected.record, ejected.detached);
				toasts.error(t('pty.ptyPanel.attach'), t('pty.ptyPanel.targetFull'));
				return;
			}
		}
		clearFailure();
		attachOpen = false;
		void store.clone(id).then(() => {
			void tick().then(() => focusInstance(store.activeId));
		});
	}

	function closeRow(id: string): void {
		attachOpen = false;
		requestClose(id);
	}

	function originSummary(session: LocalPtySession): string {
		return session.origin?.command ?? session.origin?.cwd ?? '';
	}

	function statusText(session: LocalPtySession): string {
		return session.status === 'exited' ? t('pty.ptyPanel.exited') : t('pty.ptyPanel.running');
	}

	async function spawnAndFocus(command?: string): Promise<void> {
		const session = await store.spawn({ cwd, env, command });
		if (session) {
			await tick();
			focusInstance(session.id);
			return;
		}
		toasts.error(t('pty.ptyPanel.newTerminal'), t('pty.ptyPanel.disconnected'));
	}

	async function restartExited(id: string): Promise<void> {
		if (!store.byId(id)) return;
		clearFailure();
		await store.clone(id);
		await tick();
		focusInstance(store.activeId);
	}
</script>

{#snippet sessionTabs()}
	<div class="flex flex-wrap items-center gap-1">
		{#each attached as session (session.id)}
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
							focusInstance(session.id);
						}}
						ondblclick={() => startRename(session.id, session.title)}
					>
						{session.title}
						{#if session.status === 'exited'}
							<span class="text-muted-foreground">({t('pty.ptyPanel.exited')})</span>
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
					icon="eye-off"
					label={t('pty.ptyPanel.detach')}
					compact
					variant="ghost"
					onclick={() => detachSession(session.id)}
				/>
				<IconButton
					icon="x"
					label={t('pty.ptyPanel.closeTerminal')}
					compact
					variant="ghost"
					onclick={() => requestClose(session.id)}
				/>
			</div>
		{/each}
		<IconButton
			icon="plus"
			label={t('pty.ptyPanel.newTerminal')}
			compact
			variant="ghost"
			onclick={() => void spawnAndFocus()}
		/>
		{#if attachableCount > 0}
			<IconButton
				icon="eye"
				label={t('pty.ptyPanel.attach')}
				compact
				variant="ghost"
				onclick={() => openAttach()}
			/>
			<span class="text-micro text-muted-foreground" aria-hidden="true">{attachableCount}</span>
		{/if}
		{#each quickCommands as quick (quick.command)}
			<Button size="sm" variant="outline" onclick={() => void spawnAndFocus(quick.command)}>
				{quick.label}
			</Button>
		{/each}
	</div>
{/snippet}

{#snippet emptyState()}
	<div class="flex flex-col items-start gap-2">
		<p class="text-caption text-muted-foreground">{t('pty.ptyPanel.noSessions')}</p>
		<div class="flex gap-2">
			<Button size="sm" variant="outline" onclick={() => void spawnAndFocus()}>
				{t('pty.ptyPanel.newTerminal')}
			</Button>
			{#if foreignCount > 0}
				<Button size="sm" variant="ghost" onclick={() => openAttach()}>
					{t('pty.ptyPanel.attach')}
				</Button>
			{/if}
		</div>
	</div>
{/snippet}

{#snippet allDetachedState()}
	<div class="flex flex-col items-start gap-2">
		<p class="text-caption text-muted-foreground">{t('pty.ptyPanel.allDetached')}</p>
		<div class="flex gap-2">
			<Button size="sm" variant="outline" onclick={() => openAttach()}>
				{t('pty.ptyPanel.attach')}
			</Button>
			<Button size="sm" variant="ghost" onclick={() => void spawnAndFocus()}>
				{t('pty.ptyPanel.newTerminal')}
			</Button>
		</div>
	</div>
{/snippet}

{#snippet attachRow(session: LocalPtySession, foreign: boolean)}
	<div class="flex items-center gap-2 rounded-md border border-border px-2 py-1.5">
		<button
			type="button"
			class="min-w-0 flex-1 cursor-pointer truncate text-left text-body hover:text-foreground"
			title={originSummary(session) || session.title}
			onclick={() => (foreign ? attachForeign(session.id) : attachMine(session.id))}
		>
			<span class="font-medium">{session.title}</span>
			<span class="text-micro text-muted-foreground">({statusText(session)})</span>
			{#if originSummary(session)}
				<span class="block truncate text-micro text-muted-foreground">{originSummary(session)}</span
				>
			{/if}
		</button>
		{#if session.status === 'exited'}
			<IconButton
				icon="plus"
				label={t('pty.ptyPanel.restart')}
				compact
				variant="ghost"
				onclick={() => restartRow(session.id)}
			/>
		{/if}
		<IconButton
			icon="x"
			label={t('pty.ptyPanel.closeTerminal')}
			compact
			variant="ghost"
			onclick={() => closeRow(session.id)}
		/>
	</div>
{/snippet}

{#snippet attachDialog()}
	<Dialog
		bind:open={attachOpen}
		title={t('pty.ptyPanel.attachDialogTitle')}
		description={t('pty.ptyPanel.attachDialogDescription')}
	>
		<div class="flex max-h-96 flex-col gap-3 overflow-y-auto">
			{#if detachedMine.length === 0 && foreignCount === 0}
				<p class="text-caption text-muted-foreground">{t('pty.ptyPanel.attachEmpty')}</p>
			{/if}
			{#if detachedMine.length > 0}
				<div class="flex flex-col gap-1.5">
					<p class="text-micro font-medium uppercase tracking-wide text-muted-foreground">
						{t('pty.ptyPanel.attachSectionMine')}
					</p>
					{#each detachedMine as session (session.id)}
						{@render attachRow(session, false)}
					{/each}
				</div>
			{/if}
			{#each foreignGroups as group (group.repo)}
				{#if group.sessions.length > 0}
					<div class="flex flex-col gap-1.5">
						<p
							class="truncate text-micro font-medium uppercase tracking-wide text-muted-foreground"
							title={group.repo}
						>
							{t('pty.ptyPanel.attachSectionOthers')} · {group.repo}
						</p>
						{#each group.sessions as session (session.id)}
							{@render attachRow(session, true)}
						{/each}
					</div>
				{/if}
			{/each}
		</div>
	</Dialog>
{/snippet}

{#snippet closeConfirmDialog()}
	<Dialog
		bind:open={closeConfirmOpen}
		title={t('pty.ptyPanel.closeConfirmTitle')}
		description={t('pty.ptyPanel.closeConfirmBody')}
		onclose={cancelClose}
	>
		<div class="flex justify-end gap-2">
			<Button size="sm" variant="ghost" onclick={cancelClose}>
				{t('common.cancel')}
			</Button>
			<Button size="sm" variant="outline" onclick={confirmClose}>
				{t('pty.ptyPanel.closeConfirmOk')}
			</Button>
		</div>
	</Dialog>
{/snippet}

{#snippet failureBanner()}
	{#if failedId && store.byId(failedId)}
		<div
			class="flex items-center gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1 text-micro"
		>
			<span>{t('pty.ptyPanel.disconnected')}</span>
			<Button size="sm" variant="outline" onclick={() => void restartExited(failedId!)}>
				{t('pty.ptyPanel.restart')}
			</Button>
			<Button size="sm" variant="ghost" onclick={clearFailure}>
				{t('pty.ptyPanel.dismiss')}
			</Button>
		</div>
	{/if}
{/snippet}

{#snippet terminalViewport(viewportClass: string)}
	<div class={viewportClass} style={viewportStyle}>
		{#if terminalView}
			{@const TerminalView = terminalView}
			{#each attached as session (session.id)}
				<!-- All tabs stay mounted; inactive ones hide so their sockets keep streaming. -->
				<div class="h-full w-full {store.activeId === session.id ? '' : 'hidden'}">
					<svelte:boundary>
						<TerminalView
							{session}
							{store}
							register={registerInstance}
							unregister={unregisterInstance}
							onConnect={(id) => void handleConnect(id)}
							onSessionExited={handleSessionExited}
							onConnectFailed={handleConnectFailed}
						/>
						<!-- The boundary passes (error, reset); the panel shows a generic message. -->
						<!-- eslint-disable-next-line @typescript-eslint/no-unused-vars -->
						{#snippet failed(_, reset)}
							<div class="flex h-full flex-col items-start justify-center gap-2 p-4">
								<p class="text-caption text-muted-foreground">
									{t('pty.ptyPanel.disconnected')}
								</p>
								<Button size="sm" variant="outline" onclick={() => reset()}>
									{t('common.retry')}
								</Button>
							</div>
						{/snippet}
					</svelte:boundary>
				</div>
			{/each}
		{:else if terminalViewFailed}
			<div class="flex h-full flex-col items-start justify-center gap-2 p-4">
				<p class="text-caption text-muted-foreground">{t('pty.ptyPanel.disconnected')}</p>
				<Button size="sm" variant="outline" onclick={loadTerminalView}>
					{t('common.retry')}
				</Button>
			</div>
		{:else}
			<div class="flex h-full flex-col items-start justify-center gap-2 p-4">
				<p class="text-caption text-muted-foreground">{t('common.loading')}</p>
			</div>
		{/if}
	</div>
{/snippet}

<div class={open ? 'contents' : 'hidden'}>
	{#if embedded}
		<div class="flex h-full min-h-0 flex-col gap-1">
			{#if attached.length === 0}
				{#if store.sessions.length === 0}
					{@render emptyState()}
				{:else}
					{@render allDetachedState()}
				{/if}
			{:else}
				{@render sessionTabs()}
				<div class="min-h-0 flex-1">
					{@render terminalViewport(
						'h-full min-h-0 overflow-hidden rounded-md border border-border bg-background',
					)}
				</div>
				{@render failureBanner()}
			{/if}
		</div>
	{:else}
		<Card
			title={t('pty.ptyPanel.title')}
			description={t('pty.ptyPanel.description')}
			class={fill ? 'flex h-full min-h-0 flex-col' : undefined}
			bodyClass={fill ? 'flex min-h-0 flex-1 flex-col' : undefined}
		>
			{#if attached.length === 0}
				{#if store.sessions.length === 0}
					{@render emptyState()}
				{:else}
					{@render allDetachedState()}
				{/if}
			{:else}
				<div class="mb-2">
					{@render sessionTabs()}
				</div>

				{@render failureBanner()}

				{@render terminalViewport(
					fill
						? 'min-h-0 flex-1 overflow-hidden rounded-md border border-border bg-background'
						: 'h-72 overflow-hidden rounded-md border border-border bg-background',
				)}

				<Separator class="my-2" />

				<div class="flex gap-2">
					<Button
						size="sm"
						variant="ghost"
						onclick={() => requestClose(store.activeId)}
						disabled={!store.activeId}
					>
						{t('pty.ptyPanel.closeActive')}
					</Button>
					{#if onClose}
						<Button size="sm" variant="ghost" onclick={closePanel}>
							{t('pty.ptyPanel.hidePanel')}
						</Button>
					{/if}
				</div>
			{/if}
		</Card>
	{/if}
	{@render attachDialog()}
	{@render closeConfirmDialog()}
</div>
