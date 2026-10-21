<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate, goto } from '$app/navigation';
	import { page } from '$app/state';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Switch from '@wf-agent/ui/components/Switch.svelte';
	import Select from '@wf-agent/ui/components/Select.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import PageState from '$lib/components/layout/PageState.svelte';
	import ShortcutsSettings from '$lib/components/layout/ShortcutsSettings.svelte';
	import UnsavedChangesDialog from '@wf-agent/ui/components/UnsavedChangesDialog.svelte';
	import {
		preferences,
		type Density,
		type ThemeMode,
	} from '$lib/stores/preferences.svelte';
	import {
		describeToolOutputBudgets,
		resolveToolOutputBudgets,
		type ToolOutputDetail,
	} from '$lib/config/tool-output';
	import { behavior } from '$lib/stores/behavior.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import { shortcutOverrides } from '$lib/shortcuts/registry';
	import { saveShortcutOverrides } from '$lib/shortcuts/settings';
	import {
		backupFilename,
		describeBackup,
		parseBackupDocument,
		serializeBackupDocument,
		type SettingsBackup,
	} from '$lib/services/portability';
	import { cn } from '@wf-agent/ui/cn';
import { t, message } from '$lib/i18n';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';

	const SECTIONS = [
		{ id: 'appearance', label: message('page.settings.appearance'), icon: 'sun' },
		{ id: 'shortcuts', label: message('page.settings.shortcuts'), icon: 'command' },
		{ id: 'execution', label: message('page.settings.executionDefaults'), icon: 'activity' },
		{ id: 'notifications', label: message('page.settings.notifications'), icon: 'bell' },
		{ id: 'workspace', label: message('page.settings.workspace'), icon: 'sliders' },
		{ id: 'portability', label: message('page.settings.portability'), icon: 'archive' },
	] as const;

	const requestedSection = parseListParams(page.url).tab;
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
		gotoWithParams(page.url, { tab: section === 'appearance' ? '' : section });
	});

	const THEME_OPTIONS = [
		{ value: 'light', label: message('page.settings.themeLight') },
		{ value: 'dark', label: message('page.settings.themeDark') },
		{ value: 'system', label: message('page.settings.themeSystem') },
	];

	const DENSITY_OPTIONS = [
		{ value: 'compact', label: message('page.settings.densityCompact') },
		{ value: 'default', label: message('page.settings.densityDefault') },
		{ value: 'comfortable', label: message('page.settings.densityComfortable') },
	];

	const TOOL_DETAIL_OPTIONS = [
		{ value: 'compact', label: message('page.settings.detailCompact') },
		{ value: 'balanced', label: message('page.settings.detailBalanced') },
		{ value: 'verbose', label: message('page.settings.detailVerbose') },
	];

	const toolBudgetsHint = $derived(
		describeToolOutputBudgets(
			resolveToolOutputBudgets(preferences.toolOutputDetail),
		),
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
	const serverSection = $derived(
		section === 'execution' || section === 'notifications',
	);
	// Server controls stay disabled until the backend document arrives.
	const serverUnreachable = $derived(
		!behavior.loaded && behavior.error !== null,
	);

	let unsavedOpen = $state(false);
	let unsavedBusy = $state(false);
	let pendingNavUrl = $state<string | null>(null);

	onMount(() => {
		void behavior.load();
		const guardUnload = (event: BeforeUnloadEvent): void => {
			if (behavior.dirty) event.preventDefault();
		};
		window.addEventListener('beforeunload', guardUnload);
		return () => window.removeEventListener('beforeunload', guardUnload);
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
		// The target replays an intercepted navigation URL, which resolve() cannot rebuild.
		// eslint-disable-next-line svelte/no-navigation-without-resolve
		if (target) void goto(target);
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
			},
			shortcuts: shortcutOverrides(),
			behavior: {
				pageSize: behavior.pageSize,
				autoRefresh: behavior.autoRefresh,
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
			backupMessage = `Ready to restore: ${describeBackup(pendingBackup)}.`;
		} catch (error) {
			backupError =
				error instanceof Error ? error.message : t('page.settings.invalidBackup');
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
	<PageHeader
		title={t('page.settings.title')}
		description={t('page.settings.description')}
	>
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
					Local preferences apply instantly.
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
					<Card
						title={t('page.settings.theme')}
						description={t('page.settings.themeDesc')}
					>
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
									variant={preferences.theme === option.value
										? 'secondary'
										: 'outline'}
									size="sm"
									onclick={() =>
										preferences.setTheme(option.value as ThemeMode)}
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
							onchange={() =>
								preferences.setToolOutputPreviews(
									!preferences.toolOutputPreviews,
								)}
						/>
						<div class="mt-3">
							<Select
								value={preferences.toolOutputDetail}
								options={TOOL_DETAIL_OPTIONS}
								placeholder={t('page.settings.detail')}
								class="w-56"
								disabled={!preferences.toolOutputPreviews}
								onchange={(value) =>
									preferences.setToolOutputDetail(value as ToolOutputDetail)}
							/>
							<p class="mt-2 text-caption text-muted-foreground">
								Preview {toolBudgetsHint}
							</p>
						</div>
					</Card>
				</div>
			{:else if section === 'shortcuts'}
				<div class="space-y-3">
					<ShortcutsSettings />
				</div>
			{:else if section === 'portability'}
				<div class="space-y-3">
					<Card
						title={t('page.settings.backup')}
						description={t('page.settings.backupDesc')}
					>
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
					<Card
						title={t('page.settings.restore')}
						description={t('page.settings.restoreDesc')}
					>
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
								Will replace: {describeBackup(pendingBackup)}.
							</p>
							{#if serverUnreachable}
								<p class="mt-2 text-caption text-warning">
									Server is unreachable, so only the local part can be restored
									now. Retry the server part later.
								</p>
							{/if}
							<div class="mt-3 flex flex-wrap gap-2">
								<Button size="sm" variant="outline" onclick={applyLocalBackup}>
									Restore local only
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
									Cancel
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
								Server-side section. Changes only take effect after Save in the
								header.
							</p>
							{#if behavior.loaded && behavior.error}
								<p class="text-caption text-destructive">
									{t('page.settings.serverUpdateFailed', { error: behavior.error ?? '' })}
								</p>
							{/if}
							<Card
								title={t('page.settings.paging')}
								description={t('page.settings.pagingDesc')}
							>
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
							<Card title={t('page.settings.liveUpdates')} description={t('page.settings.storedServerSide')}>
								<Switch
									checked={behavior.autoRefresh}
									label={t('page.settings.autoRefresh')}
									disabled={serverUnreachable}
									onchange={() => {
										behavior.autoRefresh = !behavior.autoRefresh;
									}}
								/>
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
											toasts.success(t('page.settings.saved'), t('page.settings.preferenceApplied'))}
									>
										{t('page.settings.testSuccessToast')}
									</Button>
									<Button
										variant="outline"
										size="sm"
										onclick={() =>
											toasts.error(t('page.settings.blocked'), t('page.settings.retryMayBeRequired'))}
									>
										{t('page.settings.testErrorToast')}
									</Button>
								</div>
							</Card>
							<Card title={t('page.settings.accessibility')} description={t('page.settings.storedServerSide')}>
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
					<Card title={t('page.settings.sidebarWidth')} description={t('page.settings.persistentNote')}>
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
								class="w-full accent-[hsl(var(--primary))]"
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
