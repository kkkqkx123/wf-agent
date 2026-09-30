<script lang="ts">
	import { tick } from 'svelte';
	import type { Snippet } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import Select from '@wf-agent/ui/components/Select.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import GraphCanvas, {
		type CanvasContext,
		type CanvasMove,
		type CanvasPosition,
		type PresenceCursor,
	} from '$lib/components/domain/GraphCanvas.svelte';
	import AddNodeDrawer from '$lib/components/domain/AddNodeDrawer.svelte';
	import ContextMenu, {
		type ContextMenuItem,
	} from '@wf-agent/ui/components/ContextMenu.svelte';
	import {
		deriveGroups,
		foldForCap,
		groupIdFromTitle,
		isGroupTitleId,
	} from '$lib/graph/group-view';
	import {
		columnPositions,
		layeredPositions,
		sortIdsByCanvasPosition,
	} from '$lib/graph/layout';
	import { registerCanvasShortcuts } from '$lib/graph/canvas-shortcuts';
	import {
		capGraph,
		distinctKinds,
		EDGE_LABEL_LIMIT,
		GRAPH_NODE_CAP,
		kindCounts,
		legendFor,
		type DisplayEdge,
		type DisplayNode,
		type GraphLayoutKind,
		type GraphPreset,
	} from '$lib/graph/display-model';
	import { toasts } from '$lib/stores/toast.svelte';
	import { preferences } from '$lib/stores/preferences.svelte';
	import { cn } from '@wf-agent/ui/cn';

	export interface GraphOverlay {
		id: string;
		label: string;
		ids: string[];
	}

	interface Props {
		nodes: DisplayNode[];
		edges: DisplayEdge[];
		preset: GraphPreset;
		loading?: boolean;
		error?: string | null;
		onretry?: () => void;
		selectedId?: string | null;
		onselect?: (id: string | null) => void;
		onexpand?: (id: string) => void;
		expandLabel?: string;
		overlays?: GraphOverlay[];
		activeOverlay?: string | null;
		onoverlay?: (id: string | null) => void;
		inspector?: Snippet;
		actions?: Snippet;
		/** Controlled edit mode; intents report back instead of mutating. */
		editable?: boolean;
		editMode?: boolean;
		editDirty?: boolean;
		canUndo?: boolean;
		canRedo?: boolean;
		editBusy?: boolean;
		positions?: Record<string, CanvasPosition>;
		issueIds?: string[];
		pulseIds?: string[];
		criticalIds?: string[];
		failedIds?: string[];
		heatTierById?: Record<string, number>;
		decisionIds?: string[];
		heatLabels?: Record<string, string>;
		decisionLabels?: Record<string, string>;
		onenteredit?: () => void;
		onexitedit?: () => void;
		onundo?: () => void;
		onredo?: () => void;
		onsave?: () => void;
		onvalidate?: () => void;
		onpromote?: () => void;
		onmovenode?: (id: string, position: CanvasPosition) => void;
		/** Batched move for group drags; undoes in a single step. */
		onmovenodes?: (moves: CanvasMove[]) => void;
		/** `nodeType` is normalised: a builtin name or a plugin-contributed one. */
		onaddnode?: (
			position: CanvasPosition,
			nodeType: string,
			name: string | null,
		) => void;
		ondeleteedge?: (id: string) => void;
		onconnect?: (source: string, target: string) => void;
		ondeletenodes?: (ids: string[]) => void;
		/** Whole-group delete for collapsed-title selection. */
		ondeletegroups?: (ids: string[]) => void;
		/** Jump from the detail card to the node definition. */
		onjumpparam?: (id: string) => void;
		/** Remote cursors rendered over the canvas; omitted for read-only views. */
		presence?: PresenceCursor[];
		/** Local pointer position in canvas-wrapper coordinates. */
		oncursormove?: (position: CanvasPosition) => void;
		class?: string;
	}

	let {
		nodes,
		edges,
		preset,
		loading = false,
		error = null,
		onretry,
		selectedId = null,
		onselect,
		onexpand,
		expandLabel = 'Load neighbors',
		overlays = [],
		activeOverlay = null,
		onoverlay,
		inspector,
		actions,
		editable = false,
		editMode = false,
		editDirty = false,
		canUndo = false,
		canRedo = false,
		editBusy = false,
		positions = undefined,
		issueIds = [],
		pulseIds = [],
		criticalIds = [],
		failedIds = [],
		heatTierById = {},
		decisionIds = [],
		heatLabels = {},
		decisionLabels = {},
		onenteredit,
		onexitedit,
		onundo,
		onredo,
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
		onjumpparam,
		presence = [],
		oncursormove,
		class: className = '',
	}: Props = $props();

	let canvas: GraphCanvas | null = $state(null);
	let explorerRoot: HTMLDivElement | null = $state(null);
	// Layout is chosen once at mount from the preset; users can change it
	// afterwards via the toolbar, so live-reactivity would clobber that.
	function initialLayout(): GraphLayoutKind {
		return preset === 'decision' ? 'columns' : 'layered';
	}
	let layout = $state<GraphLayoutKind>(initialLayout());
	let showFilters = $state(false);
	let query = $state('');
	let hiddenKinds = $state<string[]>([]);
	let collapsedIds = $state<string[]>([]);
	let matchIndex = $state(0);

	function cycleMinimap(): void {
		const current = preferences.minimapMode;
		preferences.setMinimapMode(
			current === 'auto' ? 'on' : current === 'on' ? 'off' : 'auto',
		);
	}

	const kinds = $derived(distinctKinds(nodes));
	const activeIds = $derived(
		new Set(
			overlays.find((overlay) => overlay.id === activeOverlay)?.ids ?? [],
		),
	);

	const filtered = $derived.by(() => {
		const hidden = new Set(hiddenKinds);
		const needle = query.trim().toLowerCase();
		const keptNodes = nodes.filter((node) => {
			if (hidden.has(node.kind || 'unknown')) return false;
			if (
				needle &&
				!node.label.toLowerCase().includes(needle) &&
				!node.id.toLowerCase().includes(needle) &&
				!(node.groupId ?? '').toLowerCase().includes(needle)
			) {
				return false;
			}
			return true;
		});
		const kept = new Set(keptNodes.map((node) => node.id));
		return {
			nodes: keptNodes,
			edges: edges.filter(
				(edge) => kept.has(edge.source) && kept.has(edge.target),
			),
		};
	});

	// Groups fold before the node cap: non-critical groups collapse first so
	// truncation drops structure last, not first.
	const groups = $derived(deriveGroups(filtered.nodes));
	const groupMemberCounts = $derived.by(() => {
		const counts: Record<string, number> = {};
		for (const node of filtered.nodes) {
			if (node.groupId) counts[node.groupId] = (counts[node.groupId] ?? 0) + 1;
		}
		return counts;
	});
	const protectedGroups = $derived.by(() => {
		const watched = [
			...(selectedId ? [selectedId] : []),
			...pulseIds,
			...criticalIds,
			...failedIds,
		];
		return new Set(
			filtered.nodes.flatMap((node) =>
				node.groupId && watched.includes(node.id) ? [node.groupId] : [],
			),
		);
	});
	const statusById = $derived.by(() => {
		const table: Record<string, string | undefined> = {};
		for (const node of filtered.nodes) table[node.id] = node.status;
		for (const id of failedIds) table[id] = 'failed';
		for (const id of pulseIds) table[id] = 'running';
		return table;
	});
	const folded = $derived(
		foldForCap(
			filtered.nodes,
			filtered.edges,
			new Set(collapsedIds),
			protectedGroups,
			GRAPH_NODE_CAP,
			{ statusById },
		),
	);
	const foldedView = $derived(folded.view);

	const capped = $derived(
		capGraph(foldedView.nodes, foldedView.edges, GRAPH_NODE_CAP, [
			...failedIds,
			...pulseIds,
			...criticalIds,
			...(selectedId ? [selectedId] : []),
		]),
	);

	const selected = $derived(
		capped.nodes.find((node) => node.id === selectedId) ??
			foldedView.nodes.find((node) => node.id === selectedId) ??
			null,
	);

	const selectedNeighbors = $derived.by(() => {
		if (!selectedId) return { predecessors: 0, successors: 0 };
		let predecessors = 0;
		let successors = 0;
		for (const edge of foldedView.edges) {
			if (edge.target === selectedId) predecessors += 1;
			if (edge.source === selectedId) successors += 1;
		}
		return { predecessors, successors };
	});

	// Hover text for the canvas: heat durations and decision branches
	// merged per node; a node carrying both joins them in one line.
	const mergedTooltipLabels = $derived.by(() => {
		const merged: Record<string, string> = { ...decisionLabels };
		for (const [id, text] of Object.entries(heatLabels)) {
			merged[id] = merged[id] ? `${merged[id]} · ${text}` : text;
		}
		return merged;
	});

	function toggleGroup(groupId: string): void {
		collapsedIds = collapsedIds.includes(groupId)
			? collapsedIds.filter((entry) => entry !== groupId)
			: [...collapsedIds, groupId];
	}

	function collapseAllGroups(): void {
		collapsedIds = groups.map((group) => group.id);
	}

	function expandAllGroups(): void {
		collapsedIds = [];
	}

	function focusGroup(titleId: string): void {
		const title = foldedView.titles[titleId];
		if (!title) return;
		if (folded.collapsed.has(title.groupId)) {
			void focus(titleId);
			return;
		}
		const visible = title.memberIds.filter((id) =>
			capped.nodes.some((node) => node.id === id),
		);
		if (visible.length === 0) {
			void focus(titleId);
			return;
		}
		canvas?.fitTo(visible);
	}

	async function copyText(text: string, label: string): Promise<void> {
		try {
			await navigator.clipboard.writeText(text);
			toasts.success(`${label} copied`);
		} catch {
			toasts.error('Copy failed', 'Clipboard is unavailable.');
		}
	}

	function isHiddenNode(id: string): boolean {
		return foldedView.hiddenIds.has(id);
	}

	function guardConnect(source: string, target: string): boolean {
		if (isHiddenNode(source) || isHiddenNode(target)) {
			toasts.info('Hidden group member', 'Expand the group before connecting.');
			return false;
		}
		if (isGroupTitleId(source) || isGroupTitleId(target)) {
			toasts.info(
				'Group title cannot connect',
				'Expand the group and connect a member.',
			);
			return false;
		}
		return true;
	}

	function guardMoves(moves: CanvasMove[]): CanvasMove[] {
		const visible = moves.filter((move) => !isHiddenNode(move.id));
		if (visible.length !== moves.length) {
			toasts.info(
				'Hidden group member',
				'Expand the group before moving hidden nodes.',
			);
		}
		return visible;
	}

	function handleExpand(id: string): void {
		// Double-clicking a group title folds back out instead of loading
		// neighborhoods; every other node keeps the existing behavior.
		const title = foldedView.titles[id];
		if (title) {
			toggleGroup(title.groupId);
			return;
		}
		onexpand?.(id);
	}

	function handleDeleteSelected(): void {
		const ids = canvas?.selectedNodeIds() ?? [];
		const groupIds = ids
			.filter((id) => isGroupTitleId(id))
			.map((id) => foldedView.titles[id]?.groupId ?? groupIdFromTitle(id))
			.filter(Boolean);
		const nodeIds = ids.filter((id) => !isGroupTitleId(id));
		if (groupIds.length > 0) ondeletegroups?.(groupIds);
		if (nodeIds.length > 0) ondeletenodes?.(nodeIds);
	}

	// Search-locate: every capped node already matches the query. Matches
	// follow canvas geometry left to right, top to bottom so stepping feels
	// stable instead of following input order.
	const matchIds = $derived.by(() => {
		const ids = capped.nodes.map((node) => node.id);
		const base =
			layout === 'columns'
				? columnPositions(capped.nodes)
				: layout === 'layered'
					? layeredPositions(capped.nodes, capped.edges)
					: new Map<string, { x: number; y: number }>();
		const resolved = new SvelteMap<string, { x: number; y: number }>();
		for (const id of ids) {
			const override = positions?.[id];
			const computed = base.get(id);
			if (override) resolved.set(id, override);
			else if (computed) resolved.set(id, computed);
		}
		return sortIdsByCanvasPosition(ids, resolved);
	});
	const matchLabel = $derived(
		matchIds.length === 0
			? 'No matches'
			: `${Math.min(matchIndex + 1, matchIds.length)} of ${matchIds.length}`,
	);

	$effect(() => {
		void query;
		void hiddenKinds;
		void capped.nodes;
		matchIndex = 0;
	});

	function stepMatch(delta: number): void {
		if (matchIds.length === 0) return;
		matchIndex = (matchIndex + delta + matchIds.length) % matchIds.length;
		const id = matchIds[matchIndex];
		if (id) void focus(id);
	}

	// Right-click menu: same actions as the toolbar, no new semantics.
	interface PendingMenu {
		kind: 'node' | 'edge' | 'group' | 'blank';
		id: string | null;
		x: number;
		y: number;
	}

	let contextMenu = $state<PendingMenu | null>(null);

	let addNodeOpen = $state(false);
	let pendingAddPosition = $state<CanvasPosition | null>(null);

	function openAddNode(position: CanvasPosition | null): void {
		pendingAddPosition = position;
		addNodeOpen = true;
	}

	function handleAddNodeChoice(nodeType: string, name: string | null): void {
		onaddnode?.(
			pendingAddPosition ?? canvas?.viewportCenter() ?? { x: 0, y: 0 },
			nodeType,
			name,
		);
		pendingAddPosition = null;
	}

	function openContext(info: CanvasContext): void {
		const kind =
			info.kind === 'node' && isGroupTitleId(info.id ?? '')
				? 'group'
				: info.kind;
		contextMenu = { kind, id: info.id, x: info.x, y: info.y };
	}

	const menuTitle = $derived(
		!contextMenu || contextMenu.kind === 'blank'
			? 'Canvas'
			: contextMenu.kind === 'group'
				? `Group ${foldedView.titles[contextMenu.id ?? '']?.label ?? ''}`
				: contextMenu.kind === 'edge'
					? `Edge ${contextMenu.id ?? ''}`
					: `Node ${contextMenu.id ?? ''}`,
	);

	const paramReason = $derived.by(() => {
		if (!contextMenu || !contextMenu.id) return 'No target node.';
		if (!onjumpparam) return 'No definition panel is available.';
		return null;
	});

	const menuItems = $derived.by((): ContextMenuItem[] => {
		if (!contextMenu) return [];
		const write = editMode;
		if (contextMenu.kind === 'node') {
			return [
				{ id: 'locate', label: 'Focus node' },
				{ id: 'copy-id', label: 'Copy node id' },
				{
					id: 'params',
					label: 'Locate in definition',
					disabled: paramReason !== null,
					...(paramReason ? { reason: paramReason } : {}),
				},
				{
					id: 'delete-node',
					label: 'Delete node',
					danger: true,
					disabled: !write,
					...(!write
						? { reason: 'Read-only canvas. Enter edit mode to delete.' }
						: {}),
				},
			];
		}
		if (contextMenu.kind === 'group') {
			const title = contextMenu.id
				? foldedView.titles[contextMenu.id]
				: undefined;
			const collapsedNow = title ? folded.collapsed.has(title.groupId) : false;
			return [
				{ id: 'focus-group', label: 'Focus whole group' },
				{
					id: 'toggle-group',
					label: collapsedNow ? 'Expand group' : 'Collapse group',
				},
				{
					id: 'delete-group',
					label: 'Delete group',
					danger: true,
					disabled: !write,
					...(!write
						? { reason: 'Read-only canvas. Enter edit mode to delete.' }
						: {}),
				},
			];
		}
		if (contextMenu.kind === 'edge') {
			return [
				{ id: 'focus-source', label: 'Focus source node' },
				{ id: 'focus-target', label: 'Focus target node' },
				{
					id: 'delete-edge',
					label: 'Delete edge',
					danger: true,
					disabled: !write,
					...(!write
						? { reason: 'Read-only canvas. Enter edit mode to delete.' }
						: {}),
				},
			];
		}
		return [
			{ id: 'fit', label: 'Fit view' },
			{ id: 'relayout', label: 'Re-run layout' },
			{
				id: 'edit-toggle',
				label: editMode ? 'Exit edit mode' : 'Enter edit mode',
				disabled: !editable,
				...(!editable ? { reason: 'This graph is not editable.' } : {}),
			},
		];
	});

	function menuAction(action: string): void {
		const menu = contextMenu;
		contextMenu = null;
		if (!menu) return;
		const id = menu.id;
		switch (action) {
			case 'locate':
				if (id) void focus(id);
				break;
			case 'copy-id':
				if (id) void copyText(id, 'Node id');
				break;
			case 'params':
				if (id && !paramReason) onjumpparam?.(id);
				break;
			case 'delete-node':
				if (id) ondeletenodes?.([id]);
				break;
			case 'focus-group':
				if (id) focusGroup(id);
				break;
			case 'toggle-group': {
				const title = id ? foldedView.titles[id] : undefined;
				if (title) toggleGroup(title.groupId);
				break;
			}
			case 'delete-group': {
				const title = id ? foldedView.titles[id] : undefined;
				if (title) ondeletegroups?.([title.groupId]);
				break;
			}
			case 'delete-edge':
				if (id) ondeleteedge?.(id);
				break;
			case 'focus-source':
			case 'focus-target': {
				const edge = foldedView.edges.find((entry) => entry.id === id);
				const endpoint =
					action === 'focus-source' ? edge?.source : edge?.target;
				if (endpoint) void focus(endpoint);
				break;
			}
			case 'fit':
				canvas?.fit();
				break;
			case 'relayout':
				canvas?.relayout();
				break;
			case 'edit-toggle':
				if (editMode) onexitedit?.();
				else onenteredit?.();
				break;
			default:
				break;
		}
	}

	$effect(() => {
		const root = explorerRoot;
		if (!root) return;
		return registerCanvasShortcuts(root, () => ({
			fit: () => canvas?.fit(),
			relayout: () => canvas?.relayout(),
			zoomIn: () => canvas?.zoomIn(),
			zoomOut: () => canvas?.zoomOut(),
			focusSelected: () => {
				const id = selectedId ?? canvas?.selectedNodeIds()[0];
				if (id) canvas?.zoomTo(id);
			},
			selectAll: () => canvas?.selectAll(),
			deleteSelected: () => handleDeleteSelected(),
			undo: () => onundo?.(),
			redo: () => onredo?.(),
			toggleEdit: () => {
				if (!editable) return;
				if (editMode) onexitedit?.();
				else onenteredit?.();
			},
			save: () => onsave?.(),
			canWrite: () => editMode,
			onreadonlywrite: () =>
				toasts.info('Read-only canvas', 'Enter edit mode to change the graph.'),
		}));
	});

	function toggleKind(kind: string): void {
		hiddenKinds = hiddenKinds.includes(kind)
			? hiddenKinds.filter((entry) => entry !== kind)
			: [...hiddenKinds, kind];
	}

	function isolateKind(kind: string): void {
		hiddenKinds = kinds.filter((entry) => entry !== kind);
	}

	const aggregatedCounts = $derived(kindCounts(nodes));

	function toggleOverlay(id: string): void {
		onoverlay?.(activeOverlay === id ? null : id);
	}

	function handleExport(): void {
		const done = canvas?.exportPng() ?? false;
		if (done) {
			toasts.success('Graph exported as PNG');
		} else {
			toasts.error('Graph export failed', 'The renderer is not ready yet.');
		}
	}

	/** Focus a node from outside (version diff rows, validation issues). */
	export async function focus(id: string): Promise<void> {
		if (foldedView.hiddenIds.has(id)) {
			const groupId = filtered.nodes.find((node) => node.id === id)?.groupId;
			if (groupId) {
				collapsedIds = collapsedIds.filter((entry) => entry !== groupId);
				await tick();
			}
		}
		onselect?.(id);
		canvas?.zoomTo(id);
	}

	// Denser execution graphs hide edge labels sooner to stay readable.
	const edgeLabelLimit = $derived(EDGE_LABEL_LIMIT[preset] ?? 60);

	function handleBoxSelect(ids: string[]): void {
		if (ids.length === 1) {
			onselect?.(ids[0]);
			return;
		}
		toasts.info(
			'Box selection',
			`${ids.length} nodes in the box. Click a node to inspect it.`,
		);
	}

	const layoutOptions = [
		{ value: 'layered', label: 'Layered' },
		{ value: 'columns', label: 'Columns' },
		{ value: 'force', label: 'Force' },
		{ value: 'grid', label: 'Grid' },
	];
