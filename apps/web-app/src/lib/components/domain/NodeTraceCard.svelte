<script lang="ts">
	import type {
		LlmReasoningStep,
		NodeInputContext,
		NodeTrace,
	} from '#lib/types/models';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import JsonViewer from '#lib/ui/components/JsonViewer.svelte';
	import StreamMarkdown from '#lib/components/chat/LazyStreamMarkdown.svelte';
	import KeyValueList from './KeyValueList.svelte';
	import StatusBadge from '#lib/ui/components/StatusBadge.svelte';
	import { toneText } from '#lib/ui/components/variants';
	import { statusTone } from '#lib/ui/status';
	import { formatDateTime, formatDuration } from '#lib/utils/format';
	import { extractMarkdownText } from '#lib/utils/markdown';
	import { cn } from '#lib/ui/cn';
	import { t } from '#lib/i18n';

	interface Props {
		trace: NodeTrace;
		expanded?: boolean;
		highlighted?: boolean;
		detail?: NodeInputContext | null;
		reasoning?: LlmReasoningStep[] | null;
		loading?: boolean;
		ontoggle?: () => void;
		onlocate?: () => void;
	}

	let {
		trace,
		expanded = false,
		highlighted = false,
		detail = null,
		reasoning = null,
		loading = false,
		ontoggle,
		onlocate,
	}: Props = $props();

	const tone = $derived(toneText(statusTone(trace.status), 'text-foreground'));
	const title = $derived(trace.nodeName || trace.nodeId);
	const hasDetail = $derived(detail !== null || (reasoning?.length ?? 0) > 0);
	/** Reasoning only exists for LLM nodes; rendering the section empty would
	 * imply the backend returned nothing rather than "not an LLM node". */
	const isLlmNode = $derived(trace.nodeType.trim().toUpperCase() === 'LLM');
	/** String outputs read better as Markdown; structured values stay in the
	 * JSON viewer. Historical records always render in their final state. */
	const markdownOutput = $derived(extractMarkdownText(trace.output));
</script>

<article
	class={cn(
		'overflow-hidden rounded-lg border bg-card transition-colors',
		highlighted ? 'border-ring/60' : 'border-border',
	)}
