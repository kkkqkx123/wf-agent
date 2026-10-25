<script lang="ts">
	import type { Workflow } from '#lib/types/models';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import StatusBadge from '#lib/ui/components/StatusBadge.svelte';
	import { formatPercent, formatRelativeTime, shortId } from '#lib/utils/format';
	import { toasts } from '#lib/stores/toast.svelte';
	import { cn } from '#lib/ui/cn';
	import { t } from '#lib/i18n';

	interface Props {
		workflow: Workflow;
		selected?: boolean;
		class?: string;
		onselect?: (workflow: Workflow) => void;
		onopen?: (workflow: Workflow) => void;
	}

	let { workflow, selected = false, class: className = '', onselect, onopen }: Props = $props();

	async function copyId(): Promise<void> {
		try {
			await navigator.clipboard.writeText(workflow.id);
			toasts.success(t('common.copied'), shortId(workflow.id, 12));
		} catch {
			toasts.error(t('domain.graph.copyFailed'), t('domain.graph.copyFailedDetail'));
		}
	}
</script>

<div
	class={cn(
		'group flex h-full flex-col rounded-lg border px-3 py-2.5 text-left transition-colors duration-150',
		selected
			? 'border-[hsl(var(--ring))] bg-accent'
			: 'border-border bg-card hover:border-ring/40 hover:bg-accent/40',
		className,
	)}
>
	<button
		type="button"
		onclick={() => onselect?.(workflow)}
		class="w-full flex-1 text-left"
		aria-pressed={selected}
	>
		<div class="flex items-start justify-between gap-2">
			<h3 class="truncate text-body font-medium text-foreground">
				{workflow.name}
			</h3>
			<StatusBadge status={workflow.status} size="sm" />
		</div>

		<p class="mt-1 line-clamp-2 text-caption text-muted-foreground">
			{workflow.description}
		</p>

		<div class="mt-2 flex flex-wrap items-center gap-1">
			{#each workflow.tags.slice(0, 3) as tag (tag)}
				<Badge variant="outline" class="text-[0.625rem]">{tag}</Badge>
			{/each}
			{#if workflow.tags.length > 3}
				<span class="text-micro text-muted-foreground">+{workflow.tags.length - 3}</span>
			{/if}
		</div>

		<div
			class="mt-2.5 flex items-center justify-between gap-2 border-t border-border pt-2 text-micro text-muted-foreground"
		>
			<span class="flex items-center gap-1">
				<Icon name="workflow" size={11} />
				{t('domain.workflowCard.meta', {
					version: workflow.version,
					count: workflow.nodeCount,
				})}
			</span>
			<span class="flex items-center gap-2 tabular-nums">
				{#if workflow.successRate !== null}
					<span
						>{t('domain.workflowCard.ok', {
							rate: formatPercent(workflow.successRate, 0),
						})}</span
					>
				{/if}
				<span>{formatRelativeTime(workflow.updatedAt)}</span>
			</span>
		</div>
	</button>

	<div
		class="mt-1 flex items-center gap-1 border-t border-border pt-2 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"
	>
		{#if onopen}
			<IconButton
				icon="arrow-right"
				label={t('page.executions.openDetail')}
				compact
				onclick={() => onopen?.(workflow)}
			/>
		{/if}
		<IconButton icon="copy" label={t('common.copy')} compact onclick={() => void copyId()} />
	</div>
</div>
