<script lang="ts">
	import { onDestroy, onMount, tick, untrack } from 'svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Separator from '#lib/ui/components/Separator.svelte';
	import type TerminalInstance from './TerminalInstance.svelte';
	import {
		normalizePtyRepoKey,
		ptyStoreFor,
		type PtySessionStore,
	} from '#lib/stores/pty-session.svelte';
	import { getPty } from '#lib/services/pty';
	import {
		clearPtyTrace,
		getPtyTrace,
		installPtyErrorCapture,
		isPtyDebugEnabled,
		ptyTrace,
		readPtyPersisted,
		runPtyDiag,
		uninstallPtyErrorCapture,
	} from './pty-debug';
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
	let instance: TerminalInstance | undefined = $state();
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
	let debugMsg = $state('');

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
			ptyTrace('panel.effect.skip', { reason: !isOpen ? 'closed' : 'no-key', key });
			return;
		}
		if (ensuredKey === key) return;
		ensuredKey = key;
		const current = store;
		ptyTrace('panel.effect.run', { key, sessions: current.sessions.length });
		void current.reconcile().then(() => {
			if (!open) return;
			if (normalizePtyRepoKey(repo) !== key) return;
			// Restored sessions may all be exited (stale ids after a reload):
			// keep their scrollback tabs and ensure one live session so the
			// panel never shows a prompt-looking dead terminal. A persisted
			// empty list means the user closed all, so stay empty instead of
			// resurrecting. Only a fresh repo without any stored record
			// auto-creates its first session.
			if (current.sessions.length === 0) {
				if (!current.hasStoredRecord) {
					ptyTrace('panel.effect.autospawn', { reason: 'empty-fresh' });
					void spawnAndFocus();
				}
			} else if (!current.sessions.some((s) => s.status !== 'exited')) {
				ptyTrace('panel.effect.autospawn', {
				reason: 'all-exited',
					sessions: current.sessions.length,
				});
				void spawnAndFocus();
			}
		});
	});

	onDestroy(() => {
		store.close();
		uninstallPtyErrorCapture();
		if (typeof window !== 'undefined') {
			try {
				const w = window as unknown as Record<string, unknown>;
				if (w.__ptyDiag) delete w.__ptyDiag;
				if (w.__ptyTrace) delete w.__ptyTrace;
			} catch {
				// Cleanup is best effort only.
			}
		}
	});

	onMount(() => {
		if (!isPtyDebugEnabled()) return;
		installPtyErrorCapture();
		ptyTrace('panel.mount', { repo: normalizedRepo, open });
		try {
			const w = window as unknown as Record<string, unknown>;
			w.__ptyTrace = getPtyTrace;
			w.__ptyDiag = () =>
				runPtyDiag(store).then((result) => {
					console.debug('[pty] diag', result);
					return result;
				});
		} catch {
			// Console helpers are best effort only.
		}
	});

	function closePanel(): void {
		onClose?.();
	}

	/** Focus the active terminal instance (used when the host panel opens). */
	export function focusActive(): void {
		instance?.focus();
	}

	async function handleConnect(id: string): Promise<void> {
		if (!store.byId(id)) return;
		ptyTrace('panel.connect.start', { id });
		try {
			const view = await getPty(id);
			if (!store.byId(id)) return;
			store.syncMetaFromServer(id, view);
			instance?.adoptServerSize(id, view.rows, view.cols);
			ptyTrace('panel.connect.ok', { id });
		} catch (error) {
			ptyTrace('panel.connect.miss', {
				id,
				message: error instanceof Error ? error.message : String(error),
			});
			// Session vanished; adopt() on next mount drops it.
		}
	}

	function handleSessionExited(id: string): void {
		if (!store.byId(id)) return;
		ptyTrace('panel.exited', { id });
		failedId = id;
	}

	function handleConnectFailed(id: string): void {
		if (!store.byId(id)) return;
		ptyTrace('panel.connectFailed', { id });
		failedId = id;
	}

	function clearFailure(): void {
		failedId = null;
	}

	function removeSession(id: string | null | undefined): void {
		if (!id) return;
		ptyTrace('panel.remove.click', { id, sessions: store.sessions.length });
		if (failedId === id) failedId = null;
		void store.remove(id);
	}

	async function spawnAndFocus(command?: string): Promise<void> {
		ptyTrace('panel.spawn.click', { sessions: store.sessions.length, command: command ?? null });
		const session = await store.spawn({ cwd, env, command });
		if (session) {
			ptyTrace('panel.spawn.done', { id: session.id });
			await tick();
			instance?.focus();
			return;
		}
		ptyTrace('panel.spawn.empty', {});
		toasts.error(t('pty.ptyPanel.newTerminal'), t('pty.ptyPanel.disconnected'));
	}

	async function copyDebugReport(): Promise<void> {
		const persisted = readPtyPersisted(normalizedRepo);
		const report = {
			url: typeof window === 'undefined' ? '' : window.location.href,
			repoKey: normalizedRepo,
			sessions: store.sessions.map((s) => ({
				id: s.id,
				title: s.title,
				status: s.status ?? null,
			})),
			activeId: store.activeId,
			failedId,
			persisted,
			trace: getPtyTrace(),
		};
		debugMsg = '';
		try {
			await navigator.clipboard.writeText(JSON.stringify(report, null, 2));
			debugMsg = `copied ${report.trace.length} trace entries`;
		} catch {
			console.debug('[pty] report', report);
			debugMsg = 'clipboard blocked; report printed to console';
		}
		ptyTrace('panel.report', { sessions: report.sessions.length });
	}

	async function runDiag(): Promise<void> {
		debugMsg = 'diag running...';
		try {
			const result = await runPtyDiag(store);
			console.debug('[pty] diag', result);
			debugMsg = `diag done: spawned=${result.spawnedId ?? 'none'} local=${result.localAfterRemove.ids.length} server=${result.serverAfterRemove.length}`;
		} catch (error) {
			debugMsg = `diag failed: ${error instanceof Error ? error.message : String(error)}`;
		}
	}

	async function restartExited(id: string): Promise<void> {
		if (!store.byId(id)) return;
		clearFailure();
		await store.clone(id);
		await tick();
		instance?.focus();
	}
