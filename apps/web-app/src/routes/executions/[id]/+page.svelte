<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';
	import ExecutionInspector from '#lib/components/domain/ExecutionInspector.svelte';
	import SessionInspector from '#lib/components/domain/SessionInspector.svelte';
	import { isExecutionTab, type ExecutionTab } from '#lib/config/execution-tabs';
	import StatusBadge from '#lib/ui/components/StatusBadge.svelte';
	import {
		cancelExecution,
		getUnifiedExecutionDetail,
		pauseExecution,
		resumeExecution,
		type UnifiedExecutionDetail,
	} from '#lib/services/executions';
	import { rerunWorkflowRun } from '#lib/services/run';
	import { cancelAgentLoop, pauseAgentLoop, resumeAgentLoop } from '#lib/services/agent-loops';
	import type { Execution } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { gotoWithParams, parseListParams } from '#lib/utils/route';
	import { t } from '#lib/i18n';

	let execution = $state<Execution | null>(null);
	let unified = $state<UnifiedExecutionDetail | null>(null);
	let loadError = $state<string | null>(null);
	let loading = $state(false);
	let controlBusy = $state(false);
	let cancelArmed = $state(false);
	let rerunning = $state(false);

	const requestedTab = parseListParams(page.url as unknown as URL).tab ?? null;
	let tab = $state<ExecutionTab>(isExecutionTab(requestedTab) ? requestedTab : 'overview');

	$effect(() => {
		gotoWithParams(page.url as unknown as URL, {
			tab: tab === 'overview' ? '' : tab,
		});
	});

	async function load(id: string): Promise<void> {
		loadError = null;
		loading = true;
		try {
			// Unified detail only: the inspector reads one cross-engine shape.
			unified = await getUnifiedExecutionDetail(id);
			const record =
				typeof unified.record === 'object' && unified.record !== null
					? (unified.record as Record<string, unknown>)
					: {};
			const completed = ['completed', 'succeeded', 'success'].includes(
				unified.status.toLowerCase(),
			);
			execution = {
				id: unified.executionId,
				workflowId: unified.definitionId ?? '',
				workflowName: unified.definitionId ?? unified.executionId,
				kind: unified.executionType,
				status: unified.status,
				startedAt: unified.startTime,
				endedAt: unified.endTime,
				durationMs: null,
				progress: completed ? 1 : 0,
				currentNode: typeof record.current_node_id === 'string' ? record.current_node_id : null,
				trigger: null,
				tasksTotal: 0,
				tasksDone: completed ? 1 : 0,
				failedNodes: 0,
				memoryPeakBytes: null,
				version: unified.version,
				error: unified.error,
			};
		} catch (e) {
			loadError = e instanceof Error ? e.message : t('page.executionDetail.errorTitle');
			execution = null;
			unified = null;
		} finally {
			loading = false;
		}
	}

	// Each engine exposes its own pause/resume/cancel endpoints; pick per kind.
	// Chat sessions are terminal transcripts with no live controls.
	function controlsFor(kind: UnifiedExecutionDetail['executionType']): {
		pause: (id: string) => Promise<void>;
		resume: (id: string) => Promise<void>;
		cancel: (id: string) => Promise<void>;
	} | null {
		if (kind === 'agent') {
			return {
				pause: pauseAgentLoop,
				resume: resumeAgentLoop,
				cancel: cancelAgentLoop,
			};
		}
		if (kind === 'workflow') {
			return {
				pause: pauseExecution,
				resume: resumeExecution,
				cancel: cancelExecution,
			};
		}
		return null;
	}

	async function runControl(label: string, action: (id: string) => Promise<void>): Promise<void> {
		const id = page.params.id;
		if (!id) return;
		controlBusy = true;
		try {
			await action(id);
			toasts.success(t('page.executionDetail.done', { label }));
			await load(id);
		} catch (e) {
			toasts.error(
				t('page.executionDetail.failed', { label }),
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

	async function copyExecutionId(id: string): Promise<void> {
		try {
			await navigator.clipboard.writeText(id);
			toasts.success(t('common.copied'), id.slice(0, 12));
		} catch {
			toasts.error(t('page.executionDetail.copyFailed'));
		}
	}

	async function rerunCurrent(id: string): Promise<void> {
		if (rerunning) return;
		rerunning = true;
		try {
			const nextId = await rerunWorkflowRun(id);
			toasts.success(t('domain.runs.rerunStarted'), nextId);
			await goto(resolve('/executions/[id]', { id: nextId }));
		} catch (e) {
			toasts.error(t('domain.runs.rerunFailed'), e instanceof Error ? e.message : undefined);
		} finally {
			rerunning = false;
		}
	}

	$effect(() => {
		const id = page.params.id;
		if (!id) return;
		if (unified && unified.executionId === id) return;
		void load(id);
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={execution?.workflowName ?? t('page.executionDetail.titleFallback')}
		description={t('page.executionDetail.description')}
	>
		{#snippet meta()}
			{#if execution && unified}
				<StatusBadge status={execution.status} size="sm" />
				<Badge variant="outline">{unified.executionType}</Badge>
				{#if unified.version}
					<Badge variant="outline">v{unified.version}</Badge>
				{/if}
				<span class="font-mono text-caption text-muted-foreground">{execution.id}</span>
				{#if unified.rerunOf}
					<a
						class="font-mono text-caption text-muted-foreground underline-offset-2 hover:underline"
						href={resolve('/executions/[id]', { id: unified.rerunOf })}
						title="Rerun source run"
					>
						rerun of {unified.rerunOf.slice(0, 8)}
					</a>
				{/if}
				{#if unified.debug}
					<Badge variant="outline">debug</Badge>
				{/if}
				{#if unified.parentExecutionId}
					<a
						class="font-mono text-caption text-muted-foreground underline-offset-2 hover:underline"
						href={resolve('/executions/[id]', { id: unified.parentExecutionId })}
						title="Parent run"
					>
						child of {unified.parentExecutionId.slice(0, 8)}
					</a>
				{/if}
			{/if}
		{/snippet}
		{#snippet actions()}
			{@const controls = unified ? controlsFor(unified.executionType) : null}
			{@const currentId = page.params.id}
			{#if controls}
				<IconButton
					icon="pause"
					label={t('page.executionDetail.pause')}
					disabled={controlBusy}
					onclick={() => void runControl(t('page.executionDetail.pause'), controls.pause)}
				/>
				<IconButton
					icon="play"
					label={t('page.executionDetail.resume')}
					disabled={controlBusy}
					onclick={() => void runControl(t('page.executionDetail.resume'), controls.resume)}
				/>
				{#if cancelArmed}
					<Button
						variant="outline"
						size="sm"
						disabled={controlBusy}
						onclick={() => {
							cancelArmed = false;
							void runControl(t('page.executionDetail.cancel'), controls.cancel);
						}}
					>
						{t('page.executionDetail.cancelArmed')}
					</Button>
					<Button variant="ghost" size="sm" onclick={() => (cancelArmed = false)}>
						{t('page.executionDetail.keep')}
					</Button>
				{:else}
					<Button variant="outline" size="sm" onclick={() => (cancelArmed = true)}>
						<Icon name="square" size={13} />
						{t('page.executionDetail.cancel')}
					</Button>
				{/if}
			{/if}
			{#if unified && currentId}
				{#if unified.executionType === 'workflow'}
					<IconButton
						icon="refresh"
						label={t('domain.runs.rerun')}
						disabled={rerunning}
						onclick={() => void rerunCurrent(currentId)}
					/>
				{:else}
					<IconButton
						icon="refresh"
						label={unified.executionType === 'agent'
							? 'Agent rerun is not supported yet'
							: 'Chat transcripts are read-only and cannot rerun'}
						disabled={true}
						onclick={() => {}}
					/>
				{/if}
			{/if}
			{#if currentId}
				<IconButton
					icon="copy"
					label={t('common.copy')}
					onclick={() => void copyExecutionId(currentId)}
				/>
			{/if}
			{#if unified?.executionType === 'workflow' && currentId && unified.definitionId}
				<Button
					variant="outline"
					size="sm"
					href={`/workflows/${encodeURIComponent(unified.definitionId.replace(/^workflow:/, ''))}?tab=graph&run=${encodeURIComponent(currentId)}`}
					title="Project this run onto the definition graph (read-only)"
				>
					<Icon name="workflow" size={13} />
					{t('page.executionDetail.viewOnGraph')}
				</Button>
			{/if}
			<Button variant="outline" size="sm" href="/executions">
				<Icon name="arrow-left" size={13} />
				{t('page.executionDetail.backToWorkbench')}
			</Button>
		{/snippet}
	</PageHeader>

	<div class="min-h-0 flex-1 overflow-hidden px-4 pb-4">
		<PageState
			{loading}
			error={loadError}
			errorTitle={t('page.executionDetail.errorTitle')}
			empty={!execution && !unified}
			emptyTitle={t('page.executionDetail.notFound')}
			emptyDescription={t('page.executionDetail.notFoundDesc')}
			onretry={() => {
				const id = page.params.id;
				if (id) void load(id);
			}}
			class="h-full rounded-lg border border-border bg-card"
		>
			<div class="h-full overflow-hidden rounded-lg border border-border bg-card">
				{#if unified && (unified.executionType === 'agent' || unified.executionType === 'chat')}
					<SessionInspector sessionId={unified.executionId} />
				{:else if execution}
					<ExecutionInspector
						{execution}
						bind:tab
						onrefresh={() => {
							const id = page.params.id;
							if (id) void load(id);
						}}
					/>
				{/if}
			</div>
		</PageState>
	</div>
</div>
