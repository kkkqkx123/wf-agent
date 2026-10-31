<script lang="ts">
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Segmented from '#lib/ui/components/Segmented.svelte';
	import ErrorState from '#lib/ui/components/ErrorState.svelte';
	import Skeleton from '#lib/ui/components/Skeleton.svelte';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import ModelsPanel from '#lib/components/domain/resources/ModelsPanel.svelte';
	import ToolsPanel from '#lib/components/domain/resources/ToolsPanel.svelte';
	import ScriptsPanel from '#lib/components/domain/resources/ScriptsPanel.svelte';
	import SkillsPanel from '#lib/components/domain/resources/SkillsPanel.svelte';
	import ToolRunDialog from '#lib/components/domain/resources/ToolRunDialog.svelte';
	import SkillContentDialog from '#lib/components/domain/resources/SkillContentDialog.svelte';
	import {
		listModelProfiles,
		listProviders,
		listScripts,
		listSkills,
		listTools,
		setSkillEnabled,
		setToolEnabled,
	} from '#lib/services/resources';
	import { listAuthProviders, logoutAuth, type AuthProviderStatus } from '#lib/services/auth';
	import AuthLoginDialog from '#lib/components/domain/resources/AuthLoginDialog.svelte';
	import AuthProvidersPanel from '#lib/components/domain/resources/AuthProvidersPanel.svelte';
	import type { ModelProfile, Provider, Script, Skill, Tool } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { gotoWithParams, parseListParams } from '#lib/utils/route';
	import { t } from '#lib/i18n';
	import { onMount } from 'svelte';
	import { page } from '$app/state';

	const TABS = [
		{ id: 'models', label: 'Models' },
		{ id: 'tools', label: 'Tools' },
		{ id: 'scripts', label: 'Scripts' },
		{ id: 'skills', label: 'Skills' },
	];

	const requestedTab = parseListParams(page.url as unknown as URL).tab;
	let tab = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab) ? requestedTab : 'models',
	);

	$effect(() => {
		gotoWithParams(page.url as unknown as URL, {
			tab: tab === 'models' ? '' : tab,
		});
	});
	let modelProfiles = $state<ModelProfile[]>([]);
	let providers = $state<Provider[]>([]);
	let tools = $state<Tool[]>([]);
	let scripts = $state<Script[]>([]);
	let skills = $state<Skill[]>([]);
	let authProviders = $state<AuthProviderStatus[]>([]);
	let authBusyId = $state<string | null>(null);
	let authDialogOpen = $state(false);
	let activeAuthProvider = $state<AuthProviderStatus | null>(null);

	let tabError = $state<string | null>(null);
	let tabLoading = $state(false);

	let toolEnabled = $state<Record<string, boolean>>({});
	let skillEnabled = $state<Record<string, boolean>>({});

	/** Segment sources already pulled, so a tab loads once. */
	let seenModels = $state(false);
	let seenTools = $state(false);
	let seenScripts = $state(false);
	let seenSkills = $state(false);

	let toolDialogOpen = $state(false);
	let activeTool = $state<Tool | null>(null);

	let skillDialogOpen = $state(false);
	let activeSkill = $state<Skill | null>(null);

	onMount(() => {
		void loadTab(tab);
	});

	$effect(() => {
		void loadTab(tab);
	});

	async function loadTab(current: string): Promise<void> {
		tabLoading = true;
		tabError = null;
		try {
			if (current === 'models' && !seenModels) {
				seenModels = true;
				const [profiles, providerList, authList] = await Promise.all([
					listModelProfiles(),
					listProviders().catch(() => [] as Provider[]),
					listAuthProviders().catch(() => [] as AuthProviderStatus[]),
				]);
				modelProfiles = profiles;
				providers = providerList;
				authProviders = authList;
			} else if (current === 'tools' && !seenTools) {
				seenTools = true;
				const toolPage = await listTools({ limit: 200 });
				tools = toolPage.items;
				toolEnabled = Object.fromEntries(tools.map((tool) => [tool.id, tool.enabled]));
			} else if (current === 'scripts' && !seenScripts) {
				seenScripts = true;
				const scriptPage = await listScripts({ limit: 200 });
				scripts = scriptPage.items;
			} else if (current === 'skills' && !seenSkills) {
				seenSkills = true;
				skills = await listSkills();
				skillEnabled = Object.fromEntries(skills.map((skill) => [skill.id, skill.enabled]));
			}
		} catch (e) {
			tabError = e instanceof Error ? e.message : t('page.resources.loadFailed');
			if (current === 'models') seenModels = false;
			if (current === 'tools') seenTools = false;
			if (current === 'scripts') seenScripts = false;
			if (current === 'skills') seenSkills = false;
		} finally {
			tabLoading = false;
		}
	}

	async function reload(): Promise<void> {
		seenModels = false;
		seenTools = false;
		seenScripts = false;
		seenSkills = false;
		await loadTab(tab);
	}

	async function toggleTool(id: string, checked: boolean): Promise<void> {
		const previous = toolEnabled[id] ?? false;
		toolEnabled = { ...toolEnabled, [id]: checked };
		try {
			await setToolEnabled(id, checked);
		} catch (e) {
			toolEnabled = { ...toolEnabled, [id]: previous };
			toasts.error(
				t('page.resources.toolToggleFailed'),
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function toggleSkill(name: string, checked: boolean): Promise<void> {
		const row = skills.find((s) => s.id === name);
		const key = row?.name ?? name;
		const previous = skillEnabled[name] ?? false;
		skillEnabled = { ...skillEnabled, [name]: checked };
		try {
			await setSkillEnabled(key, checked);
		} catch (e) {
			skillEnabled = { ...skillEnabled, [name]: previous };
			toasts.error(
				t('page.resources.skillToggleFailed'),
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	function openToolDialog(tool: Tool): void {
		activeTool = tool;
		toolDialogOpen = true;
	}

	function openSkillDialog(skill: Skill): void {
		activeSkill = skill;
		skillDialogOpen = true;
	}

	function openAuthDialog(provider: AuthProviderStatus): void {
		authBusyId = provider.id;
		activeAuthProvider = provider;
		authDialogOpen = true;
		authBusyId = null;
	}

	async function logoutAuthProvider(provider: AuthProviderStatus): Promise<void> {
		authBusyId = provider.id;
		try {
			await logoutAuth(provider.id);
			toasts.success(t('page.resources.auth.logoutSucceeded'));
			authProviders = await listAuthProviders().catch(() => authProviders);
		} catch (e) {
			toasts.error(
				t('page.resources.auth.logoutFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			authBusyId = null;
		}
	}

	async function refreshAuthProviders(): Promise<void> {
		try {
			authProviders = await listAuthProviders();
		} catch {
			authProviders = [];
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader title={t('page.resources.title')} description={t('page.resources.description')}>
		{#snippet actions()}
			<IconButton icon="refresh" label={t('common.refresh')} onclick={() => void reload()} />
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" panelId="resources-panel" />

	<div
		id="resources-panel"
		role="tabpanel"
		aria-label={t('page.resources.sections')}
		class="min-h-0 flex-1 overflow-y-auto px-4 py-3"
	>
		{#if tabLoading}
			<Skeleton lines={5} class="rounded-lg border border-border bg-card p-4" />
		{:else if tabError}
			<ErrorState
				title={t('page.resources.loadFailed')}
				description={tabError}
				onretry={() => void loadTab(tab)}
				class="rounded-lg border border-border bg-card"
			/>
		{:else if tab === 'models'}
			<div class="space-y-3">
				<AuthProvidersPanel
					providers={authProviders}
					busyId={authBusyId}
					onconnect={openAuthDialog}
					onlogout={(provider) => void logoutAuthProvider(provider)}
				/>
				<ModelsPanel {modelProfiles} {providers} />
			</div>
		{:else if tab === 'tools'}
			<ToolsPanel
				{tools}
				{toolEnabled}
				ontoggle={(id, checked) => void toggleTool(id, checked)}
				onrun={openToolDialog}
			/>
		{:else if tab === 'scripts'}
			<ScriptsPanel {scripts} />
		{:else}
			<SkillsPanel
				{skills}
				{skillEnabled}
				ontoggle={(id, checked) => void toggleSkill(id, checked)}
				onview={openSkillDialog}
			/>
		{/if}
	</div>
</div>

<ToolRunDialog bind:open={toolDialogOpen} tool={activeTool} />

<SkillContentDialog bind:open={skillDialogOpen} skill={activeSkill} />

<AuthLoginDialog
	bind:open={authDialogOpen}
	provider={activeAuthProvider}
	ondone={() => void refreshAuthProviders()}
/>