</script>

{#snippet debugBar()}
	{#if isPtyDebugEnabled()}
		<div class="mb-2 flex flex-wrap items-center gap-2 rounded-md border border-dashed border-border px-2 py-1 text-micro text-muted-foreground">
			<span>pty-debug</span>
			<Button size="sm" variant="outline" onclick={() => void copyDebugReport()}>
				Copy report
			</Button>
			<Button size="sm" variant="outline" onclick={() => void runDiag()}>
				Run diag
			</Button>
			<Button
				size="sm"
				variant="ghost"
				onclick={() => {
					clearPtyTrace();
					debugMsg = 'trace cleared';
				}}
			>
				Clear
			</Button>
			{#if debugMsg}
				<span>{debugMsg}</span>
			{/if}
		</div>
	{/if}
{/snippet}

{#snippet sessionTabs()}
	<div class="flex flex-wrap items-center gap-1">
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
					icon="x"
					label={t('pty.ptyPanel.closeTerminal')}
					compact
					variant="ghost"
					onclick={() => removeSession(session.id)}
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
		<Button size="sm" variant="outline" onclick={() => void spawnAndFocus()}>
			{t('pty.ptyPanel.newTerminal')}
		</Button>
	</div>
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
		{#key store.activeId}
			{@const active = store.active()}
			{#if active}
				{#if terminalView}
					{@const TerminalView = terminalView}
					<svelte:boundary>
						<TerminalView
							bind:this={instance}
							session={active}
							{store}
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
			{/if}
		{/key}
	</div>
{/snippet}

<div class={open ? 'contents' : 'hidden'}>
	{@render debugBar()}
	{#if embedded}
		<div class="flex h-full min-h-0 flex-col gap-1">
			{#if store.sessions.length === 0}
				{@render emptyState()}
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
			{#if store.sessions.length === 0}
				{@render emptyState()}
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
						onclick={() => removeSession(store.activeId)}
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
</div>
