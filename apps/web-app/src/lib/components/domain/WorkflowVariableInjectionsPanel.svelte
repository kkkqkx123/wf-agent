<script lang="ts">
	import { SvelteSet } from 'svelte/reactivity';
	import Card from '#lib/ui/components/Card.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import Textarea from '#lib/ui/components/Textarea.svelte';
	import Select from '#lib/ui/components/Select.svelte';
	import ErrorState from '#lib/ui/components/ErrorState.svelte';
	import { toasts } from '#lib/stores/toast.svelte';
	import {
		listVariableInjections,
		removeVariableInjection,
		submitVariableInjection,
		type VariableInjection,
		type VariableInjectionPolicy,
	} from '#lib/services/executions';
	import { formatDateTime } from '#lib/utils/format';
	import { t } from '#lib/i18n';

	interface Props {
		executionId: string;
		class?: string;
	}

	let { executionId, class: className = '' }: Props = $props();

	let injections = $state<VariableInjection[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let seen = $state('');

	let variableName = $state('');
	let valueText = $state('');
	let source = $state('web');
	let policy = $state<VariableInjectionPolicy>('next_boundary');
	let submitting = $state(false);
	let removing = new SvelteSet<string>();

	$effect(() => {
		const id = executionId;
		if (seen === id) return;
		seen = id;
		void load(id);
	});

	async function load(id: string): Promise<void> {
		loading = true;
		error = null;
		try {
			injections = await listVariableInjections(id);
		} catch (e) {
			seen = '';
			injections = [];
			error = e instanceof Error ? e.message : t('domain.variableInjections.failed');
		} finally {
			loading = false;
		}
	}

	function parseValue(text: string): unknown {
		const trimmed = text.trim();
		if (trimmed === '') return null;
		try {
			return JSON.parse(trimmed);
		} catch {
			return trimmed;
		}
	}

	async function submit(): Promise<void> {
		if (submitting) return;
		const name = variableName.trim();
		if (!name) {
			toasts.error(t('domain.variableInjections.nameRequired'));
			return;
		}
		submitting = true;
		try {
			await submitVariableInjection(executionId, {
				variableName: name,
				value: parseValue(valueText),
				source: source.trim() || 'web',
				policy,
			});
			toasts.success(t('domain.variableInjections.submitted'));
			variableName = '';
			valueText = '';
			await load(executionId);
		} catch (e) {
			toasts.error(
				t('domain.variableInjections.submitFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			submitting = false;
		}
	}

	async function remove(injectionId: string): Promise<void> {
		if (removing.has(injectionId)) return;
		removing.add(injectionId);
		try {
			await removeVariableInjection(executionId, injectionId);
			toasts.success(t('domain.variableInjections.removed'));
			await load(executionId);
		} catch (e) {
			toasts.error(
				t('domain.variableInjections.removeFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			removing.delete(injectionId);
		}
	}

	function policyLabel(value: string): string {
		return value === 'on_resume'
			? t('domain.variableInjections.policyOnResume')
			: t('domain.variableInjections.policyNextBoundary');
	}
</script>

<Card
	title={t('domain.variableInjections.title')}
	description={t('domain.variableInjections.description')}
	class={className}
>
	{#if error}
		<ErrorState
			title={t('domain.variableInjections.failedTitle')}
			description={error}
			onretry={() => void load(executionId)}
			class="rounded-lg border border-border bg-card"
		/>
	{:else}
		<div class="space-y-3">
			<form
				class="space-y-2 rounded-md border border-border bg-muted/30 p-3"
				onsubmit={(event) => {
					event.preventDefault();
					void submit();
				}}
			>
				<div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
					<Input
						bind:value={variableName}
						placeholder={t('domain.variableInjections.namePlaceholder')}
						size="sm"
						aria-label={t('domain.variableInjections.name')}
					/>
					<Input
						bind:value={source}
						placeholder={t('domain.variableInjections.sourcePlaceholder')}
						size="sm"
						aria-label={t('domain.variableInjections.source')}
					/>
				</div>
				<Textarea
					bind:value={valueText}
					placeholder={t('domain.variableInjections.valuePlaceholder')}
					class="min-h-16 text-caption"
					aria-label={t('domain.variableInjections.value')}
				/>
				<div class="flex items-center justify-between gap-2">
					<Select
						bind:value={policy}
						options={[
							{ value: 'next_boundary', label: t('domain.variableInjections.policyNextBoundary') },
							{ value: 'on_resume', label: t('domain.variableInjections.policyOnResume') },
						]}
						size="sm"
						class="w-44"
					/>
					<Button type="submit" size="sm" disabled={submitting || loading}>
						{submitting
							? t('domain.variableInjections.submitting')
							: t('domain.variableInjections.submit')}
					</Button>
				</div>
			</form>

			{#if loading && injections.length === 0}
				<p class="text-caption text-muted-foreground">{t('domain.variableInjections.loading')}</p>
			{:else if injections.length === 0}
				<p class="text-caption text-muted-foreground">{t('domain.variableInjections.empty')}</p>
			{:else}
				<ul class="space-y-2">
					{#each injections as injection (injection.injectionId)}
						<li
							class="flex items-start justify-between gap-2 rounded-md border border-border bg-card p-2"
						>
							<div class="min-w-0 flex-1">
								<p class="truncate font-mono text-caption text-foreground">
									{injection.variableName}
									<span class="ml-1 text-micro text-muted-foreground">
										· {policyLabel(injection.policy)}
									</span>
								</p>
								<p class="mt-0.5 break-words text-micro text-muted-foreground">
									{JSON.stringify(injection.value)}
								</p>
								<p class="mt-0.5 text-micro text-muted-foreground">
									{injection.source} · {formatDateTime(injection.submittedAt)}
								</p>
							</div>
							<Button
								variant="ghost"
								size="sm"
								disabled={removing.has(injection.injectionId)}
								onclick={() => void remove(injection.injectionId)}
							>
								{removing.has(injection.injectionId)
									? t('domain.variableInjections.removing')
									: t('domain.variableInjections.remove')}
							</Button>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	{/if}
</Card>
