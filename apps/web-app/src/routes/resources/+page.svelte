<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Switch from '$lib/components/ui/Switch.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import JsonEditor from '$lib/components/domain/JsonEditor.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import { jsonErrorLine } from '$lib/services/templates';
	import {
		executeTool,
		formFromToolParams,
		formToToolParams,
		getSkillContent,
		getToolDetail,
		listModelProfiles,
		listProviders,
		listScripts,
		listSkills,
		listTools,
		setSkillEnabled,
		setToolEnabled,
		validateToolParams,
		type ToolParameterSchema,
	} from '$lib/services/resources';
	import type {
		ModelProfile,
		Provider,
		Script,
		Skill,
		Tool,
		ToolRun,
	} from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import {
		formatNumber,
		formatPercent,
		formatRelativeTime,
	} from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';
	import { onMount } from 'svelte';
	import { page } from '$app/state';

	const TABS = [
		{ id: 'models', label: 'Models' },
		{ id: 'tools', label: 'Tools' },
		{ id: 'scripts', label: 'Scripts' },
		{ id: 'skills', label: 'Skills' },
	];

	const requestedTab = parseListParams(page.url).tab;
	let tab = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab)
			? requestedTab
			: 'models',
	);

	$effect(() => {
		gotoWithParams(page.url, { tab: tab === 'models' ? '' : tab });
	});
	let modelProfiles = $state<ModelProfile[]>([]);
	let providers = $state<Provider[]>([]);
	let tools = $state<Tool[]>([]);
	let scripts = $state<Script[]>([]);
	let skills = $state<Skill[]>([]);

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
	let toolParams = $state('{}');
	let toolSyntaxError = $state<string | null>(null);
	let toolEditor = $state<{
		scrollToLine: (line: number) => void;
	} | null>(null);
	let toolIssues = $state<string[]>([]);
	let toolValidated = $state(false);
	let toolRun = $state<ToolRun | null>(null);
	let toolBusy = $state(false);
	let toolTab = $state<'form' | 'json'>('json');
	let toolSchema = $state<ToolParameterSchema | null>(null);
	let toolForm = $state<Record<string, string>>({});

	const toolErrorLine = $derived(
		toolSyntaxError ? jsonErrorLine(toolParams, toolSyntaxError) : null,
	);

	let skillDialogOpen = $state(false);
	let activeSkill = $state<Skill | null>(null);
	let skillContent = $state('');
	let skillContentError = $state<string | null>(null);

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
				modelProfiles = await listModelProfiles();
				providers = await listProviders();
			} else if (current === 'tools' && !seenTools) {
				seenTools = true;
				const toolPage = await listTools({ limit: 200 });
				tools = toolPage.items;
				toolEnabled = Object.fromEntries(
					tools.map((tool) => [tool.id, tool.enabled]),
				);
			} else if (current === 'scripts' && !seenScripts) {
				seenScripts = true;
				const scriptPage = await listScripts({ limit: 200 });
				scripts = scriptPage.items;
			} else if (current === 'skills' && !seenSkills) {
				seenSkills = true;
				skills = await listSkills();
				skillEnabled = Object.fromEntries(
					skills.map((skill) => [skill.id, skill.enabled]),
				);
			}
		} catch (e) {
			tabError = e instanceof Error ? e.message : 'Resources failed.';
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
				'Tool toggle failed',
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
				'Skill toggle failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	function openToolDialog(tool: Tool): void {
		activeTool = tool;
		toolParams = '{}';
		toolSyntaxError = null;
		toolIssues = [];
		toolValidated = false;
		toolRun = null;
		toolTab = 'json';
		toolSchema = null;
		toolForm = {};
		toolDialogOpen = true;
		void loadToolSchema(tool.id);
	}

	async function loadToolSchema(toolId: string): Promise<void> {
		try {
			const detail = await getToolDetail(toolId);
			toolSchema = detail.parameters;
			if (toolSchema && Object.keys(toolSchema.properties).length > 0) {
				toolForm = formFromToolParams(toolSchema, {});
				toolTab = 'form';
			}
		} catch {
			toolSchema = null;
		}
	}

	function enterToolForm(): void {
		if (!toolSchema) return;
		try {
			const parsed = JSON.parse(toolParams) as Record<string, unknown>;
			toolForm = formFromToolParams(
				toolSchema,
				parsed && typeof parsed === 'object' ? parsed : {},
			);
		} catch {
			toolForm = formFromToolParams(toolSchema, {});
		}
		toolTab = 'form';
	}

	function syncToolFormToText(): void {
		if (!toolSchema) return;
		let base: Record<string, unknown> = {};
		try {
			const parsed = JSON.parse(toolParams) as unknown;
			if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
				base = parsed as Record<string, unknown>;
			}
		} catch {
			base = {};
		}
		toolParams = JSON.stringify(
			formToToolParams(toolSchema, toolForm, base),
			null,
			2,
		);
	}

	function switchToolTab(next: 'form' | 'json'): void {
		if (next === 'form' && toolTab !== 'form') enterToolForm();
		if (next === 'json' && toolTab !== 'json') syncToolFormToText();
		toolTab = next;
	}

	function parseToolParams(): {
		value: Record<string, unknown> | null;
		error: string | null;
	} {
		try {
			const value = JSON.parse(toolParams) as unknown;
			if (!value || typeof value !== 'object' || Array.isArray(value)) {
				return { value: null, error: 'Parameters must be a JSON object' };
			}
			return { value: value as Record<string, unknown>, error: null };
		} catch (e) {
			return {
				value: null,
				error: e instanceof Error ? e.message : 'Invalid JSON',
			};
		}
	}

	async function runToolValidation(): Promise<void> {
		if (!activeTool) return;
		if (toolTab === 'form') syncToolFormToText();
		const { value, error } = parseToolParams();
		toolSyntaxError = error;
		if (!value) {
			toolIssues = [error ?? 'Invalid JSON'];
			toolValidated = false;
			return;
		}
		toolBusy = true;
		try {
			toolIssues = await validateToolParams(activeTool.id, value);
			toolValidated = true;
		} catch (e) {
			toolIssues = [e instanceof Error ? e.message : 'Validation failed.'];
			toolValidated = false;
		} finally {
			toolBusy = false;
		}
	}

	async function runToolExecute(): Promise<void> {
		if (!activeTool) return;
		if (toolTab === 'form') syncToolFormToText();
		const { value, error } = parseToolParams();
		toolSyntaxError = error;
		if (!value) {
			toolIssues = [error ?? 'Invalid JSON'];
			toolValidated = false;
			return;
		}
		toolBusy = true;
		toolRun = null;
		try {
			toolRun = await executeTool(activeTool.id, value);
		} catch (e) {
			toasts.error(
				'Tool run failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			toolBusy = false;
		}
	}

	async function openSkillDialog(skill: Skill): Promise<void> {
		activeSkill = skill;
		skillContent = '';
		skillContentError = null;
		skillDialogOpen = true;
		try {
			skillContent = await getSkillContent(skill.name);
		} catch (e) {
			skillContentError =
				e instanceof Error ? e.message : 'Prompt failed to load.';
		}
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Models & tools"
		description="Profiles, providers, tool registry, scripts and skills."
	>
		{#snippet actions()}
			<IconButton
				icon="refresh"
				label="Refresh"
				onclick={() => void reload()}
			/>
			<Button
				size="sm"
				disabled
				title="Resource creation is not available in this release"
			>
				<Icon name="plus" size={13} />
				New
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" panelId="resources-panel" />

	<div
		id="resources-panel"
		role="tabpanel"
		aria-label="Resource sections"
		class="min-h-0 flex-1 overflow-y-auto px-4 py-3"
	>
		{#if tabLoading}
			<Skeleton lines={5} class="rounded-lg border border-border bg-card p-4" />
		{:else if tabError}
			<ErrorState
				title="Resources failed to load"
				description={tabError}
				onretry={() => void loadTab(tab)}
				class="rounded-lg border border-border bg-card"
			/>
		{:else if tab === 'models'}
			<div class="space-y-3">
				<Card title="Model profiles" bodyClass="p-0">
					<div class="overflow-x-auto">
						<table class="w-full border-collapse text-body">
							<thead>
								<tr class="border-b border-border">
									<th
										class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
										>Profile</th
									>
									<th
										class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
										>Provider</th
									>
									<th
										class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
										>Model</th
									>
									<th
										class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
										>Status</th
									>
									<th
										class="px-3 py-2 text-right text-micro uppercase tracking-wide text-muted-foreground"
										>Requests</th
									>
									<th
										class="px-3 py-2 text-right text-micro uppercase tracking-wide text-muted-foreground"
										>Tokens</th
									>
									<th
										class="px-3 py-2 text-right text-micro uppercase tracking-wide text-muted-foreground"
										>Cost</th
									>
								</tr>
							</thead>
							<tbody>
								{#each modelProfiles as profile (profile.id)}
									<tr
										class="border-b border-border/60 transition-colors last:border-0 hover:bg-accent/40"
									>
										<td class="px-3 py-2.5">
											<span class="flex items-center gap-1.5">
												<span class="truncate">{profile.name}</span>
												{#if profile.isDefault}
													<Badge variant="info" class="text-[0.625rem]"
														>default</Badge
													>
												{/if}
											</span>
										</td>
										<td class="px-3 py-2.5 text-caption text-muted-foreground"
											>{profile.provider}</td
										>
										<td class="px-3 py-2.5 font-mono text-caption"
											>{profile.model}</td
										>
										<td class="px-3 py-2.5"
											><StatusBadge status={profile.status} size="sm" /></td
										>
										<td
											class="px-3 py-2.5 text-right tabular-nums text-caption"
										>
											{formatNumber(profile.requests)}
										</td>
										<td
											class="px-3 py-2.5 text-right tabular-nums text-caption text-muted-foreground"
										>
											{formatNumber(profile.tokens)}
										</td>
										<td
											class="px-3 py-2.5 text-right tabular-nums text-caption"
										>
											{profile.cost === null
												? '—'
												: `$${profile.cost.toFixed(2)}`}
										</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				</Card>

				<Card title="Providers">
					<ul class="divide-y divide-border">
						{#each providers as provider (provider.id)}
							<li
								class="flex flex-wrap items-center justify-between gap-2 py-2 first:pt-0"
							>
								<div class="min-w-0">
									<p class="truncate text-body">{provider.name}</p>
									<p
										class="truncate font-mono text-micro text-muted-foreground"
									>
										{provider.baseUrl}
									</p>
								</div>
								<div class="flex shrink-0 items-center gap-2">
									<span class="text-caption text-muted-foreground"
										>{provider.models} models</span
									>
									<StatusBadge status={provider.status} size="sm" />
								</div>
							</li>
						{/each}
					</ul>
				</Card>
			</div>
		{:else if tab === 'tools'}
			{#if tools.length === 0}
				<EmptyState
					icon="blocks"
					title="No tools registered"
					description="Tools appear here once the registry has entries."
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
					{#each tools as tool (tool.id)}
						<Card title={tool.name}>
							{#snippet actions()}
								<Switch
									checked={toolEnabled[tool.id] ?? false}
									label="Enable {tool.name}"
									hideLabel
									onchange={(checked) => void toggleTool(tool.id, checked)}
								/>
							{/snippet}
							<p class="text-caption text-muted-foreground">{tool.description}</p>
							<div
								class="mt-2 flex items-center justify-between text-micro text-muted-foreground"
							>
								<span class="rounded border border-border px-1.5 py-0.5"
									>{tool.kind}</span
								>
								<span class="tabular-nums">
									{formatNumber(tool.calls)} calls ·
									{tool.successRate === null
										? '—'
										: formatPercent(tool.successRate, 0)} ok
								</span>
							</div>
							{#snippet footer()}
								<div class="flex items-center gap-2">
									<Button
										variant="ghost"
										size="sm"
										onclick={() => openToolDialog(tool)}
									>
										Validate / Run
									</Button>
								</div>
							{/snippet}
						</Card>
					{/each}
				</div>
			{/if}
		{:else if tab === 'scripts'}
			<Card bodyClass="p-0">
				<div class="overflow-x-auto">
					<table class="w-full border-collapse text-body">
						<thead>
							<tr class="border-b border-border">
								<th
									class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
									>Script</th
								>
								<th
									class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
									>Runtime</th
								>
								<th
									class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
									>State</th
								>
								<th
									class="px-3 py-2 text-right text-micro uppercase tracking-wide text-muted-foreground"
									>Runs</th
								>
								<th
									class="px-3 py-2 text-right text-micro uppercase tracking-wide text-muted-foreground"
									>Updated</th
								>
							</tr>
						</thead>
						<tbody>
							{#each scripts as script (script.id)}
								<tr
									class="border-b border-border/60 transition-colors last:border-0 hover:bg-accent/40"
								>
									<td class="px-3 py-2.5 font-mono text-caption"
										>{script.name}</td
									>
									<td class="px-3 py-2.5 text-caption text-muted-foreground"
										>{script.runtime}</td
									>
									<td class="px-3 py-2.5">
										<StatusBadge
											status={script.enabled ? 'enabled' : 'disabled'}
											size="sm"
										/>
									</td>
									<td class="px-3 py-2.5 text-right tabular-nums text-caption">
										{formatNumber(script.runs)}
									</td>
									<td
										class="px-3 py-2.5 text-right text-caption text-muted-foreground"
									>
										{formatRelativeTime(script.updatedAt)}
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			</Card>
		{:else}
			{#if skills.length === 0}
				<EmptyState
					icon="sparkles"
					title="No skills registered"
					description="Skills appear here once the registry has entries."
					class="rounded-lg border border-border bg-card"
				/>
			{:else}
				<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
					{#each skills as skill (skill.id)}
						<Card title={skill.name}>
							{#snippet actions()}
								<Switch
									checked={skillEnabled[skill.id] ?? false}
									label="Enable {skill.name}"
									hideLabel
									onchange={(checked) => void toggleSkill(skill.id, checked)}
								/>
							{/snippet}
							<p class="text-caption text-muted-foreground">
								{skill.description}
							</p>
							<p
								class="mt-2 rounded-md bg-muted px-2 py-1.5 font-mono text-micro text-muted-foreground"
							>
								{skill.promptPreview}
							</p>
							{#snippet footer()}
								<div class="flex items-center justify-between">
									<span
										class={cn(
											'tabular-nums',
											skill.enabled ? 'text-success' : 'text-muted-foreground',
										)}
									>
										v{skill.version}
									</span>
									<Button
										variant="ghost"
										size="sm"
										onclick={() => void openSkillDialog(skill)}
									>
										View prompt
									</Button>
								</div>
							{/snippet}
						</Card>
					{/each}
				</div>
			{/if}
		{/if}
	</div>
</div>

<Dialog
	bind:open={toolDialogOpen}
	title={activeTool ? `Validate / run ${activeTool.name}` : 'Tool'}
	description={toolSchema
		? 'Structured fields with JSON advanced mode; validation stays server-side.'
		: 'Free-form JSON validated server-side; no parameter schema was reported for this tool.'}
>
	{#if toolSchema && Object.keys(toolSchema.properties).length > 0}
		<Segmented
			items={[
				{ id: 'form', label: 'Form' },
				{ id: 'json', label: 'JSON' },
			]}
			value={toolTab}
			size="sm"
			onchange={(id) => switchToolTab(id as 'form' | 'json')}
		/>
		{#if toolTab === 'form'}
			<div class="mt-2 space-y-2">
				{#each Object.entries(toolSchema.properties) as [key, prop] (key)}
					{@const required = toolSchema.required.includes(key)}
					<label class="block">
						<span class="mb-1 block text-caption text-muted-foreground">
							{key}{#if required}<span class="text-destructive"> *</span>{/if}
							{#if prop.description}
								<span class="ml-1 text-micro">· {prop.description}</span>
							{/if}
						</span>
						{#if prop.enum && prop.enum.length > 0}
							<Select
								value={toolForm[key] ?? ''}
								options={prop.enum.map((option) => ({
									value: String(option),
									label: String(option),
								}))}
								size="sm"
								placeholder={key}
								class="w-full"
								onchange={(value) => {
									toolForm = { ...toolForm, [key]: value };
								}}
							/>
						{:else if prop.type === 'boolean'}
							<Select
								value={toolForm[key] ?? ''}
								options={[
									{ value: 'true', label: 'true' },
									{ value: 'false', label: 'false' },
								]}
								size="sm"
								placeholder={key}
								class="w-40"
								onchange={(value) => {
									toolForm = { ...toolForm, [key]: value };
								}}
							/>
						{:else}
							<Input
								bind:value={toolForm[key]}
								placeholder={prop.description ?? key}
								size="sm"
								class="w-full"
							/>
						{/if}
					</label>
				{/each}
				<p class="text-micro text-muted-foreground">
					Form edits typed fields; remaining parameters stay in JSON mode.
				</p>
			</div>
		{:else}
			<div class="mt-2">
				<JsonEditor
					bind:this={toolEditor}
					bind:value={toolParams}
					errorLine={toolErrorLine}
					placeholder={'{\n  "input": "value"\n}'}
					label="Tool parameters"
					minHeight="8rem"
				/>
			</div>
		{/if}
	{:else}
		<JsonEditor
			bind:this={toolEditor}
			bind:value={toolParams}
			errorLine={toolErrorLine}
			placeholder={'{\n  "input": "value"\n}'}
			label="Tool parameters"
			minHeight="8rem"
		/>
	{/if}
	{#if toolSyntaxError}
		<div class="mt-2 flex items-center justify-between gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5">
			<p class="text-caption text-destructive">
				JSON syntax{#if toolErrorLine} (line {toolErrorLine}){/if}: {toolSyntaxError}
			</p>
			{#if toolErrorLine}
				<Button
					variant="ghost"
					size="sm"
					onclick={() => toolEditor?.scrollToLine(toolErrorLine ?? 1)}
				>
					Go to line
				</Button>
			{/if}
		</div>
	{/if}
	{#if toolIssues.length > 0}
		<ul class="mt-2 space-y-1 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5">
			{#each toolIssues as issue, index (index)}
				<li class="text-caption text-destructive">{issue}</li>
			{/each}
		</ul>
	{:else if toolValidated}
		<p class="mt-2 text-caption text-success">Parameters are valid.</p>
	{/if}
	{#if toolRun}
		<div class="mt-2 rounded-md border border-border bg-muted/40 p-2">
			<p class="text-caption">
				{toolRun.success ? 'Succeeded' : `Failed: ${toolRun.error}`}
			</p>
			{#if toolRun.output}
				<pre class="mt-1 max-h-48 overflow-auto font-mono text-micro">{toolRun.output}</pre>
			{/if}
			<p class="mt-1 text-micro text-muted-foreground">
				{toolRun.durationMs}ms · {toolRun.retries} retries
			</p>
		</div>
	{/if}
	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button
				variant="outline"
				size="sm"
				disabled={toolBusy}
				onclick={() => void runToolValidation()}
			>
				Validate
			</Button>
			<Button size="sm" disabled={toolBusy} onclick={() => void runToolExecute()}>
				{toolBusy ? 'Running…' : 'Run'}
			</Button>
		</div>
	{/snippet}
</Dialog>

<Dialog
	bind:open={skillDialogOpen}
	title={activeSkill ? `Prompt · ${activeSkill.name}` : 'Skill prompt'}
>
	{#if skillContentError}
		<ErrorState
			title="Prompt failed to load"
			description={skillContentError}
			onretry={() => {
				if (activeSkill) void openSkillDialog(activeSkill);
			}}
		/>
	{:else if !skillContent}
		<Skeleton lines={6} />
	{:else}
		<pre
			class="max-h-96 overflow-auto rounded-md border border-border bg-muted/40 p-2 font-mono text-small"
		>{skillContent}</pre>
	{/if}
	{#snippet footer()}
		<div class="flex items-center justify-end">
			<Button variant="ghost" size="sm" onclick={() => (skillDialogOpen = false)}>
				Close
			</Button>
		</div>
	{/snippet}
</Dialog>
