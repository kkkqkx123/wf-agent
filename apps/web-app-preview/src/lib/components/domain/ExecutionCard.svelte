<script lang="ts">
	import type { Execution } from '#lib/types/models';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import Progress from '@wf-agent/ui/components/Progress.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import { statusTone } from '@wf-agent/ui/status';
	import {
		formatDuration,
		formatRelativeTime,
		shortId,
	} from '#lib/utils/format';
	import { isErrorStatus } from '@wf-agent/ui/status';
	import { toasts } from '#lib/stores/toast.svelte';
	import { cn } from '@wf-agent/ui/cn';
	import { t } from '#lib/i18n';

	interface Props {
		execution: Execution;
		selected?: boolean;
		class?: string;
		onselect?: (execution: Execution) => void;
		onopen?: (execution: Execution) => void;
		onrerun?: (execution: Execution) => void;
		rerunning?: boolean;
	}

	let {
		execution,
		selected = false,
		class: className = '',
		onselect,
		onopen,
		onrerun,
		rerunning = false,
	}: Props = $props();

	const tone = $derived(statusTone(execution.status));
	const progressTone = $derived(
		tone === 'danger'
			? 'danger'
			: tone === 'success'
				? 'success'
				: tone === 'running'
					? 'running'
					: 'default',
	);
	const canRerun = $derived(
		execution.kind === undefined || execution.kind === 'workflow',
	);
	const rowError = $derived(isErrorStatus(execution.status));

	async function copyId(): Promise<void> {
		try {
			await navigator.clipboard.writeText(execution.id);
			toasts.success(t('common.copied'), shortId(execution.id, 12));
		} catch {
			toasts.error(
				t('domain.graph.copyFailed'),
				t('domain.graph.copyFailedDetail'),
			);
		}
	}
</script>

<div
	data-execution-id={execution.id}
	class={cn(
		'group rounded-lg border px-3 py-2.5 transition-colors duration-150',
		selected
			? 'border-[hsl(var(--ring))] bg-accent'
			: rowError
				? 'border-destructive/40 bg-destructive/[0.04] hover:border-destructive/60 hover:bg-destructive/[0.07]'
				: 'border-border bg-card hover:border-ring/40 hover:bg-accent/40',
		className,
	)}
>
	<button
		type="button"
		onclick={() => onselect?.(execution)}
		class="w-full text-left"
		aria-pressed={selected}
	>
		<div class="flex items-center justify-between gap-2">
			<span class="truncate text-body font-medium text-foreground"
				>{execution.workflowName}</span
			>
			<span class="flex shrink-0 items-center gap-1.5">
				{#if execution.kind}
					<span
						class="rounded border border-border px-1 py-px font-mono text-micro text-muted-foreground"
						>{execution.kind}</span
					>
				{/if}
				<StatusBadge status={execution.status} size="sm" />
			</span>
		</div>

		<div class="mt-1 flex items-center gap-2 text-micro text-muted-foreground">
			<span class="font-mono">{shortId(execution.id, 12)}</span>
			<span aria-hidden="true">·</span>
			<span>{formatRelativeTime(execution.startedAt)}</span>
			{#if execution.durationMs !== null}
				<span aria-hidden="true">·</span>
				<span class="tabular-nums">{formatDuration(execution.durationMs)}</span>
			{/if}
		</div>

		<div class="mt-2">
			<Progress value={execution.progress} tone={progressTone} />
		</div>

		<div
			class="mt-1.5 flex items-center justify-between gap-2 text-micro text-muted-foreground"
		>
			<span class="tabular-nums">
				{t('domain.executionCard.tasks', {
					done: execution.tasksDone,
					total: execution.tasksTotal,
				})}
			</span>
			<span class="flex min-w-0 items-center gap-1.5">
				{#if execution.failedNodes > 0}
					<span class="flex items-center gap-1 text-destructive">
						<Icon name="alert-triangle" size={11} />
						{t('domain.executionCard.failed', { count: execution.failedNodes })}
					</span>
				{/if}
				{#if execution.currentNode}
					<span class="truncate font-mono">{execution.currentNode}</span>
				{/if}
			</span>
		</div>
	</button>

	<div
		class="mt-2 flex items-center gap-1 border-t border-border pt-2 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
	>
		{#if onopen}
			<IconButton
				icon="arrow-right"
				label={t('page.executions.openDetail')}
				compact
				onclick={() => onopen?.(execution)}
			/>
		{/if}
		{#if canRerun && onrerun}
			<IconButton
				icon="refresh"
				label={t('domain.runs.rerun')}
				compact
				disabled={rerunning}
				onclick={() => onrerun?.(execution)}
			/>
		{/if}
		<IconButton
			icon="copy"
			label={t('common.copy')}
			compact
			onclick={() => void copyId()}
		/>
	</div>
</div>
