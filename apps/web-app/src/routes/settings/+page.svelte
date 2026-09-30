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
	import UnsavedChangesDialog from '@wf-agent/ui/components/UnsavedChangesDialog.svelte';
	import {
		preferences,
		type Density,
		type ThemeMode,
	} from '$lib/stores/preferences.svelte';
	import { behavior } from '$lib/stores/behavior.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import { cn } from '@wf-agent/ui/cn';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';

	const SECTIONS = [
		{ id: 'appearance', label: 'Appearance', icon: 'sun' },
		{ id: 'execution', label: 'Execution defaults', icon: 'activity' },
		{ id: 'notifications', label: 'Notifications', icon: 'bell' },
		{ id: 'workspace', label: 'Workspace', icon: 'sliders' },
	] as const;

	const requestedSection = parseListParams(page.url).tab;
	let section = $state<(typeof SECTIONS)[number]['id']>(
		requestedSection === 'execution' ||
			requestedSection === 'notifications' ||
			requestedSection === 'workspace'
			? requestedSection
			: 'appearance',
	);

	$effect(() => {
		gotoWithParams(page.url, { tab: section === 'appearance' ? '' : section });
	});

	const THEME_OPTIONS = [
		{ value: 'light', label: 'Light' },
		{ value: 'dark', label: 'Dark' },
		{ value: 'system', label: 'Follow system' },
	];

	const DENSITY_OPTIONS = [
		{ value: 'compact', label: 'Compact' },
		{ value: 'default', label: 'Default' },
		{ value: 'comfortable', label: 'Comfortable' },
	];

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
			toasts.error('Preferences failed to save', behavior.error);
		} else {
			toasts.success('Preferences saved');
		}
	}

	async function restoreBehavior(): Promise<void> {
		await behavior.restoreDefaults();
		if (behavior.error) {
			toasts.error('Restore failed', behavior.error);
		} else {
			toasts.success('Defaults restored');
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
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Settings"
		description="Appearance and workspace stay local and apply instantly. Execution and notification preferences are cross-device server behavior and need Save."
	>
		{#snippet actions()}
			{#if serverSection}
				{#if behavior.dirty}
					<span
						class="rounded border border-warning/40 bg-warning/10 px-1.5 py-0.5 text-micro text-warning"
						>Unsaved changes</span
					>
				{/if}
				<Button
					variant="outline"
					size="sm"
					disabled={behavior.saving || serverUnreachable}
					onclick={() => void restoreBehavior()}
				>
					<Icon name="history" size={13} />
					Restore defaults
				</Button>
				<Button
					size="sm"
					disabled={behavior.saving || serverUnreachable}
					onclick={() => void saveBehavior()}
				>
					<Icon name="check" size={13} />
					{behavior.saving ? 'Saving…' : 'Save server preferences'}
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
					<span class="truncate">{item.label}</span>
				</button>
			{/each}
		</nav>

		<div class="min-h-0 overflow-y-auto pb-2">
			{#if section === 'appearance'}
				<div class="space-y-3">
					<Card
						title="Theme"
						description="Applied before first paint and persisted locally."
					>
						<Select
							value={preferences.theme}
							options={THEME_OPTIONS}
							placeholder="Theme"
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
									{option.label}
								</Button>
							{/each}
						</div>
					</Card>

					<Card
						title="Type density"
						description="Scales interface type and corner roundness without changing layout units."
					>
						<Select
							value={preferences.density}
							options={DENSITY_OPTIONS}
							placeholder="Density"
							class="w-56"
							onchange={(value) => preferences.setDensity(value as Density)}
						/>
						<p class="mt-2 text-caption text-muted-foreground">
							Type {preferences.fontScale.toFixed(2)}× · corners
							{preferences.spacingScale.toFixed(2)}×
						</p>
					</Card>
				</div>
			{:else if serverSection}
				<PageState
					loading={behavior.loading}
					error={serverUnreachable ? behavior.error : null}
					errorTitle="Server preferences unavailable"
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
									Server preferences update failed ({behavior.error}); edits
									stay local until Save succeeds.
								</p>
							{/if}
							<Card
								title="Paging"
								description="Cursor pages have no total; this sets the requested page size. Stored server-side."
							>
								<Select
									value={pageSize}
									options={PAGE_SIZE_OPTIONS}
									placeholder="Page size"
									class="w-40"
									disabled={serverUnreachable}
									onchange={(value) => {
										behavior.pageSize = Number(value) || 50;
									}}
								/>
							</Card>
							<Card title="Live updates" description="Stored server-side.">
								<Switch
									checked={behavior.autoRefresh}
									label="Auto refresh lists"
									disabled={serverUnreachable}
									onchange={() => {
										behavior.autoRefresh = !behavior.autoRefresh;
									}}
								/>
								<Switch
									checked={behavior.streamFollow}
									label="Follow stream tail"
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
								Toast previews apply instantly; the accessibility preference
								below is stored server-side and needs Save.
							</p>
							<Card title="Toasts">
								<div class="mt-1 flex gap-2">
									<Button
										variant="outline"
										size="sm"
										onclick={() =>
											toasts.success('Saved', 'Preference applied')}
									>
										Test success toast
									</Button>
									<Button
										variant="outline"
										size="sm"
										onclick={() =>
											toasts.error('Blocked', 'Retry may be required')}
									>
										Test error toast
									</Button>
								</div>
							</Card>
							<Card title="Accessibility" description="Stored server-side.">
								<Switch
									checked={behavior.reduceMotion}
									label="Always reduce motion"
									disabled={serverUnreachable}
									onchange={() => {
										behavior.reduceMotion = !behavior.reduceMotion;
									}}
								/>
								<p class="mt-2 text-caption text-muted-foreground">
									The OS reduced-motion preference is honoured automatically.
								</p>
							</Card>
						</div>
					{/if}
				</PageState>
			{:else}
				<div class="space-y-3">
					<Card title="Shell">
						<Switch
							checked={preferences.sidebarCollapsed}
							label="Collapse sidebar to icon rail"
							onchange={() => preferences.toggleSidebar()}
						/>
						<Switch
							checked={preferences.inspectorPinned}
							label="Keep inspector docked on wide screens"
							class="mt-2"
							onchange={() => preferences.toggleInspectorPinned()}
						/>
					</Card>
					<Card title="Sidebar width" description="Persisted between sessions.">
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
	description="Server preferences have unsaved edits. Leaving discards the local changes."
	saveLabel="Save & leave"
	busy={unsavedBusy}
	ondiscard={() => void discardAndProceed()}
	onsave={() => void saveAndProceed()}
/>
