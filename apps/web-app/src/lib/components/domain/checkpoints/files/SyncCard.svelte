<script lang="ts">
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import Select from '#lib/ui/components/Select.svelte';
	import Skeleton from '#lib/ui/components/Skeleton.svelte';
	import Switch from '#lib/ui/components/Switch.svelte';
	import {
		exportToWorkspace,
		getExternalUpdateHint,
		getReconcilePoint,
		getSyncConfig,
		importFromExternal,
		rebaseBaseline,
		type ExternalHint,
		type ReconcilePoint,
		type SyncDefaults,
	} from '#lib/services/checkpoints';
	import { ApiHttpError } from '#lib/api/envelope';
	import { toasts } from '#lib/stores/toast.svelte';
	import { checkpoints as store } from '#lib/stores/checkpoints.svelte';
	import { fileWorkspace as workspace } from '#lib/stores/fileWorkspace.svelte';
	import { live } from '#lib/stores/live.svelte';
	import { formatRelativeTime } from '#lib/utils/format';
	import { t } from '#lib/i18n';

	const STREAM_POLL_MS = 120_000;
	const OFFLINE_POLL_MS = 15_000;

	let reconcile = $state<ReconcilePoint | null>(null);
	let hint = $state<ExternalHint | null>(null);
	let defaults = $state<SyncDefaults | null>(null);
	let configured = $state(false);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// Export dialog state: the overwrite checkbox gates submission, the
	// auto-commit default comes from the sync config when the dialog opens,
	// and an empty message lets the server suggest one.
	let exportOpen = $state(false);
	let exportConfirm = $state(false);
	let exportAutoCommit = $state(false);
	let exportMessage = $state('');

	// Rebase dialog state: commit prefix and behind are mutually exclusive,
	// mode defaults to borrow.
	let rebaseOpen = $state(false);
	let rebaseBranch = $state('');
	let rebasePrefix = $state('');
	let rebaseBehind = $state('');
	let rebaseMode = $state('borrow');

	let lastFingerprint = '';
	let timer: ReturnType<typeof setTimeout> | null = null;

	const modeOptions = [
		{ value: 'borrow', label: t('domain.files.syncRebaseBorrow') },
		{ value: 'copy', label: t('domain.files.syncRebaseCopy') },
	];

	function fingerprint(
		currentHint: ExternalHint | null,
		currentReconcile: ReconcilePoint | null,
	): string {
		return [
			currentHint?.externalHead ?? '',
			currentHint?.fileCount ?? '',
			currentHint?.internalMain ?? '',
			currentReconcile?.internalMain ?? '',
			currentReconcile?.externalCommit ?? '',
		].join('|');
	}

	function pollIntervalMs(): number {
		return live.state === 'open' ? STREAM_POLL_MS : OFFLINE_POLL_MS;
	}

	// Full load: all three read-only queries in parallel. A null sync config
	// means sync is unconfigured and the card stays hidden; transport
	// failures surface an error with retry instead.
	async function loadSync(): Promise<void> {
		loading = true;
		try {
			const [nextHint, nextReconcile, nextDefaults] = await Promise.all([
				getExternalUpdateHint(),
				getReconcilePoint(),
				getSyncConfig(),
			]);
			if (!nextDefaults) {
				configured = false;
				hint = null;
				reconcile = null;
				defaults = null;
				error = null;
				return;
			}
			configured = true;
			hint = nextHint;
			reconcile = nextReconcile;
			defaults = nextDefaults;
			lastFingerprint = fingerprint(hint, reconcile);
			error = null;
		} catch (e) {
			console.error('Failed to load git sync state:', e);
			error = e instanceof Error ? e.message : t('domain.files.syncLoadFailed');
		} finally {
			loading = false;
		}
	}

	// Poll only the external hint and the reconcile point. The fingerprint
	// guard skips view updates when nothing moved; failures keep the last
	// known state so the card never flashes empty.
	async function pollSync(): Promise<void> {
		if (!configured) return;
		try {
			const [nextHint, nextReconcile] = await Promise.all([
				getExternalUpdateHint(),
				getReconcilePoint(),
			]);
			const key = fingerprint(nextHint, nextReconcile);
			if (key === lastFingerprint) return;
			lastFingerprint = key;
			hint = nextHint;
			reconcile = nextReconcile;
		} catch (e) {
			console.error('Git sync poll failed, keeping last known state:', e);
		}
	}

	function schedulePoll(): void {
		if (typeof document === 'undefined') return;
		if (timer !== null) clearTimeout(timer);
		timer = setTimeout(() => {
			timer = null;
			if (!configured) {
				schedulePoll();
				return;
			}
			if (document.hidden) {
				schedulePoll();
				return;
			}
			void pollSync().finally(() => schedulePoll());
		}, pollIntervalMs());
	}

	// The card follows the files-domain refresh rhythm for its full load,
	// while the timer only polls the two moving sections.
	$effect(() => {
		void store.filesVersion;
		void loadSync();
	});

	// The poller only runs once sync is configured; it pauses while the page
	// is hidden and is torn down with the card.
	$effect(() => {
		if (!configured || typeof document === 'undefined') return;
		schedulePoll();
		const onVisibility = (): void => {
			schedulePoll();
			if (!document.hidden) void pollSync();
		};
		document.addEventListener('visibilitychange', onVisibility);
		return () => {
			document.removeEventListener('visibilitychange', onVisibility);
			if (timer !== null) {
				clearTimeout(timer);
				timer = null;
			}
		};
	});

	function syncFailureToast(failedTitle: string, e: unknown): void {
		if (e instanceof ApiHttpError) {
			if (e.status === 409 || e.code === 'CONFLICT' || e.code === 'CANCELLED') {
				toasts.error(t('domain.files.syncConflict'), e.message);
				return;
			}
			if (e.status === 404 || e.code === 'NOT_FOUND') {
				toasts.error(t('domain.files.syncNotFound'), e.message);
				return;
			}
			toasts.error(failedTitle, e.message);
			return;
		}
		toasts.error(failedTitle, e instanceof Error ? e.message : undefined);
	}

	function openExport(): void {
		exportAutoCommit = defaults?.autoCommit ?? false;
		exportConfirm = false;
		exportMessage = '';
		exportOpen = true;
	}

	async function runExport(): Promise<void> {
		if (!exportConfirm) return;
		workspace.beginAction('sync-export');
		try {
			const outcome = await exportToWorkspace(
				true,
				exportAutoCommit,
				exportMessage.trim() || undefined,
			);
			exportOpen = false;
			exportConfirm = false;
			exportMessage = '';
			toasts.success(t('domain.files.syncExported'), outcome.suggestedMessage);
			await loadSync();
			store.bump('files');
		} catch (e) {
			console.error('Sync export failed:', e);
			syncFailureToast(t('domain.files.syncExportFailed'), e);
		} finally {
			workspace.endAction();
		}
	}

	async function runImport(): Promise<void> {
		workspace.beginAction('sync-import');
		try {
			const outcome = await importFromExternal();
			toasts.success(
				t('domain.files.syncImported'),
				t('domain.files.syncImportedDetail', {
					commit: outcome.advancedTo.slice(0, 10),
					count: outcome.fileCount,
				}),
			);
			await loadSync();
			store.bump('files');
		} catch (e) {
			console.error('Sync import failed:', e);
			syncFailureToast(t('domain.files.syncImportFailed'), e);
		} finally {
			workspace.endAction();
		}
	}

	async function runRebase(): Promise<void> {
		const prefix = rebasePrefix.trim();
		const behindText = rebaseBehind.trim();
		if (prefix && behindText) {
			toasts.error(t('domain.files.syncMutual'));
			return;
		}
		workspace.beginAction('sync-rebase');
		try {
			const outcome = await rebaseBaseline({
				mode: rebaseMode,
				branch: rebaseBranch.trim() || null,
				commitPrefix: prefix || null,
				behind: /^\d+$/.test(behindText) ? Number(behindText) : null,
			});
			rebaseOpen = false;
			toasts.success(
				t('domain.files.syncRebased'),
				outcome.newAnchor.slice(0, 10),
			);
			await loadSync();
			store.bump('files');
		} catch (e) {
			console.error('Sync rebase failed:', e);
			syncFailureToast(t('domain.files.syncRebaseFailed'), e);
		} finally {
			workspace.endAction();
		}
	}
