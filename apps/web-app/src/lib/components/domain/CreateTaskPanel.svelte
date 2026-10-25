<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import ErrorState from '#lib/ui/components/ErrorState.svelte';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import Select from '#lib/ui/components/Select.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import Skeleton from '#lib/ui/components/Skeleton.svelte';
	import Textarea from '#lib/ui/components/Textarea.svelte';
	import JsonViewer from '#lib/ui/components/JsonViewer.svelte';
	import { listWorkflows } from '#lib/services/workflows';
	import { listTemplates } from '#lib/services/templates';
	import { listModelProfiles } from '#lib/services/resources';
	import { listDefinitionVersions } from '#lib/services/definitions';
	import { runDefinition, type RunnableKind } from '#lib/services/run';
	import { toasts } from '#lib/stores/toast.svelte';
	import { t } from '#lib/i18n';

	/** One definition that can be started as a run. */
	interface RunnableDefinition {
		kind: RunnableKind;
		id: string;
		name: string;
	}

	interface Props {
		/** Called once a run has started, with the execution it produced. */
		onstarted?: (executionId: string) => void;
		/** Deep-link preselection from a definition page (`kind:id`). */
		initialDefinition?: string | null;
		/** Pinned workflow version carried by the deep link. */
		initialVersion?: string | null;
	}

	let { onstarted, initialDefinition = null, initialVersion = null }: Props = $props();

	let definitions = $state<RunnableDefinition[]>([]);
	let modelOptions = $state<{ value: string; label: string }[]>([]);
	let loading = $state(true);
	let loadError = $state<string | null>(null);
	let selected = $state('');
	let message = $state('');
	let model = $state('');
	let input = $state('');
	let version = $state('');
	let versions = $state<{ value: string; label: string }[]>([]);
	let versionsLoading = $state(false);
	let fileScope = $state('');
	let fileScopeError = $state<string | null>(null);
	let debugRun = $state(false);
	let maxIterations = $state('');
	let tokenLimit = $state('');
	let conversationSeed = $state('');
	let conversationSeedError = $state<string | null>(null);
	let starting = $state(false);

	// Execution type and definition are two layers: the type picks the
	// executor, the definition picks what it runs. Deep links preselect
	// both so definition pages land on a ready-to-start workbench.
	let executionType = $state('workflow');

	const executionTypeOptions = $derived([
		{ value: 'workflow', label: 'Workflow' },
		{ value: 'agent', label: 'Agent' },
	]);

	const filteredDefinitions = $derived(
		definitions.filter((definition) => definition.kind === executionType),
	);

	const options = $derived(
		filteredDefinitions.map((definition) => ({
			value: `${definition.kind}:${definition.id}`,
			label: `${definition.name} · ${definition.kind}`,
		})),
	);

	const selectedDefinition = $derived(
		definitions.find((definition) => `${definition.kind}:${definition.id}` === selected) ?? null,
	);

	/** Parsed workflow input: JSON when it parses, raw text otherwise. */
	const parsedInput = $derived.by((): unknown => {
		const trimmed = input.trim();
		if (trimmed === '') return null;
		try {
			return JSON.parse(trimmed) as unknown;
		} catch {
			return trimmed;
		}
	});

	const parsedFileScope = $derived.by((): unknown => {
		const trimmed = fileScope.trim();
		if (trimmed === '') return null;
		try {
			return JSON.parse(trimmed) as unknown;
		} catch {
			return null;
		}
	});

	function validateJsonFields(): boolean {
		fileScopeError = null;
		conversationSeedError = null;
		if (input.trim() !== '') {
			try {
				JSON.parse(input);
			} catch {
				// Invalid JSON is tolerated here; no inline error state is shown.
			}
		}
		if (fileScope.trim() !== '') {
			try {
				JSON.parse(fileScope);
			} catch {
				fileScopeError = 'File scope must be valid JSON or empty.';
				return false;
			}
		}
		if (conversationSeed.trim() !== '') {
			try {
				const parsed = JSON.parse(conversationSeed) as unknown;
				if (!Array.isArray(parsed)) {
					conversationSeedError = 'Conversation seed must be a JSON array or empty.';
					return false;
				}
			} catch {
				conversationSeedError = 'Conversation seed must be a JSON array or empty.';
				return false;
			}
		}
		return true;
	}

	async function loadDefinitions(): Promise<void> {
		loading = true;
		loadError = null;
		try {
			const [workflows, agents, profiles] = await Promise.all([
				listWorkflows({ limit: 200 }),
				listTemplates({ kind: 'agent' }),
				listModelProfiles(),
			]);
			modelOptions = profiles.map((profile) => ({
				value: profile.model,
				label: profile.name || profile.model,
			}));
			const preferred = profiles.find((profile) => profile.isDefault) ?? profiles[0];
			if (!model && preferred) model = preferred.model;
			definitions = [
				...workflows.items.map((workflow) => ({
					kind: 'workflow' as const,
					id: workflow.id,
					name: workflow.name,
				})),
				...agents.map((agent) => ({
					kind: 'agent' as const,
					id: agent.id,
					name: agent.name,
				})),
			];
			if (!selected && definitions.length > 0) {
				const preselected =
					initialDefinition &&
					definitions.some(
						(definition) => `${definition.kind}:${definition.id}` === initialDefinition,
					)
						? initialDefinition
						: `${definitions[0].kind}:${definitions[0].id}`;
				selected = preselected;
				const preselectedKind = preselected.split(':')[0];
				if (preselectedKind === 'agent' || preselectedKind === 'workflow') {
					executionType = preselectedKind;
				}
			}
		} catch (e) {
			loadError = e instanceof Error ? e.message : t('page.executions.taskLoadFailed');
		} finally {
			loading = false;
		}
	}

	async function loadVersions(definitionId: string): Promise<void> {
		versions = [];
		if (!definitionId) return;
		versionsLoading = true;
		try {
			const rows = await listDefinitionVersions('workflow', definitionId);
			versions = [
				{ value: '', label: 'Latest' },
				...rows.map((row) => ({
					value: row.version,
					label: row.current ? `${row.version} · latest` : row.version,
				})),
			];
		} catch {
			versions = [{ value: '', label: 'Latest' }];
		} finally {
			versionsLoading = false;
		}
	}

	$effect(() => {
		if (selectedDefinition?.kind !== 'workflow') return;
		void loadVersions(selectedDefinition.id);
	});

	async function start(): Promise<void> {
		const definition = selectedDefinition;
		if (!definition || starting) return;
		if (!validateJsonFields()) {
			if (fileScopeError) toasts.error(fileScopeError);
			return;
		}
		if (definition.kind === 'agent' && message.trim() === '') {
			toasts.error(t('page.executions.taskMessageRequired'));
			return;
		}
		if (definition.kind === 'agent' && model === '') {
			toasts.error(t('page.executions.taskModelRequired'));
			return;
		}
		starting = true;
		try {
			const parsedMaxIterations = maxIterations.trim() === '' ? null : Number(maxIterations);
			const parsedTokenLimit = tokenLimit.trim() === '' ? null : Number(tokenLimit);
			const parsedConversation =
				conversationSeed.trim() === '' ? [] : (JSON.parse(conversationSeed) as unknown[]);
			if (
				(parsedMaxIterations !== null && !Number.isFinite(parsedMaxIterations)) ||
				(parsedTokenLimit !== null && !Number.isFinite(parsedTokenLimit))
			) {
				toasts.error('Budget fields must be numbers or empty.');
				return;
			}
			const debugWorkflow = definition.kind === 'workflow' && debugRun;
			const executionId =
				definition.kind === 'workflow'
					? await runDefinition('workflow', definition.id, {
							input: parsedInput,
							version: version.trim() === '' ? null : version.trim(),
							fileScope: parsedFileScope,
						})
					: await runDefinition('agent', definition.id, {
							message: message.trim(),
							model,
							fileScope: parsedFileScope,
							conversation: parsedConversation,
							maxIterations: parsedMaxIterations,
							tokenLimit: parsedTokenLimit,
						});
			toasts.success(
				debugWorkflow
					? `${t('page.executions.taskStarted')} (debug run)`
					: t('page.executions.taskStarted'),
				executionId,
			);
			if (onstarted) {
				onstarted(executionId);
				return;
			}
			await goto(resolve('/executions/[id]', { id: executionId }));
		} catch (e) {
			toasts.error(
				t('page.executions.taskStartFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			starting = false;
		}
	}

	onMount(() => {
		if (initialVersion) version = initialVersion;
		void loadDefinitions();
	});
</script>

<Card title={t('page.executions.taskTitle')}>
	{#if loading}
		<Skeleton lines={4} />
	{:else if loadError}
		<ErrorState
			title={t('page.executions.taskLoadFailed')}
			description={loadError}
			onretry={() => void loadDefinitions()}
		/>
	{:else if definitions.length === 0}
		<p class="text-caption text-muted-foreground">
			{t('page.executions.taskNoDefinitions')}
		</p>
	{:else}
		<div class="space-y-3">
			<Select
				bind:value={executionType}
				options={executionTypeOptions}
				label="Execution type"
				onchange={() => {
					const first = filteredDefinitions[0];
					selected = first ? `${first.kind}:${first.id}` : '';
				}}
			/>
			<Select
				bind:value={selected}
				{options}
				label={t('page.executions.taskDefinition')}
				onchange={() => undefined}
			/>
			{#if filteredDefinitions.length === 0}
				<p class="text-caption text-muted-foreground">
					No {executionType} definitions available.
				</p>
			{:else if selectedDefinition?.kind === 'workflow'}
				<Select
					bind:value={version}
					options={versions.length > 0 ? versions : [{ value: '', label: 'Latest' }]}
					label="Version"
					placeholder={versionsLoading ? 'Loading versions…' : 'Empty runs the current definition'}
					disabled={versionsLoading}
					onchange={() => undefined}
				/>
				<Textarea
					bind:value={input}
					label={t('page.executions.taskInput')}
					placeholder={t('page.executions.taskInputPlaceholder')}
				/>
				{#if input.trim() !== ''}
					<div class="rounded-md border border-border bg-muted/40 p-2">
						<p class="mb-1 text-micro text-muted-foreground">Input preview</p>
						<JsonViewer value={parsedInput} collapsed />
					</div>
				{/if}
				<Textarea
					bind:value={fileScope}
					label="File scope (JSON, optional)"
					placeholder="Optional JSON file scope"
				/>
				{#if fileScopeError}
					<p class="text-micro text-destructive">{fileScopeError}</p>
				{/if}
				<label class="flex cursor-pointer items-start gap-2 text-caption">
					<input type="checkbox" bind:checked={debugRun} class="mt-1 h-3.5 w-3.5 accent-current" />
					<span>
						Debug run of a subworkflow
						<span class="block text-micro text-muted-foreground">
							Subworkflows normally run via their parent; direct runs are for debugging only.
						</span>
					</span>
				</label>
				{#if debugRun}
					<p
						class="rounded-md border border-warning/40 bg-warning/10 px-2 py-1.5 text-caption text-muted-foreground"
					>
						This direct run is a debug run. Prefer triggering the parent workflow so lineage stays
						intact.
					</p>
				{/if}
			{:else}
				<Textarea
					bind:value={message}
					label={t('page.executions.taskMessage')}
					placeholder={t('page.executions.taskMessagePlaceholder')}
				/>
				<Select
					bind:value={model}
					options={modelOptions}
					label={t('page.executions.taskModel')}
					placeholder={t('page.executions.taskModelPlaceholder')}
				/>
				<Textarea
					bind:value={fileScope}
					label="File scope (JSON, optional)"
					placeholder="Optional JSON file scope"
				/>
				{#if fileScopeError}
					<p class="text-micro text-destructive">{fileScopeError}</p>
				{/if}
				<Textarea
					bind:value={conversationSeed}
					label="Conversation seed (JSON array, optional)"
					placeholder="Optional prior turns as a JSON array"
				/>
				{#if conversationSeedError}
					<p class="text-micro text-destructive">{conversationSeedError}</p>
				{/if}
				<div class="grid grid-cols-2 gap-2">
					<Input
						bind:value={maxIterations}
						label="Max iterations (optional)"
						placeholder="Empty uses template default"
					/>
					<Input
						bind:value={tokenLimit}
						label="Token limit (optional)"
						placeholder="Empty uses template default"
					/>
				</div>
			{/if}
			<Button size="sm" disabled={starting} onclick={() => void start()}>
				<Icon name="play" size={13} />
				{starting ? t('page.executions.taskStarting') : t('page.executions.taskStart')}
			</Button>
		</div>
	{/if}
</Card>
