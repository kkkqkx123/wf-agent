<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import JsonViewer from '@wf-agent/ui/components/JsonViewer.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import { getWorkflowDraftTopology } from '#lib/services/graph';
	import type { DisplayEdge, DisplayNode } from '#lib/graph/display-model';
	import type { DefinitionDraft, DefinitionKind } from '#lib/types/models';
	import { formatDateTime } from '#lib/utils/format';
	import { t } from '#lib/i18n';

	interface Props {
		/** The kind of definition the drafts belong to. */
		kind: DefinitionKind;
		drafts: DefinitionDraft[];
		onpromote: (draftId: string) => void;
		onvalidate: (draftId: string) => void;
	}

	let { kind, drafts, onpromote, onvalidate }: Props = $props();

	let previewId = $state<string | null>(null);
	let previewNodes = $state<DisplayNode[]>([]);
	let previewEdges = $state<DisplayEdge[]>([]);
	let previewError = $state<string | null>(null);
	let previewLoading = $state(false);
	let definitionId = $state<string | null>(null);

	// Only a workflow has a graph to preview; every kind can show the
	// definition it carries.
	const hasGraph = $derived(kind === 'workflow');

	function toggleDefinition(draftId: string): void {
		definitionId = definitionId === draftId ? null : draftId;
	}

	async function togglePreview(draftId: string): Promise<void> {
		if (previewId === draftId) {
			previewId = null;
			return;
		}
		previewId = draftId;
		previewNodes = [];
		previewEdges = [];
		previewError = null;
		previewLoading = true;
		try {
			const topology = await getWorkflowDraftTopology(draftId);
			previewNodes = topology.nodes.map((node) => ({
				id: node.id,
				label: node.label,
				kind: node.kind,
			}));
			previewEdges = topology.edges.map((edge) => ({
				id: edge.id,
				source: edge.from,
				target: edge.to,
				label: edge.label,
				kind: edge.kind,
			}));
		} catch (e) {
			previewError =
				e instanceof Error ? e.message : t('domain.drafts.previewFailed');
		} finally {
			previewLoading = false;
		}
	}
</script>

<div class="space-y-2">
	{#each drafts as draft (draft.id)}
		<Card title={draft.name}>
			{#snippet actions()}
				<StatusBadge status={draft.valid ? 'completed' : 'failed'} size="sm" />
			{/snippet}
			<p class="text-caption text-muted-foreground">
				{t('domain.drafts.updated')}
				{#if draft.updatedAt}
					· <span class="tabular-nums">{formatDateTime(draft.updatedAt)}</span>
				{/if}
			</p>
			{#if draft.issues.length > 0}
				<ul class="mt-2 space-y-1">
					{#each draft.issues as issue, index (index)}
						<li class="flex items-start gap-1.5 text-caption text-destructive">
							<Icon name="alert-circle" size={12} class="mt-0.5 shrink-0" />
							<span
								>{issue.field
									? `${issue.field}: ${issue.message}`
									: issue.message}</span
							>
						</li>
					{/each}
				</ul>
			{/if}
			{#snippet footer()}
				<div class="flex items-center gap-2">
					<Button size="sm" onclick={() => onpromote(draft.id)}
						>{t('domain.graph.promote')}</Button
					>
					<Button
						variant="ghost"
						size="sm"
						onclick={() => onvalidate(draft.id)}
					>
						{t('domain.graph.validate')}
					</Button>
					{#if hasGraph}
						<Button
							variant="ghost"
							size="sm"
							onclick={() => void togglePreview(draft.id)}
						>
							{previewId === draft.id
								? t('domain.drafts.hideGraph')
								: t('domain.drafts.previewGraph')}
						</Button>
					{/if}
					<Button
						variant="ghost"
						size="sm"
						onclick={() => toggleDefinition(draft.id)}
					>
						{definitionId === draft.id
							? t('domain.drafts.hideDefinition')
							: t('domain.drafts.viewDefinition')}
					</Button>
				</div>
			{/snippet}
			{#if previewId === draft.id}
				<div class="mt-2">
					{#if previewLoading}
						<Skeleton lines={4} />
					{:else if previewError}
						<ErrorState
							title={t('domain.drafts.previewFailedTitle')}
							description={previewError}
							onretry={() => void togglePreview(draft.id)}
						/>
					{:else}
						{#await import('#lib/components/domain/GraphExplorer.svelte') then module}
							<module.default
								nodes={previewNodes}
								edges={previewEdges}
								preset="workflow"
							/>
						{/await}
					{/if}
				</div>
			{/if}
			{#if definitionId === draft.id}
				<div class="mt-2">
					<JsonViewer value={draft.definition} />
				</div>
			{/if}
		</Card>
	{:else}
		<EmptyState
			icon="file"
			title={t('domain.drafts.emptyTitle')}
			description={t('domain.drafts.emptyDescription')}
			class="rounded-lg border border-border bg-card"
		/>
	{/each}
</div>
