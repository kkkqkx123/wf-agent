<script lang="ts">
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import GraphExplorer from '$lib/components/domain/GraphExplorer.svelte';
	import IssueList from '$lib/components/domain/IssueList.svelte';
	import type {
		CanvasPosition,
		PresenceCursor,
	} from '$lib/components/domain/GraphCanvas.svelte';
	import { GraphEditStore } from '$lib/graph/edit-store.svelte';
	import { WorkflowLockStore } from '$lib/stores/workflow-lock.svelte';
	import type { TemplateIssue } from '$lib/services/templates';

	interface Props {
		store: GraphEditStore;
		lock: WorkflowLockStore;
		editMode: boolean;
		editBusy: boolean;
		issues: TemplateIssue[];
		onenteredit: () => void;
		onexitedit: () => void;
		onrelock: () => void;
		onsave: () => void;
		onvalidate: () => void;
		onpromote: () => void;
		onmovenode: (id: string, position: CanvasPosition) => void;
		onmovenodes: (
			moves: Array<{ id: string; position: CanvasPosition }>,
		) => void;
		onaddnode: (
			position: CanvasPosition,
			nodeType: string,
			name: string | null,
		) => void;
		ondeleteedge: (id: string) => void;
		onconnect: (source: string, target: string) => void;
		ondeletenodes: (ids: string[]) => void;
		ondeletegroups: (ids: string[]) => void;
		/** Remote cursors rendered over the canvas. */
		presence?: PresenceCursor[];
		/** Local pointer position in canvas-wrapper coordinates. */
		oncursormove?: (position: CanvasPosition) => void;
	}

	let {
		store,
		lock,
		editMode,
		editBusy,
		issues,
		onenteredit,
		onexitedit,
		onrelock,
		onsave,
		onvalidate,
		onpromote,
		onmovenode,
		onmovenodes,
		onaddnode,
		ondeleteedge,
		onconnect,
		ondeletenodes,
		ondeletegroups,
		presence = [],
		oncursormove,
	}: Props = $props();

	let explorer = $state<{ focus: (id: string) => void } | null>(null);

	const issueIds = $derived(
		issues.map((issue) => issue.nodeId).filter((id) => id !== null),
	);
</script>

<div class="mb-2 flex flex-wrap items-center gap-2 text-caption">
	{#if !lock.supported}
		<Badge variant="warning">No lock protection</Badge>
	{:else if lock.held}
		<Badge variant="success">Editing · you hold the lock</Badge>
	{:else if lock.lockedByOther}
		<Badge variant="danger">Read-only · held by {lock.displayHolder}</Badge>
	{:else}
		<Badge variant="outline">Unlocked</Badge>
	{/if}
	{#if lock.refreshError}
		<span class="text-micro text-muted-foreground"
			>Lock query failed; saving is disabled.</span
		>
	{/if}
</div>
{#if editMode && lock.lockLost}
	<Card title="Edit lock lost" class="mb-2 border-destructive/40">
		<p class="text-caption text-muted-foreground">
			{#if lock.lockLostBy}
				Held by {lock.lockLostBy}.
			{:else}
				The lease expired.
			{/if}
			The canvas is read-only and your edits are kept. Re-acquire the lock to continue
			editing, or exit edit mode.
		</p>
		{#snippet footer()}
			<div class="flex items-center gap-2">
				<Button size="sm" onclick={() => onrelock()}>Re-acquire lock</Button>
				<Button variant="ghost" size="sm" onclick={() => onexitedit()}>
					Exit edit mode
				</Button>
			</div>
		{/snippet}
	</Card>
{/if}
{#if issues.length > 0}
	<Card title="Validation issues" class="mb-2">
		<IssueList
			{issues}
			onlocate={(issue) => {
				if (issue.nodeId) explorer?.focus(issue.nodeId);
			}}
		/>
	</Card>
{/if}
<GraphExplorer
	bind:this={explorer}
	nodes={store.nodes}
	edges={store.edges}
	preset="workflow"
	selectedId={store.selectedId}
	onselect={(id) => (store.selectedId = id)}
	editable
	editMode={editMode && lock.canWrite}
	editDirty={store.dirty}
	canUndo={store.canUndo}
	canRedo={store.canRedo}
	{editBusy}
	positions={store.positions}
	{issueIds}
	onenteredit={() => onenteredit()}
	onexitedit={() => onexitedit()}
	onundo={() => store.undo()}
	onredo={() => store.redo()}
	onsave={() => onsave()}
	onvalidate={() => onvalidate()}
	onpromote={() => onpromote()}
	onmovenode={(id, position) => onmovenode(id, position)}
	onmovenodes={(moves) => onmovenodes(moves)}
	onaddnode={(position, nodeType, name) => onaddnode(position, nodeType, name)}
	ondeleteedge={(id) => ondeleteedge(id)}
	onconnect={(source, target) => onconnect(source, target)}
	ondeletenodes={(ids) => ondeletenodes(ids)}
	ondeletegroups={(ids) => ondeletegroups(ids)}
	{presence}
	{oncursormove}
/>
