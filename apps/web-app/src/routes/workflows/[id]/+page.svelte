<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate, goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Segmented from '@wf-agent/ui/components/Segmented.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import GraphExplorer, {
		type GraphOverlay,
	} from '$lib/components/domain/GraphExplorer.svelte';
	import WorkflowEditPanel from '$lib/components/domain/WorkflowEditPanel.svelte';
	import WorkflowVersionsPanel from '$lib/components/domain/WorkflowVersionsPanel.svelte';
	import WorkflowDraftsPanel from '$lib/components/domain/WorkflowDraftsPanel.svelte';
	import WorkflowRunsPanel from '$lib/components/domain/WorkflowRunsPanel.svelte';
	import UnsavedChangesDialog from '@wf-agent/ui/components/UnsavedChangesDialog.svelte';
	import {
		executeWorkflow,
		exportWorkflow,
		getWorkflowDetail,
	} from '$lib/services/workflows';
	import {
		getGraphAnalysis,
		getGraphNeighbors,
		promoteWorkflowDraft,
		saveWorkflowDraft,
		validateWorkflowDraft,
	} from '$lib/services/graph';
	import type { GraphAnalysisResult, WorkflowDetail } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatNumber } from '$lib/utils/format';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';
	import type { DisplayEdge, DisplayNode } from '$lib/graph/display-model';
	import { GraphEditStore } from '$lib/graph/edit-store.svelte';
	import { WorkflowLockStore } from '$lib/stores/workflow-lock.svelte';
	import {
		allocateGraphNodeId,
		serverTemplateIssues,
		type TemplateIssue,
	} from '$lib/services/templates';
	import { nodeInsertStore } from '$lib/stores/node-insert.svelte';
	import type { CanvasPosition } from '$lib/components/domain/GraphCanvas.svelte';
	import type { ValidationIssue } from '$lib/types/models';

	const TABS = [
		{ id: 'graph', label: 'Graph' },
		{ id: 'edit', label: 'Edit' },
		{ id: 'versions', label: 'Versions' },
		{ id: 'drafts', label: 'Drafts' },
		{ id: 'runs', label: 'Runs' },
	];

	const requestedTab = parseListParams(page.url).tab;
	let tab = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab)
			? requestedTab
			: 'graph',
	);

	$effect(() => {
		gotoWithParams(page.url, { tab: tab === 'graph' ? '' : tab });
	});

	let detail = $state<WorkflowDetail | null>(null);
	let detailError = $state<string | null>(null);
	let detailLoading = $state(true);

	let graphNodeId = $state<string | null>(null);
	let analysis = $state<GraphAnalysisResult | null>(null);
	let analysisError = $state<string | null>(null);
	let activeOverlay = $state<string | null>(null);

	// Controlled canvas edit state. The canvas only emits intents; every
	// mutation lands in this store and re-renders from it.
	const editStore = new GraphEditStore();
	const lockStore = new WorkflowLockStore();
	let editSeededId = $state('');
	let editMode = $state(false);
	let editBusy = $state(false);
	let editDraftId = $state<string | null>(null);
	let editIssues = $state<ValidationIssue[]>([]);

	let unsavedOpen = $state(false);
	let unsavedBusy = $state(false);
	let pendingTab = $state<string | null>(null);
	let pendingNavUrl = $state<string | null>(null);

	const editTemplateIssues = $derived<TemplateIssue[]>(
		serverTemplateIssues(editIssues, editStore.nodes),
	);

	$effect(() => {
		if (tab !== 'edit' || !detail) return;
		lockStore.watch(detail.id);
		if (editSeededId === detail.id) return;
		editStore.load(nodes, edges);
		editSeededId = detail.id;
		editDraftId = null;
		editIssues = [];
		editMode = false;
		// A node queued from the template library lands as the first edit so
		// the user sees it without re-picking the template manually.
		const pendingInsert = nodeInsertStore.consume();
		if (pendingInsert) {
			handleAddNode({ x: 0, y: 0 }, pendingInsert.nodeType, pendingInsert.name);
		}
	});

	// A lost lease keeps the canvas read-only with a standing notice;
	// dirty content stays so it can be saved after re-acquiring.
	$effect(() => {
		if (editMode && lockStore.lockLost) {
			if (editStore.dirty) {
				toasts.warning(
					'Edit lock lost with unsaved changes',
					lockStore.lockLostBy
						? `Held by ${lockStore.lockLostBy}. Canvas kept your edits read-only; re-acquire the lock to save.`
						: 'The lease expired. Canvas kept your edits read-only; re-acquire the lock to save.',
				);
			} else {
				toasts.warning(
					'Edit lock lost',
					lockStore.lockLostBy
						? `Held by ${lockStore.lockLostBy}. Canvas is read-only.`
						: 'The lease expired. Canvas is read-only.',
				);
			}
		}
	});

	async function enterEdit(): Promise<void> {
		const ok = await lockStore.acquire();
		if (!ok) {
			toasts.warning(
				'Workflow is being edited',
				`Held by ${lockStore.displayHolder}.`,
			);
			return;
		}
		editMode = true;
	}

	function exitEdit(): void {
		editMode = false;
		lockStore.acknowledgeLockLoss();
		void lockStore.release();
	}

	/** Unsaved canvas edits block leaving the edit tab or the page. */
	function editDirty(): boolean {
		return tab === 'edit' && editMode && editStore.dirty;
	}

	function requestTab(id: string): void {
		if (id === tab) return;
		if (editDirty()) {
			pendingTab = id;
			pendingNavUrl = null;
			unsavedOpen = true;
			return;
		}
		tab = id;
	}

	function proceedPending(): void {
		if (pendingNavUrl) {
			const target = pendingNavUrl;
			pendingNavUrl = null;
			pendingTab = null;
			// The target replays an intercepted navigation URL, which resolve() cannot rebuild.
			// eslint-disable-next-line svelte/no-navigation-without-resolve
			void goto(target);
		} else if (pendingTab) {
			tab = pendingTab;
			pendingTab = null;
		}
	}

	function discardWorkflowEdits(): void {
		unsavedOpen = false;
		editStore.markClean();
		exitEdit();
		proceedPending();
	}

	async function saveAndProceed(): Promise<void> {
		unsavedBusy = true;
		try {
			await saveEditDraft();
			if (editStore.dirty) return;
			unsavedOpen = false;
			proceedPending();
		} finally {
			unsavedBusy = false;
		}
	}

	// Leaving the edit tab releases the lease; re-entering re-acquires.
	// Release failures only surface as a notice and never block navigation.
	$effect(() => {
		if (tab !== 'edit' && editMode) exitEdit();
	});

	function handleMoveNode(id: string, position: CanvasPosition): void {
		editStore.applyMove(id, position);
	}

	function handleMoveNodes(
		moves: Array<{ id: string; position: CanvasPosition }>,
	): void {
		editStore.applyMoves(moves);
	}

	function handleDeleteGroups(ids: string[]): void {
		for (const id of ids) editStore.removeGroup(id);
		toasts.info(
			'Groups deleted',
			`${ids.length} group(s) removed from the canvas.`,
		);
	}

	/** Jump from the graph detail card to the node row in the Nodes list. */
	function jumpToNodeRow(id: string): void {
		graphNodeId = id;
		requestAnimationFrame(() => {
			document
				.getElementById(`wf-node-${id}`)
				?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
		});
	}

	function handleAddNode(
		position: CanvasPosition,
		nodeType: string,
		name: string | null,
	): void {
		const existing = new Set(editStore.nodes.map((node) => node.id));
		const id = allocateGraphNodeId(existing);
		const label = name?.trim() || id;
		editStore.addNode({ id, label, kind: nodeType }, position);
		editStore.selectedId = id;
		toasts.success(
			`Node ${id} added`,
			`${nodeType}. Save the draft to keep it.`,
		);
	}

	function handleDeleteEdge(id: string): void {
		editStore.removeEdge(id);
	}

	function handleConnect(source: string, target: string): void {
		if (
			editStore.edges.some(
				(edge) => edge.source === source && edge.target === target,
			)
		) {
			return;
		}
		const reason = editStore.connect(source, target);
		if (reason) {
			toasts.info('Cannot connect', reason);
			return;
		}
		editStore.selectedId = target;
		toasts.success(
			`Edge ${source} → ${target} added`,
			'Save the draft to keep it.',
		);
	}

	function handleDeleteNodes(ids: string[]): void {
		editStore.removeNodes(ids);
		toasts.info(
			'Nodes deleted',
			`${ids.length} node(s) removed from the canvas.`,
		);
	}

	async function saveEditDraft(): Promise<void> {
		const id = page.params.id;
		if (!id || !detail) return;
		if (!lockStore.canWrite) {
			toasts.warning('Save blocked', `Held by ${lockStore.displayHolder}.`);
			return;
		}
		editBusy = true;
		try {
			const savedId = await saveWorkflowDraft(
				editStore.toDraftDefinition(id, detail.name),
			);
			editDraftId = savedId;
			editStore.markClean();
			await validateEditDraft(savedId);
			toasts.success('Draft saved', `Draft ${savedId} updated.`);
		} catch (e) {
			toasts.error('Save failed', e instanceof Error ? e.message : undefined);
		} finally {
			editBusy = false;
		}
	}

	async function validateEditDraft(draftId?: string): Promise<void> {
		const target = draftId ?? editDraftId;
		if (!target) {
			toasts.info('Nothing to validate', 'Save the draft first.');
			return;
		}
		try {
			editIssues = await validateWorkflowDraft(target);
			if (editIssues.length === 0) {
				toasts.success('Draft is valid');
			} else {
				toasts.warning(`Draft has ${editIssues.length} issue(s)`);
			}
		} catch (e) {
			toasts.error(
				'Validation failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function promoteEditDraft(): Promise<void> {
		if (!lockStore.canWrite) {
			toasts.warning(
				'Promotion blocked',
				`Held by ${lockStore.displayHolder}.`,
			);
			return;
		}
		if (!editDraftId) {
			toasts.info('Nothing to promote', 'Save the draft first.');
			return;
		}
		if (editStore.dirty) {
			toasts.error('Unsaved changes', 'Save the draft before promoting.');
			return;
		}
		try {
			const report = await promoteWorkflowDraft(editDraftId);
			toasts.success(
				`Draft promoted (${report.passCount} passed, ${report.warningCount} warnings)`,
			);
			const id = page.params.id;
			editSeededId = '';
			if (id) await load(id);
		} catch (e) {
			toasts.error(
				'Promotion failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	const workflow = $derived(detail);

	const nodes = $derived<DisplayNode[]>(
		(detail?.graph.nodes ?? []).map((node) => ({
			id: node.id,
			label: node.label,
			kind: node.kind,
			status: node.status,
		})),
	);

	const edges = $derived<DisplayEdge[]>(
		(detail?.graph.edges ?? []).map((edge) => ({
			id: edge.id,
			source: edge.from,
			target: edge.to,
			label: edge.label,
			kind: edge.kind,
			taken: edge.taken,
		})),
	);

	const overlays = $derived.by<GraphOverlay[]>(() => {
		if (!analysis) return [];
		const list: GraphOverlay[] = [];
		if (analysis.cycleDetection.hasCycle) {
			list.push({
				id: 'cycles',
				label: 'Cycle nodes',
				ids: analysis.cycleDetection.cycleNodes,
			});
		}
		if (analysis.reachability.unreachableNodes.length > 0) {
			list.push({
				id: 'unreachable',
				label: 'Unreachable',
				ids: analysis.reachability.unreachableNodes,
			});
		}
		if (analysis.reachability.deadEndNodes.length > 0) {
			list.push({
				id: 'dead-ends',
				label: 'Dead ends',
				ids: analysis.reachability.deadEndNodes,
			});
		}
		return list;
	});

	async function load(id: string): Promise<void> {
		detailLoading = true;
		detailError = null;
		try {
			detail = await getWorkflowDetail(id);
			graphNodeId = null;
			activeOverlay = null;
			void loadAnalysis(id);
		} catch (e) {
			detailError = e instanceof Error ? e.message : 'Failed to load workflow.';
			detail = null;
		} finally {
			detailLoading = false;
		}
	}

	async function loadAnalysis(id: string): Promise<void> {
		analysis = null;
		analysisError = null;
		try {
			analysis = await getGraphAnalysis(id);
		} catch (e) {
			analysisError =
				e instanceof Error ? e.message : 'Analysis failed to load.';
		}
	}

	async function expandNeighborhood(id: string): Promise<void> {
		const workflowId = page.params.id;
		if (!workflowId) return;
		try {
			const neighbors = await getGraphNeighbors(workflowId, id);
			neighborhoodIds = [
				id,
				...neighbors.predecessors,
				...neighbors.successors,
			];
			activeOverlay = '__neighborhood';
			toasts.success(
				`Neighborhood: ${neighbors.predecessors.length} in · ${neighbors.successors.length} out`,
			);
		} catch (e) {
			toasts.error(
				'Neighborhood failed to load',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	let neighborhoodIds = $state<string[]>([]);

	const effectiveOverlays = $derived.by<GraphOverlay[]>(() => {
		if (activeOverlay === '__neighborhood') {
			return [
				...overlays,
				{ id: '__neighborhood', label: 'Neighborhood', ids: neighborhoodIds },
			];
		}
		return overlays;
	});

	async function runPromote(draftId: string): Promise<void> {
		try {
			const report = await promoteWorkflowDraft(draftId);
			toasts.success(
				`Draft promoted (${report.passCount} passed, ${report.warningCount} warnings)`,
			);
			const id = page.params.id;
			if (id) await load(id);
		} catch (e) {
			toasts.error(
				'Promotion failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function runValidate(draftId: string): Promise<void> {
		try {
			const issues = await validateWorkflowDraft(draftId);
			if (detail) {
				detail = {
					...detail,
					drafts: detail.drafts.map((draft) =>
						draft.id === draftId
							? {
									...draft,
									valid: issues.length === 0,
									issues: issues.map(
										(issue) => `${issue.field}: ${issue.message}`,
									),
								}
							: draft,
					),
				};
			}
			toasts.success(
				issues.length === 0
					? 'Draft is valid'
					: `Draft has ${issues.length} issue(s)`,
			);
		} catch (e) {
			toasts.error(
				'Validation failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function runExport(id: string): Promise<void> {
		try {
			await exportWorkflow(id);
			toasts.success('Workflow definition exported');
		} catch (e) {
			toasts.error('Export failed', e instanceof Error ? e.message : undefined);
		}
	}

	async function runExecute(id: string): Promise<void> {
		try {
			const executionId = await executeWorkflow(id);
			toasts.success('Execution started');
			await goto(resolve('/executions/[id]', { id: executionId }));
		} catch (e) {
			toasts.error(
				'Execution failed to start',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	onMount(() => {
		const id = page.params.id;
		if (!id) return;
		void load(id);
		// Best-effort lease release on page hide or unload. Failures never
		// block navigation; the store surfaces them as a notice. Reloads
		// with unsaved canvas edits keep the native leave prompt.
		const release = (): void => {
			void lockStore.release();
		};
		const guardUnload = (event: BeforeUnloadEvent): void => {
			if (editDirty()) event.preventDefault();
		};
		window.addEventListener('pagehide', release);
		window.addEventListener('beforeunload', release);
		window.addEventListener('beforeunload', guardUnload);
		return () => {
			window.removeEventListener('pagehide', release);
			window.removeEventListener('beforeunload', release);
			window.removeEventListener('beforeunload', guardUnload);
			void lockStore.release();
		};
	});

	beforeNavigate((navigation) => {
		if (navigation.willUnload) return;
		if (!editDirty()) return;
		navigation.cancel();
		pendingNavUrl = navigation.to?.url.toString() ?? null;
		pendingTab = null;
		unsavedOpen = true;
	});

	$effect(() => {
		const id = page.params.id;
		if (!id) return;
		if (detail && detail.id === id) return;
		void load(id);
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={workflow?.name ?? 'Workflow detail'}
		description={workflow?.description ?? ''}
	>
		{#snippet meta()}
			{#if workflow}
				<StatusBadge status={workflow.status} />
				<Badge variant="outline">v{workflow.version}</Badge>
				<span class="font-mono text-caption text-muted-foreground"
					>{workflow.id}</span
				>
				<span class="text-caption text-muted-foreground">
					{formatNumber(workflow.nodeCount)} nodes · {formatNumber(
						workflow.edgeCount,
					)} edges
				</span>
			{/if}
		{/snippet}
		{#snippet actions()}
			<IconButton
				icon="pencil"
				label="Edit graph"
				onclick={() => (tab = 'edit')}
			/>
			<Button
				variant="outline"
				size="sm"
				onclick={() => {
					const id = page.params.id;
					if (id) void runExport(id);
				}}
			>
				<Icon name="download" size={13} />
				Export
			</Button>
			<Button
				size="sm"
				onclick={() => {
					const id = page.params.id;
					if (id) void runExecute(id);
				}}
			>
				<Icon name="play" size={13} />
				Run
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented
		items={TABS}
		value={tab}
		onchange={(id) => requestTab(id)}
		class="px-4"
	/>

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if detailLoading}
			<Skeleton lines={6} class="rounded-lg border border-border bg-card p-4" />
		{:else if detailError || !detail}
			<ErrorState
				title="Workflow failed to load"
				description={detailError ?? 'Unknown error.'}
				onretry={() => {
					const id = page.params.id;
					if (id) void load(id);
				}}
				class="rounded-lg border border-border bg-card"
			/>
		{:else if tab === 'graph'}
			{#if analysisError}
				<p class="mb-2 text-caption text-warning">
					Graph analysis unavailable ({analysisError});
					<button
						type="button"
						class="underline"
						onclick={() => {
							const id = page.params.id;
							if (id) void loadAnalysis(id);
						}}
					>
						retry
					</button>.
				</p>
			{/if}
			<GraphExplorer
				{nodes}
				{edges}
				preset="workflow"
				selectedId={graphNodeId}
				onselect={(id) => (graphNodeId = id)}
				onexpand={(id) => void expandNeighborhood(id)}
				expandLabel="Reveal neighborhood"
				overlays={effectiveOverlays}
				{activeOverlay}
				onoverlay={(id) => (activeOverlay = id)}
				onjumpparam={jumpToNodeRow}
			/>
			<div class="mt-3 grid gap-3 lg:grid-cols-2">
				<Card title="Nodes">
					<ul class="space-y-1.5">
						{#each detail.graph.nodes as node (node.id)}
							<li
								id={`wf-node-${node.id}`}
								class="flex items-center justify-between gap-2 text-caption"
							>
								<button
									type="button"
									class="truncate font-mono underline-offset-2 hover:underline"
									onclick={() => (graphNodeId = node.id)}
								>
									{node.id}
								</button>
								<span class="flex shrink-0 items-center gap-2">
									<span class="text-muted-foreground">{node.kind}</span>
									<StatusBadge status={node.status} size="sm" dot={false} />
								</span>
							</li>
						{:else}
							<li class="text-caption text-muted-foreground">
								This definition has no nodes yet.
							</li>
						{/each}
					</ul>
				</Card>
				<Card title="Edges">
					<ul class="space-y-1.5">
						{#each detail.graph.edges as edge (edge.id)}
							<li class="flex items-center gap-2 text-caption">
								<span class="font-mono">{edge.from}</span>
								<Icon
									name="arrow-right"
									size={12}
									class="text-muted-foreground"
								/>
								<span class="font-mono">{edge.to}</span>
								{#if edge.label}
									<Badge variant="outline" class="text-[0.625rem]"
										>{edge.label}</Badge
									>
								{/if}
							</li>
						{:else}
							<li class="text-caption text-muted-foreground">
								This definition has no edges yet.
							</li>
						{/each}
					</ul>
				</Card>
			</div>
		{:else if tab === 'edit'}
			<WorkflowEditPanel
				store={editStore}
				lock={lockStore}
				{editMode}
				{editBusy}
				issues={editTemplateIssues}
				onenteredit={() => void enterEdit()}
				onexitedit={exitEdit}
				onrelock={() => void enterEdit()}
				onsave={() => void saveEditDraft()}
				onvalidate={() => void validateEditDraft()}
				onpromote={() => void promoteEditDraft()}
				onmovenode={handleMoveNode}
				onmovenodes={handleMoveNodes}
				onaddnode={handleAddNode}
				ondeleteedge={handleDeleteEdge}
				onconnect={handleConnect}
				ondeletenodes={handleDeleteNodes}
				ondeletegroups={handleDeleteGroups}
			/>
		{:else if tab === 'versions'}
			{#if detail}
				<WorkflowVersionsPanel
					workflowId={detail.id}
					versions={detail.versions}
					{nodes}
					{edges}
					onchanged={() => {
						const id = page.params.id;
						if (id) void load(id);
					}}
				/>
			{/if}
		{:else if tab === 'drafts'}
			<WorkflowDraftsPanel
				drafts={detail.drafts}
				onpromote={(draftId) => void runPromote(draftId)}
				onvalidate={(draftId) => void runValidate(draftId)}
			/>
		{:else}
			<WorkflowRunsPanel workflowId={detail.id} />
		{/if}
	</div>
</div>

<UnsavedChangesDialog
	bind:open={unsavedOpen}
	description="The edit canvas has unsaved draft changes. Leaving discards the canvas edits."
	saveLabel="Save draft & leave"
	busy={unsavedBusy}
	ondiscard={discardWorkflowEdits}
	onsave={() => void saveAndProceed()}
/>
