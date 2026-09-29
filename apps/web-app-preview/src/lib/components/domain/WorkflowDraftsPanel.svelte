<script lang="ts">
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import StatusBadge from '$lib/components/ui/StatusBadge.svelte';
	import GraphExplorer from '$lib/components/domain/GraphExplorer.svelte';
	import JsonViewer from '$lib/components/ui/JsonViewer.svelte';
	import { getWorkflowDraftTopology } from '$lib/services/graph';
	import type { DisplayEdge, DisplayNode } from '$lib/graph/display-model';
	import type { WorkflowDraft } from '$lib/types/models';
	import { formatDateTime } from '$lib/utils/format';

	interface Props {
		drafts: WorkflowDraft[];
		onpromote: (draftId: string) => void;
		onvalidate: (draftId: string) => void;
	}

	let { drafts, onpromote, onvalidate }: Props = $props();

	let previewId = $state<string | null>(null);
	let previewNodes = $state<DisplayNode[]>([]);
	let previewEdges = $state<DisplayEdge[]>([]);
	let previewError = $state<string | null>(null);
	let previewLoading = $state(false);
	let definitionId = $state<string | null>(null);

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
			previewError = e instanceof Error ? e.message : 'Draft preview failed.';
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
				Updated {formatDateTime(draft.updatedAt)}
			</p>
			{#if draft.issues.length > 0}
				<ul class="mt-2 space-y-1">
					{#each draft.issues as issue, index (index)}
						<li class="flex items-start gap-1.5 text-caption text-destructive">
							<Icon name="alert-circle" size={12} class="mt-0.5 shrink-0" />
							<span>{issue}</span>
						</li>
					{/each}
				</ul>
			{/if}
			{#snippet footer()}
				<div class="flex items-center gap-2">
					<Button size="sm" onclick={() => onpromote(draft.id)}>Promote</Button>
					<Button
						variant="ghost"
						size="sm"
						onclick={() => onvalidate(draft.id)}
					>
						Validate
					</Button>
					<Button
						variant="ghost"
						size="sm"
						onclick={() => void togglePreview(draft.id)}
					>
						{previewId === draft.id ? 'Hide graph' : 'Preview graph'}
					</Button>
					<Button
						variant="ghost"
						size="sm"
						onclick={() => toggleDefinition(draft.id)}
					>
						{definitionId === draft.id ? 'Hide definition' : 'View definition'}
					</Button>
				</div>
			{/snippet}
			{#if previewId === draft.id}
				<div class="mt-2">
					{#if previewLoading}
						<Skeleton lines={4} />
					{:else if previewError}
						<ErrorState
							title="Draft preview failed to load"
							description={previewError}
							onretry={() => void togglePreview(draft.id)}
						/>
					{:else}
						<GraphExplorer
							nodes={previewNodes}
							edges={previewEdges}
							preset="workflow"
						/>
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
			title="No drafts"
			description="Editable drafts for this workspace appear here."
			class="rounded-lg border border-border bg-card"
		/>
	{/each}
</div>