</script>

{#if configured}
	<Card
		title={t('domain.files.syncTitle')}
		description={t('domain.files.syncDescription')}
	>
		{#snippet actions()}
			<Button
				variant="ghost"
				size="sm"
				disabled={workspace.busy || loading}
				onclick={() => void loadSync()}
			>
				{t('common.refresh')}
			</Button>
		{/snippet}
		{#if error}
			<p class="text-caption text-destructive">{error}</p>
			<Button variant="ghost" size="sm" onclick={() => void loadSync()}>
				{t('common.retry')}
			</Button>
		{:else if loading && !reconcile && !hint && !defaults}
			<Skeleton lines={4} />
		{:else}
			<div class="space-y-2">
				<div class="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-caption">
					<span class="text-muted-foreground"
						>{t('domain.files.syncReconcile')}</span
					>
					{#if reconcile}
						<span class="font-mono">{reconcile.internalMain.slice(0, 10)}</span>
						<span class="text-micro text-muted-foreground">←</span>
						<span class="font-mono"
							>{reconcile.externalCommit.slice(0, 10)}</span
						>
						<span class="shrink-0 text-micro text-muted-foreground">
							{formatRelativeTime(reconcile.updatedAtMs)}
						</span>
					{:else}
						<span class="text-muted-foreground"
							>{t('domain.files.syncNoReconcile')}</span
						>
					{/if}
				</div>
				<div class="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-caption">
					<span class="text-muted-foreground">{t('domain.files.syncHint')}</span
					>
					{#if hint}
						<Badge variant="warning"
							>{t('domain.files.syncExternalAhead')}</Badge
						>
						<span class="font-mono">{hint.externalHead.slice(0, 10)}</span>
						<span class="text-micro text-muted-foreground">
							{t('domain.files.showingFiles', { total: hint.fileCount })}
						</span>
					{:else}
						<span class="text-muted-foreground"
							>{t('domain.files.syncAligned')}</span
						>
					{/if}
				</div>
				<div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-caption">
					<span class="text-muted-foreground"
						>{t('domain.files.syncDefaults')}</span
					>
					{#if defaults}
						<Switch
							label={t('domain.files.syncAutoCommit')}
							checked={defaults.autoCommit}
							disabled
						/>
						<Switch
							label={t('domain.files.syncAutoImport')}
							checked={defaults.autoImport}
							disabled
						/>
					{/if}
				</div>
			</div>
			<div class="mt-2 flex flex-wrap gap-2">
				<Button
					variant="outline"
					size="sm"
					disabled={workspace.busy || store.busy}
					onclick={openExport}
				>
					{t('domain.files.syncExport')}
				</Button>
				<Button
					variant={hint ? 'default' : 'outline'}
					size="sm"
					disabled={workspace.busy || store.busy}
					onclick={() => void runImport()}
				>
					{t('domain.files.syncImport')}
				</Button>
				<Button
					variant="outline"
					size="sm"
					disabled={workspace.busy || store.busy}
					onclick={() => (rebaseOpen = true)}
				>
					{t('domain.files.syncRebase')}
				</Button>
			</div>
		{/if}
	</Card>

	<Dialog
		bind:open={exportOpen}
		title={t('domain.files.syncExportTitle')}
		description={t('domain.files.syncExportDescription')}
		onclose={() => (exportOpen = false)}
	>
		<div class="space-y-2">
			<label class="flex items-center gap-1.5 text-caption">
				<input
					type="checkbox"
					checked={exportConfirm}
					onchange={(event: Event) => {
						exportConfirm = (event.target as HTMLInputElement).checked;
					}}
					class="accent-primary"
				/>
				<span>{t('domain.files.syncExportConfirmLabel')}</span>
			</label>
			<Switch
				label={t('domain.files.syncAutoCommit')}
				bind:checked={exportAutoCommit}
			/>
			<Input
				label={t('domain.files.syncExportMessage')}
				size="sm"
				value={exportMessage}
				oninput={(event: Event) =>
					(exportMessage = (event.target as HTMLInputElement).value)}
			/>
		</div>
		{#snippet footer()}
			<Button variant="ghost" size="sm" onclick={() => (exportOpen = false)}>
				{t('common.cancel')}
			</Button>
			<Button
				size="sm"
				disabled={workspace.busy || !exportConfirm}
				onclick={() => void runExport()}
			>
				{t('domain.files.syncExport')}
			</Button>
		{/snippet}
	</Dialog>

	<Dialog
		bind:open={rebaseOpen}
		title={t('domain.files.syncRebaseTitle')}
		description={t('domain.files.syncRebaseDescription')}
		onclose={() => (rebaseOpen = false)}
	>
		<p class="mb-2 text-caption text-destructive">
			{t('domain.files.syncRebaseWarning')}
		</p>
		<div class="space-y-2">
			<Input
				label={t('domain.files.syncBranch')}
				size="sm"
				value={rebaseBranch}
				oninput={(event: Event) =>
					(rebaseBranch = (event.target as HTMLInputElement).value)}
			/>
			<Input
				label={t('domain.files.syncPrefix')}
				size="sm"
				value={rebasePrefix}
				oninput={(event: Event) =>
					(rebasePrefix = (event.target as HTMLInputElement).value)}
			/>
			<Input
				label={t('domain.files.syncBehind')}
				size="sm"
				value={rebaseBehind}
				oninput={(event: Event) =>
					(rebaseBehind = (event.target as HTMLInputElement).value)}
			/>
			<Select
				label={t('domain.files.syncMode')}
				size="sm"
				value={rebaseMode}
				options={modeOptions}
				onchange={(value) => (rebaseMode = value)}
			/>
		</div>
		{#snippet footer()}
			<Button variant="ghost" size="sm" onclick={() => (rebaseOpen = false)}>
				{t('common.cancel')}
			</Button>
			<Button
				size="sm"
				disabled={workspace.busy}
				onclick={() => void runRebase()}
			>
				{t('domain.files.syncRebase')}
			</Button>
		{/snippet}
	</Dialog>
{/if}
