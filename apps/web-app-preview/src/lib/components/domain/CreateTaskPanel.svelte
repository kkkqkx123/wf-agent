<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Select from '@wf-agent/ui/components/Select.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import Textarea from '@wf-agent/ui/components/Textarea.svelte';
	import { listWorkflows } from '#lib/services/workflows';
	import { listTemplates } from '#lib/services/templates';
	import { listModelProfiles } from '#lib/services/resources';
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

	let {
		onstarted,
		initialDefinition = null,
		initialVersion = null,
	}: Props = $props();

	let definitions = $state<RunnableDefinition[]>([]);
	let modelOptions = $state<{ value: string; label: string }[]>([]);
	let loading = $state(true);
	let loadError = $state<string | null>(null);
	let selected = $state('');
	let message = $state('');
	let model = $state('');
	let input = $state('');
	let version = $state('');
	let starting = $state(false);

	const options = $derived(
		definitions.map((definition) => ({
			value: `${definition.kind}:${definition.id}`,
			label: `${definition.name} · ${definition.kind}`,
		})),
	);

	const selectedDefinition = $derived(
		definitions.find(
			(definition) => `${definition.kind}:${definition.id}` === selected,
		) ?? null,
	);

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
			const preferred =
				profiles.find((profile) => profile.isDefault) ?? profiles[0];
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
						(definition) =>
							`${definition.kind}:${definition.id}` === initialDefinition,
					)
						? initialDefinition
						: `${definitions[0].kind}:${definitions[0].id}`;
				selected = preselected;
			}
		} catch (e) {
			loadError =
				e instanceof Error ? e.message : t('page.executions.taskLoadFailed');
		} finally {
			loading = false;
		}
	}

	async function start(): Promise<void> {
		const definition = selectedDefinition;
		if (!definition || starting) return;
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
			const executionId =
				definition.kind === 'workflow'
					? await runDefinition('workflow', definition.id, {
							input: input.trim() === '' ? null : input,
							version: version.trim() === '' ? null : version.trim(),
						})
					: await runDefinition('agent', definition.id, {
							message: message.trim(),
							model,
						});
			toasts.success(t('page.executions.taskStarted'), executionId);
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
				bind:value={selected}
				{options}
				label={t('page.executions.taskDefinition')}
				onchange={() => undefined}
			/>
			{#if selectedDefinition?.kind === 'workflow'}
				<Input
					bind:value={version}
					label="Version"
					placeholder="Empty runs the current definition"
				/>
				<Textarea
					bind:value={input}
					label={t('page.executions.taskInput')}
					placeholder={t('page.executions.taskInputPlaceholder')}
				/>
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
			{/if}
			<Button size="sm" disabled={starting} onclick={() => void start()}>
				<Icon name="play" size={13} />
				{starting
					? t('page.executions.taskStarting')
					: t('page.executions.taskStart')}
			</Button>
		</div>
	{/if}
</Card>