>
	<button
		type="button"
		onclick={() => ontoggle?.()}
		aria-expanded={expanded}
		class="flex w-full items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-accent/50"
	>
		<Icon name="git-commit" size={14} class={cn('shrink-0', tone)} />
		<span class="min-w-0 flex-1">
			<span class="block truncate font-mono text-caption text-foreground"
				>{title}</span
			>
			<span class="block truncate text-micro text-muted-foreground">
				{trace.nodeType || t('domain.nodeTrace.unknownType')}
			</span>
		</span>
		{#if trace.retryCount > 0}
			<span
				class="flex shrink-0 items-center gap-1 text-micro tabular-nums text-warning"
				title={t('domain.nodeTrace.retries', { count: trace.retryCount })}
			>
				<Icon name="refresh" size={12} />
				{trace.retryCount}
			</span>
		{/if}
		<span class="shrink-0 text-micro tabular-nums text-muted-foreground">
			{trace.durationMs === null ? '—' : formatDuration(trace.durationMs)}
		</span>
		<StatusBadge status={trace.status} size="sm" />
		<Icon
			name="chevron-down"
			size={14}
			class={cn(
				'shrink-0 text-muted-foreground transition-transform duration-150',
				expanded && 'rotate-180',
			)}
		/>
	</button>

	{#if expanded}
		<div
			class="animate-panel-in space-y-2.5 border-t border-border px-3 py-2.5"
		>
			<dl class="grid grid-cols-2 gap-x-3 gap-y-1.5">
				<div>
					<dt class="text-micro text-muted-foreground">
						{t('domain.inspector.started')}
					</dt>
					<dd class="text-caption tabular-nums">
						{trace.startedAt ? formatDateTime(trace.startedAt) : '—'}
					</dd>
				</div>
				<div>
					<dt class="text-micro text-muted-foreground">
						{t('domain.nodeTrace.ended')}
					</dt>
					<dd class="text-caption tabular-nums">
						{trace.endedAt ? formatDateTime(trace.endedAt) : '—'}
					</dd>
				</div>
			</dl>

			{#if trace.error}
				<div
					class="rounded-md border border-destructive/25 bg-destructive/10 px-2 py-1.5"
				>
					<p
						class="flex items-center gap-1.5 text-micro font-medium text-destructive"
					>
						<Icon name="alert-triangle" size={12} />
						{t('domain.nodeTrace.failure')}
					</p>
					<p
						class="mt-0.5 font-mono text-micro break-words whitespace-pre-wrap text-foreground"
					>
						{trace.error}
					</p>
				</div>
			{/if}

			<div>
				<p
					class="mb-1 text-micro uppercase tracking-wide text-muted-foreground"
				>
					{t('domain.nodeTrace.input')}
				</p>
				<JsonViewer value={trace.input} collapsed />
			</div>
			<div>
				<p
					class="mb-1 text-micro uppercase tracking-wide text-muted-foreground"
				>
					{t('domain.nodeTrace.output')}
				</p>
				{#if markdownOutput !== null}
					<StreamMarkdown content={markdownOutput} done />
				{:else}
					<JsonViewer value={trace.output} collapsed />
				{/if}
			</div>

			{#if trace.toolDependencies.length > 0}
				<div>
					<p
						class="mb-1 text-micro uppercase tracking-wide text-muted-foreground"
					>
						{t('domain.nodeTrace.toolDependencies')}
					</p>
					<ul class="flex flex-wrap gap-1.5">
						{#each trace.toolDependencies as dependency (dependency.toolName)}
							<li
								class="rounded-full border border-border px-2 py-0.5 font-mono text-micro text-foreground"
							>
								{dependency.toolName}
								<span class="tabular-nums text-muted-foreground"
									>×{dependency.callCount}</span
								>
							</li>
						{/each}
					</ul>
				</div>
			{/if}

			{#if loading}
				<p class="text-micro text-muted-foreground">
					{t('common.loading')}
				</p>
			{:else if detail}
				<div>
					<p
						class="mb-1 text-micro uppercase tracking-wide text-muted-foreground"
					>
						{t('domain.nodeTrace.inputParameters')}
					</p>
					<KeyValueList items={detail.inputParameters} dense />
				</div>
				{#if detail.availableVariables.length > 0}
					<div>
						<p
							class="mb-1 text-micro uppercase tracking-wide text-muted-foreground"
						>
							{t('domain.nodeTrace.variablesInScope')}
						</p>
						<ul class="space-y-1">
							{#each detail.availableVariables as variable (variable.name)}
								<li class="flex items-start justify-between gap-2 text-micro">
									<span class="truncate font-mono text-foreground"
										>{variable.name}</span
									>
									<span
										class="min-w-0 flex-1 truncate text-right text-muted-foreground"
										title={variable.value}>{variable.value}</span
									>
								</li>
							{/each}
						</ul>
					</div>
				{/if}
			{:else if !hasDetail}
				<p class="text-micro text-muted-foreground">
					{t('domain.nodeTrace.noContext')}
				</p>
			{/if}

			{#if isLlmNode && reasoning && reasoning.length > 0}
				<div>
					<p
						class="mb-1 text-micro uppercase tracking-wide text-muted-foreground"
					>
						{t('domain.nodeTrace.reasoning')}
					</p>
					<ol class="space-y-1.5">
						{#each reasoning as step (step.stepId)}
							<li class="rounded-md bg-muted px-2 py-1.5">
								<p
									class="flex items-center gap-1.5 text-micro text-muted-foreground"
								>
									<span class="font-mono text-foreground">{step.type}</span>
									{#if step.confidence !== null}
										<span class="tabular-nums"
											>{step.confidence.toFixed(2)}</span
										>
									{/if}
								</p>
								<StreamMarkdown content={step.content} done />
								{#each step.conclusions as conclusion, index (index)}
									<p class="mt-0.5 text-micro text-muted-foreground">
										· {conclusion}
									</p>
								{/each}
							</li>
						{/each}
					</ol>
				</div>
			{/if}

			<div class="flex items-center gap-2">
				<Button variant="ghost" size="sm" onclick={() => onlocate?.()}>
					<Icon name="zoom-in" size={13} />
					{t('domain.inspector.locateOnGraph')}
				</Button>
			</div>
		</div>
	{/if}
</article>
