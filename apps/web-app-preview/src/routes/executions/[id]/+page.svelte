<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';
	import ExecutionInspector from '#lib/components/domain/ExecutionInspector.svelte';
	import {
		isExecutionTab,
		type ExecutionTab,
	} from '#lib/config/execution-tabs';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import {
		cancelExecution,
		getExecutionDetail,
		pauseExecution,
		resumeExecution,
	} from '#lib/services/executions';
	import {
		cancelAgentLoop,
		pauseAgentLoop,
		resumeAgentLoop,
	} from '#lib/services/agent-loops';
	import type { ExecutionDetail } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { formatDuration } from '#lib/utils/format';
	import { gotoWithParams, parseListParams } from '#lib/utils/route';
	import { t } from '#lib/i18n';

	let execution = $state<ExecutionDetail | null>(null);
	let loadError = $state<string | null>(null);
	let loading = $state(false);
	let controlBusy = $state(false);
	let cancelArmed = $state(false);

	const requestedTab = parseListParams(page.url as unknown as URL).tab ?? null;
	let tab = $state<ExecutionTab>(
		isExecutionTab(requestedTab) ? requestedTab : 'overview',
	);

	$effect(() => {
		gotoWithParams(page.url as unknown as URL, { tab: tab === 'overview' ? '' : tab });
	});

	async function load(id: string): Promise<void> {
		loadError = null;
		loading = true;
		try {
			execution = await getExecutionDetail(id);
		} catch (e) {
			loadError = e instanceof Error ? e.message : t('page.executionDetail.errorTitle');
			execution = null;
		} finally {
			loading = false;
		}
	}

	// Each engine exposes its own pause/resume/cancel endpoints; pick per kind.
	function controlsFor(kind: ExecutionDetail['kind']): {
		pause: (id: string) => Promise<void>;
		resume: (id: string) => Promise<void>;
		cancel: (id: string) => Promise<void>;
	} {
		if (kind === 'agent_loop') {
			return {
				pause: pauseAgentLoop,
				resume: resumeAgentLoop,
				cancel: cancelAgentLoop,
			};
		}
		return {
			pause: pauseExecution,
			resume: resumeExecution,
			cancel: cancelExecution,
		};
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

	$effect(() => {
		const id = page.params.id;
		if (!id) return;
		if (execution && execution.id === id) return;
		void load(id);
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={execution?.workflowName ?? t('page.executionDetail.titleFallback')}
		description={t('page.executionDetail.description')}
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
			{#if execution}
				{@const controls = controlsFor(execution.kind)}
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
					<Button
						variant="ghost"
						size="sm"
						onclick={() => (cancelArmed = false)}
					>
						{t('page.executionDetail.keep')}
					</Button>
				{:else}
					<Button
						variant="outline"
						size="sm"
						onclick={() => (cancelArmed = true)}
					>
						<Icon name="square" size={13} />
						{t('page.executionDetail.cancel')}
					</Button>
				{/if}
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
			empty={!execution}
			emptyTitle={t('page.executionDetail.notFound')}
			emptyDescription={t('page.executionDetail.notFoundDesc')}
			onretry={() => {
				const id = page.params.id;
				if (id) void load(id);
			}}
			class="h-full rounded-lg border border-border bg-card"
		>
			<div
				class="h-full overflow-hidden rounded-lg border border-border bg-card"
			>
				{#if execution}
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
