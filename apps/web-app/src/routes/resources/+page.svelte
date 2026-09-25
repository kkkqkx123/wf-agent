<script lang="ts">
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Switch from '$lib/components/ui/Switch.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import Textarea from '$lib/components/ui/Textarea.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import {
		listModelProfiles,
		listProviders,
		listTools,
		listScripts,
		listSkills,
		setSkillEnabled,
		setToolEnabled,
		validateToolParams,
		executeTool,
		getSkillContent,
	} from '$lib/services/resources';
	import type {
		ModelProfile,
		Provider,
		Tool,
		Script,
		Skill,
	} from '$lib/types/models';
	import { createResource } from '$lib/stores/collection.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import {
		formatNumber,
		formatPercent,
		formatRelativeTime,
	} from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';
	import { page } from '$app/state';

	const TABS = [
		{ id: 'models', label: 'Models' },
		{ id: 'tools', label: 'Tools' },
		{ id: 'scripts', label: 'Scripts' },
		{ id: 'skills', label: 'Skills' },
	];

	interface Registry {
		modelProfiles: ModelProfile[];
		providers: Provider[];
		tools: Tool[];
		scripts: Script[];
		skills: Skill[];
		failed: string[];
	}

	const initial = parseListParams(page.url);
	let tab = $state(initial.tab ?? 'models');
	let toggling = $state('');

	$effect(() => {
		gotoWithParams(page.url, { tab });
	});

	const GROUPS = ['profiles', 'providers', 'tools', 'scripts', 'skills'];

	const registry = createResource<Registry>(async () => {
		const settled = await Promise.allSettled([
			listModelProfiles(),
			listProviders(),
			listTools({ limit: 100 }),
			listScripts({ limit: 100 }),
			listSkills(),
		]);
		const failed = settled
			.map((result, index) =>
				result.status === 'rejected' ? GROUPS[index] : '',
			)
			.filter(Boolean);
		const value = <T,>(result: PromiseSettledResult<T>, fallback: T): T =>
			result.status === 'fulfilled' ? result.value : fallback;
		const emptyPage = { items: [], hasMore: false, limit: 0, offset: 0 };
		return {
			modelProfiles: value(settled[0], []),
			providers: value(settled[1], []),
			tools: value(settled[2], emptyPage).items,
			scripts: value(settled[3], emptyPage).items,
			skills: value(settled[4], []),
			failed,
		};
	});

	$effect(() => {
		void registry.reload();
	});

	const modelProfiles = $derived(registry.data?.modelProfiles ?? []);
	const providers = $derived(registry.data?.providers ?? []);
	const tools = $derived(registry.data?.tools ?? []);
	const scripts = $derived(registry.data?.scripts ?? []);
	const skills = $derived(registry.data?.skills ?? []);
	const failed = $derived(registry.data?.failed ?? []);
	const allFailed = $derived(
		registry.data !== null && failed.length === GROUPS.length,
	);

	async function toggle(
		kind: 'tool' | 'skill',
		item: Tool | Skill,
		checked: boolean,
	): Promise<void> {
		toggling = `${kind}:${item.id}`;
		try {
			if (kind === 'tool') await setToolEnabled(item.id, checked);
			else await setSkillEnabled(item.name, checked);
			await registry.reload();
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Toggle failed');
		} finally {
			toggling = '';
		}
	}

	let toolTarget = $state<Tool | null>(null);
	let toolOpen = $state(false);
	let toolParams = $state('{}');
	let toolResult = $state('');
	let toolBusy = $state(false);

	function openTool(tool: Tool): void {
		toolTarget = tool;
		toolParams = '{}';
		toolResult = '';
		toolOpen = true;
	}

	/** Read the parameter box, reporting anything malformed into the result. */
	function readParams(): Record<string, unknown> | null {
		try {
			const parsed: unknown = JSON.parse(toolParams);
			if (parsed === null || typeof parsed !== 'object') {
				throw new Error('not an object');
			}
			return parsed as Record<string, unknown>;
		} catch {
			toolResult = 'Parameters must be a JSON object.';
			return null;
		}
	}

	async function checkParams(): Promise<void> {
		const target = toolTarget;
		const parameters = target ? readParams() : null;
		if (!target || !parameters) return;
		toolBusy = true;
		try {
			const errors = await validateToolParams(target.id, parameters);
			toolResult =
				errors.length === 0
					? 'Parameters match the tool schema.'
					: errors.join('\n');
		} catch (e) {
			toolResult = e instanceof Error ? e.message : 'Validation failed';
		} finally {
			toolBusy = false;
		}
	}

	async function runTool(): Promise<void> {
		const target = toolTarget;
		const parameters = target ? readParams() : null;
		if (!target || !parameters) return;
		toolBusy = true;
		try {
			const run = await executeTool(target.id, parameters);
			const detail = run.success ? run.output || 'ok' : run.error || 'failed';
			toolResult = `${run.success ? 'ok' : 'error'} · ${run.durationMs} ms · ${detail}`;
		} catch (e) {
			toolResult = e instanceof Error ? e.message : 'Execution failed';
		} finally {
			toolBusy = false;
		}
	}

	let skillTarget = $state<Skill | null>(null);
	let skillOpen = $state(false);
	let skillContent = $state('');

	async function openSkill(skill: Skill): Promise<void> {
		skillTarget = skill;
		skillContent = 'Loading…';
		skillOpen = true;
		try {
			skillContent = await getSkillContent(skill.name);
		} catch (e) {
			skillContent = e instanceof Error ? e.message : 'Failed to load prompt';
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
				onclick={() => void registry.reload()}
				disabled={registry.loading}
			/>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={tab} class="px-4" />

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if registry.loading && !registry.data}
			<div class="space-y-3">
				{#each Array.from({ length: 4 }, (_, position) => position) as index (index)}
					<Skeleton class="h-[124px] rounded-lg" />
				{/each}
			</div>
		{:else if allFailed}
			<EmptyState
				icon="alert-circle"
				title="Resource registry unreachable"
				description={failed.join(', ')}
				class="rounded-lg border border-border bg-card"
			>
				{#snippet actions()}
					<Button
						variant="outline"
						size="sm"
						onclick={() => void registry.reload()}
					>
						Retry
					</Button>
				{/snippet}
			</EmptyState>
		{:else}
			{#if failed.length > 0}
				<p
					class="mb-2 rounded-md border border-warning/40 bg-warning/10 px-3 py-1.5 text-caption text-warning"
				>
					Unavailable: {failed.join(', ')}
				</p>
			{/if}
			{#if tab === 'models'}
				<div class="space-y-3">
					{#snippet profileName(profile: ModelProfile)}
						<span class="flex items-center gap-1.5">
							<span class="truncate">{profile.name}</span>
							{#if profile.isDefault}
								<Badge variant="info" size="sm">default</Badge>
							{/if}
						</span>
					{/snippet}
					{#snippet profileProvider(profile: ModelProfile)}
						<span class="text-caption text-muted-foreground"
							>{profile.provider}</span
						>
					{/snippet}
					{#snippet profileModel(profile: ModelProfile)}
						<span class="font-mono text-caption">{profile.model}</span>
					{/snippet}
					{#snippet profileStatus(profile: ModelProfile)}
						<StatusBadge status={profile.status} size="sm" />
					{/snippet}
					{#snippet profileRequests(profile: ModelProfile)}
						<span class="text-caption tabular-nums">
							{formatNumber(profile.requests)}
						</span>
					{/snippet}
					{#snippet profileTokens(profile: ModelProfile)}
						<span class="text-caption tabular-nums text-muted-foreground">
							{formatNumber(profile.tokens)}
						</span>
					{/snippet}
					{#snippet profileCost(profile: ModelProfile)}
						<span class="text-caption tabular-nums">
							{profile.cost === null ? '—' : `$${profile.cost.toFixed(2)}`}
						</span>
					{/snippet}
					<Card title="Model profiles" bodyClass="p-0">
						<DataTable
							rows={modelProfiles}
							rowKey={(row) => row.id}
							columns={[
								{ key: 'profile', header: 'Profile', cell: profileName },
								{ key: 'provider', header: 'Provider', cell: profileProvider },
								{ key: 'model', header: 'Model', cell: profileModel },
								{ key: 'status', header: 'Status', cell: profileStatus },
								{
									key: 'requests',
									header: 'Requests',
									align: 'right',
									cell: profileRequests,
								},
								{
									key: 'tokens',
									header: 'Tokens',
									align: 'right',
									cell: profileTokens,
								},
								{
									key: 'cost',
									header: 'Cost',
									align: 'right',
									cell: profileCost,
								},
							]}
						/>
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
				<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
					{#each tools as tool (tool.id)}
						<Card title={tool.name}>
							{#snippet actions()}
								<Switch
									checked={tool.enabled}
									label="Enable {tool.name}"
									hideLabel
									disabled={toggling === `tool:${tool.id}`}
									onchange={(checked) => void toggle('tool', tool, checked)}
								/>
							{/snippet}
							<p class="text-caption text-muted-foreground">
								{tool.description}
							</p>
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
								<Button
									variant="ghost"
									size="sm"
									onclick={() => openTool(tool)}
								>
									Test
								</Button>
							{/snippet}
						</Card>
					{/each}
				</div>
			{:else if tab === 'scripts'}
				{#snippet scriptName(script: Script)}
					<span class="font-mono text-caption">{script.name}</span>
				{/snippet}
				{#snippet scriptRuntime(script: Script)}
					<span class="text-caption text-muted-foreground"
						>{script.runtime}</span
					>
				{/snippet}
				{#snippet scriptState(script: Script)}
					<StatusBadge
						status={script.enabled ? 'enabled' : 'disabled'}
						size="sm"
					/>
				{/snippet}
				{#snippet scriptRuns(script: Script)}
					<span class="text-caption tabular-nums"
						>{formatNumber(script.runs)}</span
					>
				{/snippet}
				{#snippet scriptUpdated(script: Script)}
					<span class="text-caption text-muted-foreground">
						{formatRelativeTime(script.updatedAt)}
					</span>
				{/snippet}
				<Card bodyClass="p-0">
					<DataTable
						rows={scripts}
						rowKey={(row) => row.id}
						columns={[
							{ key: 'script', header: 'Script', cell: scriptName },
							{ key: 'runtime', header: 'Runtime', cell: scriptRuntime },
							{ key: 'state', header: 'State', cell: scriptState },
							{ key: 'runs', header: 'Runs', align: 'right', cell: scriptRuns },
							{
								key: 'updated',
								header: 'Updated',
								align: 'right',
								cell: scriptUpdated,
							},
						]}
					/>
				</Card>
			{:else}
				<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
					{#each skills as skill (skill.id)}
						<Card title={skill.name}>
							{#snippet actions()}
								<Switch
									checked={skill.enabled}
									label="Enable {skill.name}"
									hideLabel
									disabled={toggling === `skill:${skill.id}`}
									onchange={(checked) => void toggle('skill', skill, checked)}
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
										onclick={() => void openSkill(skill)}
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

{#if toolTarget}
	<Dialog
		bind:open={toolOpen}
		title="Test {toolTarget.name}"
		description="Validate the parameter object against the tool schema, or execute the tool with it."
	>
		<Textarea
			bind:value={toolParams}
			rows={8}
			class="font-mono text-caption"
			placeholder={'{ "path": "…" }'}
		/>
		{#if toolResult}
			<pre
				class="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-all rounded-md bg-muted px-2 py-1.5 font-mono text-micro">{toolResult}</pre>
		{/if}
		{#snippet footer()}
			<Button variant="ghost" size="sm" onclick={() => (toolOpen = false)}
				>Close</Button
			>
			<Button
				variant="outline"
				size="sm"
				disabled={toolBusy}
				onclick={() => void checkParams()}
			>
				Validate
			</Button>
			<Button size="sm" disabled={toolBusy} onclick={() => void runTool()}>
				Run
			</Button>
		{/snippet}
	</Dialog>
{/if}

{#if skillTarget}
	<Dialog
		bind:open={skillOpen}
		title={skillTarget.name}
		description="Full skill prompt loaded from the skill directory."
	>
		<pre
			class="max-h-[55vh] overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted px-2 py-1.5 font-mono text-micro">{skillContent}</pre>
	</Dialog>
{/if}
