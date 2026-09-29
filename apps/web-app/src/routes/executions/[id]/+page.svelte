<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import ExecutionInspector from '$lib/components/domain/ExecutionInspector.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import {
		cancelExecution,
		getExecutionDetail,
		pauseExecution,
		resumeExecution,
	} from '$lib/services/executions';
	import type { ExecutionDetail } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDuration } from '$lib/utils/format';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';

	let execution = $state<ExecutionDetail | null>(null);
	let loadError = $state<string | null>(null);
	let controlBusy = $state(false);
	let cancelArmed = $state(false);

	const TAB_IDS = [
		'overview',
		'graph',
		'timeline',
		'tools',
		'analysis',
		'state',
	];
	const requestedTab = parseListParams(page.url).tab;
	let tab = $state(
		requestedTab && TAB_IDS.includes(requestedTab) ? requestedTab : 'overview',
	);

	$effect(() => {
		gotoWithParams(page.url, { tab: tab === 'overview' ? '' : tab });
	});

	async function load(id: string): Promise<void> {
		loadError = null;
		try {
			execution = await getExecutionDetail(id);
		} catch (e) {
			loadError = e instanceof Error ? e.message : 'Failed to load execution.';
			execution = null;
		}
	}

	async function runControl(
		label: string,
		action: (id: string) => Promise<void>,
	): Promise<void> {
		const id = page.params.id;
		if (!id) return;
		controlBusy = true;
		try {
			await action(id);
			toasts.success(`${label} done`);
			await load(id);
		} catch (e) {
			toasts.error(
				`${label} failed`,
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			controlBusy = false;
		}
	}

	onMount(() => {
		const id = page.params.id;
		if (!id) return;
		void load(id);
	});

	$effect(() => {
		const id = page.params.id;
		if (!id) return;
		if (execution && execution.id === id) return;
		void load(id);
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={execution?.workflowName ?? 'Execution detail'}
		description="Full execution detail with state, timeline and analysis."
	>
		{#snippet meta()}
			{#if execution}
				<StatusBadge status={execution.status} />
				<Badge variant="outline">{formatDuration(execution.durationMs)}</Badge>
				<span class="font-mono text-caption text-muted-foreground"
					>{execution.id}</span
				>
				{#if execution.trigger}
					<span class="text-caption text-muted-foreground"
						>{execution.trigger}</span
					>
				{/if}
			{/if}
		{/snippet}
		{#snippet actions()}
			<IconButton
				icon="pause"
				label="Pause execution"
				disabled={controlBusy}
				onclick={() => void runControl('Pause', pauseExecution)}
			/>
			<IconButton
				icon="play"
				label="Resume execution"
				disabled={controlBusy}
				onclick={() => void runControl('Resume', resumeExecution)}
			/>
			{#if cancelArmed}
				<Button
					variant="outline"
					size="sm"
					disabled={controlBusy}
					onclick={() => {
						cancelArmed = false;
						void runControl('Cancel', cancelExecution);
					}}
				>
					Confirm cancel
				</Button>
				<Button variant="ghost" size="sm" onclick={() => (cancelArmed = false)}>
					Keep
				</Button>
			{:else}
				<Button
					variant="outline"
					size="sm"
					onclick={() => (cancelArmed = true)}
				>
					<Icon name="square" size={13} />
					Cancel
				</Button>
			{/if}
			<Button variant="outline" size="sm" href="/executions">
				<Icon name="arrow-left" size={13} />
				Back to workbench
			</Button>
		{/snippet}
	</PageHeader>

	<div class="min-h-0 flex-1 overflow-hidden px-4 pb-4">
		{#if loadError && !execution}
			<ErrorState
				title="Execution failed to load"
				description={loadError}
				onretry={() => {
					const id = page.params.id;
					if (id) void load(id);
				}}
				class="rounded-lg border border-border bg-card"
			/>
		{:else if !execution}
			<Skeleton
				lines={6}
				class="h-full rounded-lg border border-border bg-card p-4"
			/>
		{:else}
			<div
				class="h-full overflow-hidden rounded-lg border border-border bg-card"
			>
				<ExecutionInspector {execution} bind:tab />
			</div>
		{/if}
	</div>
</div>
