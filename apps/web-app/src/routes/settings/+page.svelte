<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate, goto } from '$app/navigation';
	import type { ResolvedPathname } from '$app/types';
	import { page } from '$app/state';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import Switch from '#lib/ui/components/Switch.svelte';
	import Select from '#lib/ui/components/Select.svelte';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';
	import ShortcutsSettings from '#lib/components/layout/ShortcutsSettings.svelte';
	import UnsavedChangesDialog from '#lib/ui/components/UnsavedChangesDialog.svelte';
	import {
		preferences,
		type AutoTitleMode,
		type ChatExecutionPreference,
		type Density,
		type ThemeMode,
	} from '#lib/stores/preferences.svelte';
	import { EDITOR_FONT_SIZE_MIN, EDITOR_FONT_SIZE_MAX } from '#lib/stores/preferences.svelte';
	import {
		describeToolOutputBudgets,
		resolveToolOutputBudgets,
		type ToolOutputDetail,
	} from '#lib/config/tool-output';
	import { behavior } from '#lib/stores/behavior.svelte';
	import { toasts } from '#lib/stores/toast.svelte';
	import { shortcutOverrides } from '#lib/shortcuts/registry';
	import { saveShortcutOverrides } from '#lib/shortcuts/settings';
	import {
		backupFilename,
		describeBackup,
		parseBackupDocument,
		serializeBackupDocument,
		type SettingsBackup,
	} from '#lib/services/portability';
	import { cn } from '#lib/ui/cn';
	import {
		fetchServerDirectories,
		onDirectoryChange,
		onWorkspaceLabelChange,
		pinnedDirectory,
		readRecentDirectories,
		resolveDirectory,
		resolveHostPrefix,
		resolveTabLabel,
		resolveTabName,
		setDirectory,
		setHostPrefix,
		setTabName,
		type RecentDirectory,
	} from '#lib/api/client';
	import { t, message } from '#lib/i18n';
	import { gotoWithParams, parseListParams } from '#lib/utils/route';

	const SECTIONS = [
		{
			id: 'appearance',
			label: message('page.settings.appearance'),
			icon: 'sun',
		},
		{
			id: 'shortcuts',
			label: message('page.settings.shortcuts'),
			icon: 'command',
		},
		{
			id: 'execution',
			label: message('page.settings.executionDefaults'),
			icon: 'activity',
		},
		{
			id: 'notifications',
			label: message('page.settings.notifications'),
			icon: 'bell',
		},
		{
			id: 'workspace',
			label: message('page.settings.workspace'),
			icon: 'sliders',
		},
		{
			id: 'portability',
			label: message('page.settings.portability'),
			icon: 'archive',
		},
	] as const;

	const requestedSection = parseListParams(page.url as unknown as URL).tab;
	let section = $state<(typeof SECTIONS)[number]['id']>(
		requestedSection === 'shortcuts' ||
			requestedSection === 'execution' ||
			requestedSection === 'notifications' ||
			requestedSection === 'workspace' ||
			requestedSection === 'portability'
			? requestedSection
			: 'appearance',
	);

	$effect(() => {
		gotoWithParams(page.url as unknown as URL, {
			tab: section === 'appearance' ? '' : section,
		});
	});

	const THEME_OPTIONS = [
		{ value: 'light', label: message('page.settings.themeLight') },
		{ value: 'dark', label: message('page.settings.themeDark') },
		{ value: 'system', label: message('page.settings.themeSystem') },
	];

	const DENSITY_OPTIONS = [
		{ value: 'compact', label: message('page.settings.densityCompact') },
		{ value: 'default', label: message('page.settings.densityDefault') },
		{
			value: 'comfortable',
			label: message('page.settings.densityComfortable'),
		},
	];

	const TOOL_DETAIL_OPTIONS = [
		{ value: 'compact', label: message('page.settings.detailCompact') },
		{ value: 'balanced', label: message('page.settings.detailBalanced') },
		{ value: 'verbose', label: message('page.settings.detailVerbose') },
	];

	const TITLE_MODE_OPTIONS = [
		{ value: 'off', label: message('page.settings.autoTitleOff') },
		{ value: 'truncate', label: message('page.settings.autoTitleTruncate') },
		{ value: 'llm', label: message('page.settings.autoTitleLlm') },
	];

	const CHAT_EXECUTION_OPTIONS = [
		{ value: 'agent', label: message('page.settings.chatExecutionAgent') },
		{ value: 'workflow', label: message('page.settings.chatExecutionWorkflow') },
		{ value: 'chat', label: message('page.settings.chatExecutionChat') },
	];

	const toolBudgetsHint = $derived(
		describeToolOutputBudgets(resolveToolOutputBudgets(preferences.toolOutputDetail)),
	);

	const PAGE_SIZE_OPTIONS = [
		{ value: '25', label: '25' },
		{ value: '50', label: '50' },
		{ value: '100', label: '100' },
	];

	let pageSize = $derived(String(behavior.pageSize));

	// Only the execution and notification sections persist in the server
	// cross-device behavior track; appearance and workspace are the local
	// first-paint track and apply instantly in this browser.
	const serverSection = $derived(section === 'execution' || section === 'notifications');
	// Server controls stay disabled until the backend document arrives.
	const serverUnreachable = $derived(!behavior.loaded && behavior.error !== null);

	let unsavedOpen = $state(false);
	let unsavedBusy = $state(false);
	let pendingNavUrl = $state<string | null>(null);

	let dirInput = $state('');
	let dirError = $state<string | null>(null);
	let dirSaving = $state(false);
	let dirAllowedRoots = $state<string[] | null>(null);
	let dirKnown = $state(false);
	let dirPinned = $state<string | undefined>(undefined);
	let recentDirs = $state<RecentDirectory[]>([]);
	let tabInput = $state('');
	let tabPreview = $state('');
	let hostInput = $state('');

	async function loadServerDirectories(): Promise<void> {
		const state = await fetchServerDirectories();
		if (!state) return;
		dirKnown = true;
		dirAllowedRoots = state.allowedRoots;
	}

	function refreshWorkspaceInputs(): void {
		dirInput = resolveDirectory() ?? '';
		tabInput = resolveTabName() ?? '';
		tabPreview = resolveTabLabel();
		hostInput = resolveHostPrefix() ?? '';
		recentDirs = readRecentDirectories();
	}

	// Switching applies in place: the layout remounts every page under the
	// new directory instead of reloading the browser tab, so other tabs
	// pinned to different workspaces are unaffected.
	function saveDirectory(): void {
		dirError = null;
		dirSaving = true;
		try {
			setDirectory(dirInput.trim() ? dirInput : undefined);
			refreshWorkspaceInputs();
		} catch (error) {
			dirError = error instanceof Error ? error.message : String(error);
		} finally {
			dirSaving = false;
		}
	}

	function clearDirectory(): void {
		dirError = null;
		try {
			setDirectory(undefined);
		} catch (error) {
			dirError = error instanceof Error ? error.message : String(error);
		}
		refreshWorkspaceInputs();
	}

	function switchRecent(directory: string): void {
		dirError = null;
		try {
			setDirectory(directory);
		} catch (error) {
			dirError = error instanceof Error ? error.message : String(error);
		}
		refreshWorkspaceInputs();
	}

	function saveTabName(): void {
		dirError = null;
		try {
			setTabName(tabInput.trim() ? tabInput : undefined);
		} catch (error) {
			dirError = error instanceof Error ? error.message : String(error);
		}
		refreshWorkspaceInputs();
	}

	function saveHostPrefix(): void {
		dirError = null;
		try {
			setHostPrefix(hostInput.trim() ? hostInput : undefined);
		} catch (error) {
			dirError = error instanceof Error ? error.message : String(error);
		}
		refreshWorkspaceInputs();
	}

	onMount(() => {
		void behavior.load();
		dirPinned = pinnedDirectory();
		refreshWorkspaceInputs();
		void loadServerDirectories();
		const offDirectory = onDirectoryChange(() => refreshWorkspaceInputs());
		const offLabel = onWorkspaceLabelChange(() => refreshWorkspaceInputs());
		const guardUnload = (event: BeforeUnloadEvent): void => {
			if (behavior.dirty) event.preventDefault();
		};
		window.addEventListener('beforeunload', guardUnload);
		return () => {
			window.removeEventListener('beforeunload', guardUnload);
			offDirectory();
			offLabel();
		};
	});

	beforeNavigate((navigation) => {
		if (navigation.willUnload) return;
		if (!behavior.dirty) return;
		navigation.cancel();
		pendingNavUrl = navigation.to?.url.toString() ?? null;
		unsavedOpen = true;
	});

	async function saveBehavior(): Promise<void> {
		await behavior.save();
		if (behavior.error) {
			toasts.error(t('page.settings.prefsSaveFailed'), behavior.error);
		} else {
			toasts.success(t('page.settings.prefsSaved'));
		}
	}

	async function restoreBehavior(): Promise<void> {
		await behavior.restoreDefaults();
		if (behavior.error) {
			toasts.error(t('page.settings.restoreFailed'), behavior.error);
		} else {
			toasts.success(t('page.settings.defaultsRestored'));
		}
	}

	function proceedPending(): void {
		const target = pendingNavUrl;
		pendingNavUrl = null;
		// The target replays an intercepted navigation URL, which already carries the base.
		if (target) void goto(target as ResolvedPathname);
	}

	async function saveAndProceed(): Promise<void> {
		unsavedBusy = true;
		try {
			await saveBehavior();
			if (behavior.error) return;
			unsavedOpen = false;
			proceedPending();
		} finally {
			unsavedBusy = false;
		}
	}

	async function discardAndProceed(): Promise<void> {
		unsavedBusy = true;
		try {
			await behavior.load();
			unsavedOpen = false;
			proceedPending();
		} finally {
			unsavedBusy = false;
		}
	}

	let pendingBackup = $state<SettingsBackup | null>(null);
	let backupError = $state<string | null>(null);
	let backupMessage = $state('');
	let backupBusy = $state(false);

	function exportBackup(): void {
		const backup: SettingsBackup = {
			version: 1,
			preferences: {
				theme: preferences.theme,
				density: preferences.density,
				sidebarCollapsed: preferences.sidebarCollapsed,
				sidebarWidth: preferences.sidebarWidth,
				inspectorWidth: preferences.inspectorWidth,
				inspectorPinned: preferences.inspectorPinned,
				minimapMode: preferences.minimapMode,
				toolOutputPreviews: preferences.toolOutputPreviews,
				toolOutputDetail: preferences.toolOutputDetail,
				chatExecutionType: preferences.chatExecutionType,
			},
			shortcuts: shortcutOverrides(),
			behavior: {
				pageSize: behavior.pageSize,
				streamFollow: behavior.streamFollow,
				reduceMotion: behavior.reduceMotion,
			},
		};
		const blob = new Blob([serializeBackupDocument(backup)], {
			type: 'application/json',
		});
		const url = URL.createObjectURL(blob);
		const anchor = document.createElement('a');
		anchor.href = url;
		anchor.download = backupFilename();
		document.body.appendChild(anchor);
		anchor.click();
		anchor.remove();
		URL.revokeObjectURL(url);
		backupMessage = t('page.settings.backupDownloaded');
		backupError = null;
	}

	async function onBackupFileSelected(
		event: Event & { currentTarget: HTMLInputElement },
	): Promise<void> {
		const file = event.currentTarget.files?.[0] ?? null;
		pendingBackup = null;
		backupError = null;
		backupMessage = '';
		if (!file) return;
		try {
			const text = await file.text();
			pendingBackup = parseBackupDocument(text);
			backupMessage = t('page.settings.readyToRestore', { backup: describeBackup(pendingBackup) });
		} catch (error) {
			backupError = error instanceof Error ? error.message : t('page.settings.invalidBackup');
		} finally {
			event.currentTarget.value = '';
		}
	}

	function applyLocalBackup(): void {
		if (!pendingBackup) return;
		preferences.applyBackup(pendingBackup.preferences);
		saveShortcutOverrides({ ...pendingBackup.shortcuts });
		backupMessage = t('page.settings.localRestored');
		backupError = null;
		pendingBackup = null;
		toasts.success(t('page.settings.localSettingsRestored'));
	}

	async function applyFullBackup(): Promise<void> {
		if (!pendingBackup) return;
		backupBusy = true;
		try {
			preferences.applyBackup(pendingBackup.preferences);
			saveShortcutOverrides({ ...pendingBackup.shortcuts });
			behavior.applyBackup(pendingBackup.behavior);
			await behavior.save();
			if (behavior.error) {
				backupError = `Server part failed: ${behavior.error}. Local part is restored; retry Save later.`;
				return;
			}
			backupError = null;
			backupMessage = t('page.settings.backupFullyRestored');
			pendingBackup = null;
			toasts.success(t('page.settings.settingsRestored'));
		} finally {
			backupBusy = false;
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader title={t('page.settings.title')} description={t('page.settings.description')}>
		{#snippet actions()}
			{#if serverSection}
				{#if behavior.dirty}
					<span
						class="rounded border border-warning/40 bg-warning/10 px-1.5 py-0.5 text-micro text-warning"
						>{t('page.settings.unsaved')}</span
					>
				{/if}
				<Button
					variant="outline"
					size="sm"
					disabled={behavior.saving || serverUnreachable}
					onclick={() => void restoreBehavior()}
				>
					<Icon name="history" size={13} />
					{t('page.settings.restoreDefaults')}
				</Button>
				<Button
					size="sm"
					disabled={behavior.saving || serverUnreachable}
					onclick={() => void saveBehavior()}
				>
					<Icon name="check" size={13} />
					{behavior.saving ? t('page.settings.saving') : t('page.settings.saveServerPrefs')}
				</Button>
			{:else}
				<span class="text-caption text-muted-foreground">
					{t('page.settings.localPrefsApplyInstantly')}
				</span>
			{/if}
		{/snippet}
	</PageHeader>

	<div
		class="grid min-h-0 flex-1 gap-3 overflow-hidden px-4 pb-4 lg:grid-cols-[14rem_minmax(0,1fr)]"
	>
		<nav class="space-y-1 overflow-y-auto">
			{#each SECTIONS as item (item.id)}
				<button
					type="button"
					onclick={() => (section = item.id)}
					class={cn(
						'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-body transition-colors',
						section === item.id
							? 'bg-accent font-medium text-accent-foreground'
							: 'text-muted-foreground hover:bg-accent/60 hover:text-foreground',
					)}
				>
					<Icon name={item.icon} size={15} />
					<span class="truncate">{item.label()}</span>
				</button>
			{/each}
		</nav>

		<div class="min-h-0 overflow-y-auto pb-2">
			{#if section === 'appearance'}
				<div class="space-y-3">
					<Card title={t('page.settings.theme')} description={t('page.settings.themeDesc')}>
						<Select
							value={preferences.theme}
							options={THEME_OPTIONS}
							placeholder={t('page.settings.theme')}
							class="w-56"
							onchange={(value) => preferences.setTheme(value as ThemeMode)}
						/>
						<div class="mt-3 flex flex-wrap gap-2">
							{#each THEME_OPTIONS as option (option.value)}
								<Button
									variant={preferences.theme === option.value ? 'secondary' : 'outline'}
									size="sm"
									onclick={() => preferences.setTheme(option.value as ThemeMode)}
								>
									<Icon
										name={option.value === 'light'
											? 'sun'
											: option.value === 'dark'
												? 'moon'
												: 'monitor'}
										size={13}
									/>
									{option.label()}
								</Button>
							{/each}
						</div>
					</Card>

					<Card
						title={t('page.settings.typeDensity')}
						description={t('page.settings.typeDensityDesc')}
					>
						<Select
							value={preferences.density}
							options={DENSITY_OPTIONS}
							placeholder={t('page.settings.density')}
							class="w-56"
							onchange={(value) => preferences.setDensity(value as Density)}
						/>
						<p class="mt-2 text-caption text-muted-foreground">
							Type {preferences.fontScale.toFixed(2)}× · corners
							{preferences.spacingScale.toFixed(2)}×
						</p>
					</Card>

					<Card
						title={t('page.settings.toolOutput')}
						description={t('page.settings.toolOutputDesc')}
					>
						<Switch
							checked={preferences.toolOutputPreviews}
							label={t('page.settings.summarizeToolOutputs')}
							onchange={() => preferences.setToolOutputPreviews(!preferences.toolOutputPreviews)}
						/>
						<div class="mt-3">
							<Select
								value={preferences.toolOutputDetail}
								options={TOOL_DETAIL_OPTIONS}
								placeholder={t('page.settings.detail')}
								class="w-56"
								disabled={!preferences.toolOutputPreviews}
								onchange={(value) => preferences.setToolOutputDetail(value as ToolOutputDetail)}
							/>
							<p class="mt-2 text-caption text-muted-foreground">
								Preview {toolBudgetsHint}
							</p>
						</div>
					</Card>

					<Card title={t('page.settings.editor')} description={t('page.settings.editorDesc')}>
						<div class="flex items-center gap-3">
							<input
								type="range"
								min={EDITOR_FONT_SIZE_MIN}
								max={EDITOR_FONT_SIZE_MAX}
								step="1"
								value={preferences.editorFontSize}
								oninput={(event) =>
									preferences.setEditorFontSize(
										Number((event.currentTarget as HTMLInputElement).value),
									)}
								class="w-full accent-primary"
							/>
							<span
								class="w-16 shrink-0 text-right text-caption tabular-nums text-muted-foreground"
							>
								{preferences.editorFontSize}px
							</span>
						</div>
						<Switch
							checked={preferences.editorWordWrap}
							label={t('page.settings.editorWordWrap')}
							class="mt-3"
							onchange={() => preferences.setEditorWordWrap(!preferences.editorWordWrap)}
						/>
					</Card>
				</div>
			{:else if section === 'shortcuts'}
				<div class="space-y-3">
					<ShortcutsSettings />
				</div>
			{:else if section === 'portability'}
				<div class="space-y-3">
					<Card title={t('page.settings.backup')} description={t('page.settings.backupDesc')}>
						<Button size="sm" variant="outline" onclick={exportBackup}>
							<Icon name="download" size={13} />
							{t('page.settings.exportSettings')}
						</Button>
						{#if backupMessage && !pendingBackup}
							<p class="mt-2 text-caption text-muted-foreground">
								{backupMessage}
							</p>
						{/if}
					</Card>
					<Card title={t('page.settings.restore')} description={t('page.settings.restoreDesc')}>
						<input
							type="file"
							accept="application/json,.json"
							onchange={(event) => void onBackupFileSelected(event)}
							class="text-body"
						/>
						{#if backupError}
							<p class="mt-2 text-caption text-destructive">{backupError}</p>
						{/if}
						{#if pendingBackup}
							<p class="mt-2 text-caption text-muted-foreground">
								{t('page.settings.willReplace', { backup: describeBackup(pendingBackup) })}.
							</p>
							{#if serverUnreachable}
								<p class="mt-2 text-caption text-warning">
									{t('page.settings.localOnlyNote')}
								</p>
							{/if}
							<div class="mt-3 flex flex-wrap gap-2">
								<Button size="sm" variant="outline" onclick={applyLocalBackup}>
									{t('page.settings.restoreLocalOnly')}
								</Button>
								<Button
									size="sm"
									disabled={backupBusy || serverUnreachable}
									onclick={() => void applyFullBackup()}
								>
									<Icon name="upload" size={13} />
									{backupBusy ? t('page.settings.restoring') : t('page.settings.restoreAll')}
								</Button>
								<Button
									size="sm"
									variant="ghost"
									onclick={() => {
										pendingBackup = null;
										backupMessage = '';
									}}
								>
									{t('common.cancel')}
								</Button>
							</div>
						{:else if backupMessage}
							<p class="mt-2 text-caption text-muted-foreground">
								{backupMessage}
							</p>
						{/if}
					</Card>
				</div>
			{:else if serverSection}
				<PageState
					loading={behavior.loading}
					error={serverUnreachable ? behavior.error : null}
					errorTitle={t('page.settings.serverPrefsUnavailable')}
					onretry={() => void behavior.load()}
				>
					{#if section === 'execution'}
						<div class="space-y-3">
							<p class="text-caption text-muted-foreground">
								{t('page.settings.serverSectionNote')}
							</p>
							{#if behavior.loaded && behavior.error}
								<p class="text-caption text-destructive">
									{t('page.settings.serverUpdateFailed', {
										error: behavior.error ?? '',
									})}
								</p>
							{/if}
							<Card title={t('page.settings.paging')} description={t('page.settings.pagingDesc')}>
								<Select
									value={pageSize}
									options={PAGE_SIZE_OPTIONS}
									placeholder={t('page.settings.pageSize')}
									class="w-40"
									disabled={serverUnreachable}
									onchange={(value) => {
										behavior.pageSize = Number(value) || 50;
									}}
								/>
							</Card>
							<Card
								title={t('page.settings.liveUpdates')}
								description={t('page.settings.storedServerSide')}
							>
								<Switch
									checked={behavior.streamFollow}
									label={t('page.settings.followStream')}
									class="mt-2"
									disabled={serverUnreachable}
									onchange={() => {
										behavior.streamFollow = !behavior.streamFollow;
									}}
								/>
							</Card>
						</div>
					{:else}
						<div class="space-y-3">
							<p class="text-caption text-muted-foreground">
								{t('page.settings.toastPreviewNote')}
							</p>
							<Card title={t('page.settings.toasts')}>
								<div class="mt-1 flex gap-2">
									<Button
										variant="outline"
										size="sm"
										onclick={() =>
											toasts.success(
												t('page.settings.saved'),
												t('page.settings.preferenceApplied'),
											)}
									>
										{t('page.settings.testSuccessToast')}
									</Button>
									<Button
										variant="outline"
										size="sm"
										onclick={() =>
											toasts.error(
												t('page.settings.blocked'),
												t('page.settings.retryMayBeRequired'),
											)}
									>
										{t('page.settings.testErrorToast')}
									</Button>
								</div>
							</Card>
							<Card
								title={t('page.settings.accessibility')}
								description={t('page.settings.storedServerSide')}
							>
								<Switch
									checked={behavior.reduceMotion}
									label={t('page.settings.reduceMotion')}
									disabled={serverUnreachable}
									onchange={() => {
										behavior.reduceMotion = !behavior.reduceMotion;
									}}
								/>
								<p class="mt-2 text-caption text-muted-foreground">
									{t('page.settings.reduceMotionNote')}
								</p>
							</Card>
						</div>
					{/if}
				</PageState>
			{:else}
				<div class="space-y-3">
					<Card
						title={t('page.settings.serverDirectory')}
						description={t('page.settings.serverDirectoryDesc')}
					>
						<label for="server-directory" class="block text-caption text-muted-foreground">
							{t('page.settings.serverDirectory')}
						</label>
						<input
							id="server-directory"
							type="text"
							bind:value={dirInput}
							placeholder={t('page.settings.serverDirectoryPlaceholder')}
							class="mt-1 w-full max-w-md rounded-md border border-input bg-card px-2.5 py-1.5 text-body text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
						/>
						{#if dirError}
							<p class="mt-2 text-caption text-destructive">{dirError}</p>
						{/if}
						{#if dirPinned}
							<p class="mt-2 text-caption text-warning">
								{t('page.settings.serverDirectoryPinned')}
							</p>
						{/if}
						<p class="mt-2 text-caption text-muted-foreground">
							{#if dirKnown}
								{#if dirAllowedRoots !== null && dirAllowedRoots.length > 0}
									{t('page.settings.serverDirectoryAllowed', {
										roots: dirAllowedRoots.join(', '),
									})}
								{:else}
									{t('page.settings.serverDirectoryAllowAny')}
								{/if}
							{:else}
								{t('page.settings.serverDirectoryUnavailable')}
							{/if}
						</p>
						<div class="mt-3 flex gap-2">
							<Button
								size="sm"
								disabled={dirSaving || dirPinned !== undefined}
								onclick={saveDirectory}
							>
								{t('common.save')}
							</Button>
							<Button
								size="sm"
								variant="outline"
								disabled={dirPinned !== undefined}
								onclick={clearDirectory}
							>
								{t('page.settings.serverDirectoryClear')}
							</Button>
						</div>
						{#if recentDirs.length > 0}
							<p class="mt-4 block text-caption text-muted-foreground">
								{t('page.settings.recentDirectories')}
							</p>
							<div class="mt-1 flex max-w-md flex-col gap-1">
								{#each recentDirs as recent (recent.directory)}
									<button
										type="button"
										disabled={dirPinned !== undefined}
										onclick={() => switchRecent(recent.directory)}
										class="truncate rounded-md border border-border bg-muted/30 px-2.5 py-1.5 text-left text-body text-foreground transition-colors hover:bg-accent disabled:cursor-not-allowed disabled:opacity-60"
									>
										{recent.directory}
									</button>
								{/each}
							</div>
						{/if}
						<label for="tab-label" class="mt-4 block text-caption text-muted-foreground">
							{t('page.settings.tabLabel')}
						</label>
						<input
							id="tab-label"
							type="text"
							bind:value={tabInput}
							placeholder={t('page.settings.tabLabelPlaceholder')}
							class="mt-1 w-full max-w-md rounded-md border border-input bg-card px-2.5 py-1.5 text-body text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
						/>
						<p class="mt-2 text-caption text-muted-foreground">
							{t('page.settings.tabLabelDesc')}
							{#if tabPreview}
								<span class="text-foreground"> {tabPreview}</span>
							{/if}
						</p>
						<div class="mt-3 flex gap-2">
							<Button size="sm" variant="outline" onclick={saveTabName}>
								{t('common.save')}
							</Button>
						</div>
						<label for="host-prefix" class="mt-4 block text-caption text-muted-foreground">
							{t('page.settings.hostPrefix')}
						</label>
						<input
							id="host-prefix"
							type="text"
							bind:value={hostInput}
							placeholder={t('page.settings.hostPrefixPlaceholder')}
							class="mt-1 w-full max-w-md rounded-md border border-input bg-card px-2.5 py-1.5 text-body text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
						/>
						<p class="mt-2 text-caption text-muted-foreground">
							{t('page.settings.hostPrefixDesc')}
						</p>
						<div class="mt-3 flex gap-2">
							<Button size="sm" variant="outline" onclick={saveHostPrefix}>
								{t('common.save')}
							</Button>
						</div>
					</Card>
					<Card title={t('page.settings.shell')}>
						<Switch
							checked={preferences.sidebarCollapsed}
							label={t('page.settings.collapseSidebar')}
							onchange={() => preferences.toggleSidebar()}
						/>
						<Switch
							checked={preferences.inspectorPinned}
							label={t('page.settings.keepInspectorDocked')}
							class="mt-2"
							onchange={() => preferences.toggleInspectorPinned()}
						/>
					</Card>
					<Card
						title={t('page.settings.sessionTitles')}
						description={t('page.settings.sessionTitlesDesc')}
					>
						<Select
							value={preferences.autoTitleMode}
							options={TITLE_MODE_OPTIONS}
							placeholder={t('page.settings.autoTitleMode')}
							class="w-56"
							onchange={(value) => preferences.setAutoTitleMode(value as AutoTitleMode)}
						/>
						<div class="mt-3">
							<label for="title-model" class="block text-caption text-muted-foreground">
								{t('page.settings.titleModel')}
							</label>
							<input
								id="title-model"
								type="text"
								value={preferences.titleModel}
								placeholder={t('page.settings.titleModelPlaceholder')}
								disabled={preferences.autoTitleMode !== 'llm'}
								oninput={(event) =>
									preferences.setTitleModel((event.currentTarget as HTMLInputElement).value)}
								class="mt-1 w-56 rounded-md border border-input bg-card px-2.5 py-1.5 text-body text-foreground placeholder:text-muted-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring disabled:cursor-not-allowed disabled:opacity-60"
							/>
							<p class="mt-2 text-caption text-muted-foreground">
								{t('page.settings.titleModelDesc')}
							</p>
						</div>
					</Card>
					<Card
						title={t('page.settings.chatDefaults')}
						description={t('page.settings.chatDefaultsDesc')}
					>
						<Select
							value={preferences.chatExecutionType}
							options={CHAT_EXECUTION_OPTIONS}
							placeholder={t('page.settings.chatExecutionType')}
							class="w-56"
							onchange={(value) =>
								preferences.setChatExecutionType(value as ChatExecutionPreference)}
						/>
					</Card>
					<Card
						title={t('page.settings.sidebarWidth')}
						description={t('page.settings.persistentNote')}
					>
						<div class="flex items-center gap-3">
							<input
								type="range"
								min="200"
								max="360"
								step="8"
								value={preferences.sidebarWidth}
								oninput={(event) =>
									preferences.setSidebarWidth(
										Number((event.currentTarget as HTMLInputElement).value),
									)}
								class="w-full accent-primary"
							/>
							<span
								class="w-12 shrink-0 text-right text-caption tabular-nums text-muted-foreground"
							>
								{preferences.sidebarWidth}px
							</span>
						</div>
					</Card>
				</div>
			{/if}
		</div>
	</div>
</div>

<UnsavedChangesDialog
	bind:open={unsavedOpen}
	description={t('page.settings.unsavedDialogDesc')}
	saveLabel={t('page.settings.saveLeave')}
	busy={unsavedBusy}
	ondiscard={() => void discardAndProceed()}
	onsave={() => void saveAndProceed()}
/>