</script>

<div
	bind:this={explorerRoot}
	class={cn('flex min-h-0 flex-col gap-2', className)}
>
	<div class="flex flex-wrap items-center gap-1.5">
		<IconButton icon="plus" label="Zoom in" onclick={() => canvas?.zoomIn()} />
		<IconButton
			icon="minus"
			label="Zoom out"
			onclick={() => canvas?.zoomOut()}
		/>
		<IconButton
			icon="maximize"
			label="Fit to view (F)"
			onclick={() => canvas?.fit()}
		/>
		<IconButton
			icon="refresh"
			label="Re-run layout (R)"
			onclick={() => canvas?.relayout()}
		/>
		<Select
			bind:value={layout}
			options={layoutOptions}
			size="sm"
			placeholder=""
			class="w-28"
		/>
		<IconButton icon="download" label="Export as PNG" onclick={handleExport} />
		<IconButton
			icon="filter"
			label={showFilters ? 'Hide filters' : 'Show filters'}
			onclick={() => (showFilters = !showFilters)}
		/>
		<Button
			variant={preferences.minimapMode === 'off' ? 'outline' : 'default'}
			size="sm"
			onclick={cycleMinimap}
			title="Cycle minimap auto / on / off"
		>
			Minimap {preferences.minimapMode === 'auto'
				? 'auto'
				: preferences.minimapMode === 'on'
					? 'on'
					: 'off'}
		</Button>
		<Button
			variant="outline"
			size="sm"
			disabled={!selectedId}
			onclick={() => selectedId && canvas?.zoomTo(selectedId)}
		>
			Zoom to selection
		</Button>
		<Button
			variant="outline"
			size="sm"
			disabled={activeIds.size === 0}
			onclick={() => canvas?.fitTo([...activeIds])}
		>
			Fit highlight
		</Button>
		{#if actions}
			<span class="mx-1 h-5 w-px bg-border"></span>
			{@render actions()}
		{/if}
		{#if editable}
			<span class="mx-1 h-5 w-px bg-border"></span>
			{#if !editMode}
				<Button variant="outline" size="sm" onclick={() => onenteredit?.()}>
					Enter edit mode
				</Button>
			{:else}
				<Badge variant="warning">Editing{editDirty ? ' · unsaved' : ''}</Badge>
				<Button
					variant="outline"
					size="sm"
					disabled={!canUndo}
					onclick={() => onundo?.()}
				>
					Undo
				</Button>
				<Button
					variant="outline"
					size="sm"
					disabled={!canRedo}
					onclick={() => onredo?.()}
				>
					Redo
				</Button>
				<Button variant="outline" size="sm" onclick={() => openAddNode(null)}>
					Add node
				</Button>
				<Button variant="outline" size="sm" onclick={handleDeleteSelected}>
					Delete selected
				</Button>
				<Button
					size="sm"
					disabled={!editDirty || editBusy}
					onclick={() => onsave?.()}
				>
					{editBusy ? 'Saving…' : 'Save draft'}
				</Button>
				<Button variant="ghost" size="sm" onclick={() => onvalidate?.()}>
					Validate
				</Button>
				<Button variant="ghost" size="sm" onclick={() => onpromote?.()}>
					Promote
				</Button>
				<Button variant="ghost" size="sm" onclick={() => onexitedit?.()}>
					Exit
				</Button>
			{/if}
		{/if}
		<span class="ml-auto text-micro text-muted-foreground">
			{#if folded.auto.length > 0}
				Folded {folded.auto.length} group(s) ·
			{/if}
			{#if capped.truncated}
				Showing {capped.nodes.length} of {capped.total} nodes · Retention order: failed,
				running, critical path, selection ·
			{/if}
			{filtered.nodes.length} nodes · {filtered.edges.length} edges
		</span>
	</div>

	{#if editMode}
		<p class="text-micro text-muted-foreground">
			Drag nodes to move · drag a hotspot or shift-click another node to connect
			from the selection · double-click empty canvas or use Add node to pick a
			template · click an edge to delete it · Delete selected removes the
			selection. Layout is frozen while editing.
		</p>
	{/if}
	{#if overlays.length > 0}
		<div class="flex flex-wrap items-center gap-1.5">
			<span class="text-micro text-muted-foreground">Highlight:</span>
			{#each overlays as overlay (overlay.id)}
				<Button
					variant={activeOverlay === overlay.id ? 'default' : 'outline'}
					size="sm"
					onclick={() => toggleOverlay(overlay.id)}
				>
					{overlay.label}
					<Badge variant="neutral" class="ml-1">{overlay.ids.length}</Badge>
				</Button>
			{/each}
		</div>
	{/if}
	{#if groups.length > 0}
		<div class="flex flex-wrap items-center gap-1.5">
			<span class="text-micro text-muted-foreground">Groups:</span>
			{#each groups as group (group.id)}
				<Button
					variant={folded.collapsed.has(group.id) ? 'default' : 'outline'}
					size="sm"
					onclick={() => toggleGroup(group.id)}
				>
					{group.label}
					<Badge variant="neutral" class="ml-1"
						>{groupMemberCounts[group.id] ?? 0}</Badge
					>
				</Button>
			{/each}
			<Button variant="ghost" size="sm" onclick={collapseAllGroups}>
				Collapse all
			</Button>
			<Button variant="ghost" size="sm" onclick={expandAllGroups}>
				Expand all
			</Button>
		</div>
	{/if}

	{#if showFilters}
		<Card title="Filters" class="shrink-0">
			<div class="flex flex-wrap items-center gap-2">
				<Input
					bind:value={query}
					placeholder="Search nodes… (Enter to locate)"
					onkeydown={(event) => {
						if (event.key === 'Enter') {
							event.preventDefault();
							stepMatch(event.shiftKey ? -1 : 1);
						}
					}}
					class="h-7 w-44"
				/>
				{#if query.trim()}
					<span class="text-micro text-muted-foreground">{matchLabel}</span>
					<IconButton
						icon="chevron-left"
						label="Previous match"
						disabled={matchIds.length === 0}
						onclick={() => stepMatch(-1)}
					/>
					<IconButton
						icon="chevron-right"
						label="Next match"
						disabled={matchIds.length === 0}
						onclick={() => stepMatch(1)}
					/>
				{/if}
				{#each kinds as kind (kind)}
					<label
						class="flex cursor-pointer items-center gap-1.5 rounded-md border border-border px-2 py-1 text-caption"
					>
						<input
							type="checkbox"
							checked={!hiddenKinds.includes(kind)}
							onchange={() => toggleKind(kind)}
							class="accent-current"
						/>
						<span class="font-mono">{kind}</span>
					</label>
				{/each}
				{#if hiddenKinds.length > 0 || query}
					<Button
						variant="ghost"
						size="sm"
						onclick={() => {
							hiddenKinds = [];
							query = '';
						}}
					>
						Clear
					</Button>
				{/if}
			</div>
		</Card>
	{/if}

	{#if loading}
		<Skeleton class="h-96 w-full rounded-lg" />
	{:else if error}
		<ErrorState
			title="Graph failed to load"
			description={error}
			{onretry}
			class="rounded-lg border border-border bg-card"
		/>
	{:else if nodes.length === 0}
		<EmptyState
			icon="workflow"
			title="No graph data"
			description="This item has no nodes yet."
			class="rounded-lg border border-border bg-card"
		/>
	{:else}
		<div class="grid min-h-0 gap-2 xl:grid-cols-[1fr_16rem]">
			<GraphCanvas
				bind:this={canvas}
				nodes={capped.nodes}
				edges={capped.edges}
				{preset}
				{layout}
				{selectedId}
				highlightIds={[...activeIds]}
				problemIds={issueIds}
				{pulseIds}
				{criticalIds}
				{failedIds}
				{heatTierById}
				{decisionIds}
				tooltipLabels={mergedTooltipLabels}
				{positions}
				{editMode}
				{edgeLabelLimit}
				minimap={preferences.minimapMode}
				{presence}
				{oncursormove}
				collapsedIds={[...folded.collapsed]}
				hiddenIds={[...foldedView.hiddenIds]}
				groupTitles={foldedView.titles}
				onselect={(id) => onselect?.(id)}
				onexpand={handleExpand}
				onboxselect={handleBoxSelect}
				onmovenode={(id, position) => {
					if (isHiddenNode(id)) {
						toasts.info(
							'Hidden group member',
							'Expand the group before moving hidden nodes.',
						);
						return;
					}
					onmovenode?.(id, position);
				}}
				ongroupmove={(moves) => {
					const visible = guardMoves(moves);
					if (visible.length > 0) onmovenodes?.(visible);
				}}
				onbackgrounddoubleclick={(position) => openAddNode(position)}
				ondeleteedge={(id) => ondeleteedge?.(id)}
				onconnect={(source, target) => {
					if (guardConnect(source, target)) onconnect?.(source, target);
				}}
				onconnectreject={(reason) => toasts.info('Cannot connect', reason)}
				oncontext={openContext}
				class="min-h-0"
			/>
			<div class="flex min-h-0 flex-col gap-2">
				{#if capped.truncated || folded.auto.length > 0}
					<Card title="Large graph">
						<p class="text-caption text-muted-foreground">
							Retention order: failed, running, critical path, selection.
							Non-critical groups fold first; leftovers sample across kinds.
							{#if folded.auto.length > 0}
								Folded {folded.auto.length} non-critical group(s) ({folded.auto.join(
									', ',
								)}) to preserve structure.
							{/if}
							{#if capped.truncated}
								Showing {capped.nodes.length} of {capped.total} nodes, sampled across
								kinds ({aggregatedCounts
									.map((entry) => `${entry.kind} ${entry.count}`)
									.join(' · ')}). Fold to one kind or use filters; double-click
								a node to expand its neighborhood.
							{/if}
						</p>
						<div class="mt-2 flex flex-wrap gap-1.5">
							{#each aggregatedCounts.slice(0, 4) as entry (entry.kind)}
								<Button
									variant="outline"
									size="sm"
									onclick={() => isolateKind(entry.kind)}
								>
									Fold to {entry.kind}
								</Button>
							{/each}
							{#if hiddenKinds.length > 0}
								<Button
									variant="ghost"
									size="sm"
									onclick={() => {
										hiddenKinds = [];
									}}
								>
									Show all kinds
								</Button>
							{/if}
						</div>
					</Card>
				{/if}
				{#if selected}
					<Card title={selected.label}>
						{#snippet actions()}
							<IconButton
								icon="x"
								label="Clear selection"
								onclick={() => onselect?.(null)}
							/>
						{/snippet}
						{#if isGroupTitleId(selected.id)}
							{@const title = foldedView.titles[selected.id]}
							<p class="text-caption text-muted-foreground">
								Group · {title?.label ?? selected.id} · {title?.memberIds
									.length ?? 0} member(s) ·
								{selectedNeighbors.predecessors} in · {selectedNeighbors.successors}
								out
							</p>
							{#if title}
								<div class="mt-2 flex flex-wrap gap-1.5">
									<Button
										variant="outline"
										size="sm"
										onclick={() => toggleGroup(title.groupId)}
									>
										{folded.collapsed.has(title.groupId)
											? 'Expand group'
											: 'Collapse group'}
									</Button>
									<Button
										variant="outline"
										size="sm"
										onclick={() => focusGroup(selected.id)}
									>
										Focus whole group
									</Button>
								</div>
								<p
									class="mt-1 text-micro text-muted-foreground"
									title="Groups have no definition entry."
								>
									Locate in definition is unavailable for groups.
								</p>
							{/if}
						{:else}
							<dl class="space-y-1 text-caption">
								<div class="flex justify-between gap-2">
									<dt class="text-muted-foreground">Kind</dt>
									<dd class="font-mono">{selected.kind}</dd>
								</div>
								{#if selected.status}
									<div class="flex justify-between gap-2">
										<dt class="text-muted-foreground">Status</dt>
										<dd>
											<StatusBadge
												status={selected.status}
												size="sm"
												dot={false}
											/>
										</dd>
									</div>
								{/if}
								{#if selected.iteration !== undefined}
									<div class="flex justify-between gap-2">
										<dt class="text-muted-foreground">Iteration</dt>
										<dd class="font-mono">{selected.iteration}</dd>
									</div>
								{/if}
								{#if heatLabels[selected.id]}
									<div class="flex justify-between gap-2">
										<dt class="text-muted-foreground">Duration</dt>
										<dd class="font-mono tabular-nums">
											{heatLabels[selected.id]}
										</dd>
									</div>
								{/if}
								{#if decisionLabels[selected.id]}
									<div class="flex justify-between gap-2">
										<dt class="text-muted-foreground">Branches</dt>
										<dd class="text-right font-mono">
											{decisionLabels[selected.id]}
										</dd>
									</div>
								{/if}
								<div class="flex justify-between gap-2">
									<dt class="text-muted-foreground">Links</dt>
									<dd class="font-mono">
										{selectedNeighbors.predecessors} in · {selectedNeighbors.successors}
										out
									</dd>
								</div>
							</dl>
							{#if inspector}
								<div class="mt-2 border-t border-border pt-2">
									{@render inspector()}
								</div>
							{/if}
							{#if onexpand}
								<div class="mt-2">
									<Button
										variant="outline"
										size="sm"
										onclick={() => selected && onexpand?.(selected.id)}
									>
										<Icon name="git-commit" size={13} />
										{expandLabel}
									</Button>
								</div>
							{/if}
							<div class="mt-2 flex flex-wrap gap-1.5">
								<Button
									variant="outline"
									size="sm"
									onclick={() =>
										selected && void copyText(selected.id, 'Node id')}
								>
									Copy node id
								</Button>
								{#if onjumpparam}
									<Button
										variant="outline"
										size="sm"
										onclick={() => selected && onjumpparam?.(selected.id)}
									>
										Locate in definition
									</Button>
								{:else}
									<Button
										variant="outline"
										size="sm"
										disabled
										title="No definition panel is available."
									>
										Locate in definition
									</Button>
								{/if}
							</div>
							{#if !onjumpparam}
								<p class="mt-1 text-micro text-muted-foreground">
									No definition panel is available.
								</p>
							{/if}
						{/if}
					</Card>
				{/if}
				<Card title="Legend">
					<ul class="space-y-1">
						{#each legendFor(preset) as entry (entry.label)}
							<li
								class="flex items-center gap-2 text-caption text-muted-foreground"
							>
								<span
									class="inline-block h-2.5 w-2.5"
									style:background={entry.shape.startsWith('line')
										? 'transparent'
										: entry.color}
									style:border-radius={entry.shape === 'ellipse'
										? '9999px'
										: entry.shape === 'diamond'
											? '2px'
											: '4px'}
									style:transform={entry.shape === 'diamond'
										? 'rotate(45deg)'
										: 'none'}
									style:border={entry.shape.startsWith('line')
										? `2px ${entry.shape === 'line-dashed' ? 'dashed' : 'solid'} ${entry.color}`
										: 'none'}
								></span>
								{entry.label}
							</li>
						{/each}
					</ul>
				</Card>
			</div>
		</div>
	{/if}
	{#if contextMenu}
		{#key `${contextMenu.kind}:${contextMenu.id ?? ''}:${contextMenu.x}:${contextMenu.y}`}
			<ContextMenu
				x={contextMenu.x}
				y={contextMenu.y}
				title={menuTitle}
				items={menuItems}
				onaction={menuAction}
				onclose={() => (contextMenu = null)}
			/>
		{/key}
	{/if}
	<AddNodeDrawer
		bind:open={addNodeOpen}
		position={pendingAddPosition}
		onselect={handleAddNodeChoice}
		onclose={() => (pendingAddPosition = null)}
	/>
</div>
