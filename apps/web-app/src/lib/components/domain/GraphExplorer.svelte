<script lang="ts">
	import { tick } from 'svelte';
	import type { Snippet } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import EmptyState from '#lib/ui/components/EmptyState.svelte';
	import ErrorState from '#lib/ui/components/ErrorState.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import Select from '#lib/ui/components/Select.svelte';
	import Skeleton from '#lib/ui/components/Skeleton.svelte';
	import StatusBadge from '#lib/ui/components/StatusBadge.svelte';
	import GraphCanvas, {
		type CanvasContext,
		type CanvasMove,
		type CanvasPosition,
		type PresenceCursor,
	} from '#lib/components/domain/GraphCanvas.svelte';
	import AddNodeDrawer from '#lib/components/domain/AddNodeDrawer.svelte';
	import ContextMenu, {
		type ContextMenuItem,
	} from '#lib/ui/components/ContextMenu.svelte';
	import {
		deriveGroups,
		foldForCap,
		groupIdFromTitle,
		isGroupTitleId,
	} from '#lib/graph/group-view';
	import {
		columnPositions,
		compactPositions,
		layeredPositions,
		rowPositions,
		sortIdsByCanvasPosition,
	} from '#lib/graph/layout';
	import {
		registerCanvasShortcuts,
		shortcutTitle,
	} from '#lib/graph/canvas-shortcuts';
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
		type GraphOverlay,
		type GraphPreset,
	} from '#lib/graph/display-model';
	import { toasts } from '#lib/stores/toast.svelte';
	import { GraphFilterState } from '#lib/graph/use-graph-filter.svelte';
	import { preferences } from '#lib/stores/preferences.svelte';
	import {
		getWorkflowLayout,
		resetWorkflowLayout,
		saveWorkflowLayout,
	} from '#lib/services/layout';
	import { cn } from '#lib/ui/cn';
	import { t } from '#lib/i18n';

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
		/** Workflow id backing the graph; enables layout load/save/reset. */
		layoutId?: string;
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
		/** Toggle the host's recent-runs panel, if it renders one. */
		ontogglelogs?: () => void;
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
		layoutId = undefined,
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
		ontogglelogs,
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
	const filter = new GraphFilterState();
	let collapsedIds = $state<string[]>([]);
	let matchIndex = $state(0);

	// Persisted layout (positions saved on the backend). Loaded once per
	// workflow id; view-mode drags update it and debounce a save.
	let savedPositions = $state<Record<string, CanvasPosition>>({});
	let layoutLoadedId: string | null = null;
	let layoutSaveTimer: ReturnType<typeof setTimeout> | null = null;

	$effect(() => {
		const id = layoutId;
		if (!id || layoutLoadedId === id) return;
		layoutLoadedId = id;
		void getWorkflowLayout(id)
			.then((saved) => {
				if (!saved) return;
				const positions: Record<string, CanvasPosition> = {};
				for (const [nodeId, point] of Object.entries(saved.positions ?? {})) {
					positions[nodeId] = { x: point.x, y: point.y };
				}
				savedPositions = positions;
				const kind = saved.settings?.kind;
				if (
					kind === 'layered' ||
					kind === 'columns' ||
					kind === 'rows' ||
					kind === 'compact'
				) {
					layout = kind;
				}
			})
			.catch(() => {
				// Layout persistence is optional; the preset layout still renders.
			});
	});

	function handleViewMove(id: string, position: CanvasPosition): void {
		if (!layoutId || editMode) return;
		savedPositions = { ...savedPositions, [id]: position };
		if (layoutSaveTimer) clearTimeout(layoutSaveTimer);
		layoutSaveTimer = setTimeout(() => {
			layoutSaveTimer = null;
			void saveWorkflowLayout(layoutId, {
				positions: savedPositions,
				settings: { kind: layout },
			}).catch(() => {
				toasts.error(
					t('domain.graph.layoutSaveFailed'),
					t('domain.graph.layoutSaveFailedDetail'),
				);
			});
		}, 800);
	}

	async function handleResetLayout(): Promise<void> {
		if (!layoutId) return;
		if (layoutSaveTimer) {
			clearTimeout(layoutSaveTimer);
			layoutSaveTimer = null;
		}
		savedPositions = {};
		canvas?.relayout();
		try {
			await resetWorkflowLayout(layoutId);
		} catch {
			toasts.error(
				t('domain.graph.layoutResetFailed'),
				t('domain.graph.layoutResetFailedDetail'),
			);
		}
	}

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

	const filtered = $derived(filter.filter(nodes, edges));

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
			toasts.success(t('domain.graph.copied', { name: label }));
		} catch {
			toasts.error(
				t('domain.graph.copyFailed'),
				t('domain.graph.copyFailedDetail'),
			);
		}
	}

	function isHiddenNode(id: string): boolean {
		return foldedView.hiddenIds.has(id);
	}

	function guardConnect(source: string, target: string): boolean {
		if (isHiddenNode(source) || isHiddenNode(target)) {
			toasts.info(
				t('domain.graph.hiddenMember'),
				t('domain.graph.hiddenMemberConnect'),
			);
			return false;
		}
		if (isGroupTitleId(source) || isGroupTitleId(target)) {
			toasts.info(
				t('domain.graph.groupTitleCannotConnect'),
				t('domain.graph.groupTitleConnectDetail'),
			);
			return false;
		}
		return true;
	}

	function guardMoves(moves: CanvasMove[]): CanvasMove[] {
		const visible = moves.filter((move) => !isHiddenNode(move.id));
		if (visible.length !== moves.length) {
			toasts.info(
				t('domain.graph.hiddenMember'),
				t('domain.graph.hiddenMemberMove'),
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
				? columnPositions(capped.nodes, capped.edges)
				: layout === 'rows'
					? rowPositions(capped.nodes, capped.edges)
					: layout === 'compact'
						? compactPositions(capped.nodes, capped.edges)
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
			? t('domain.graph.noMatches')
			: t('domain.graph.matchOf', {
					index: Math.min(matchIndex + 1, matchIds.length),
					total: matchIds.length,
				}),
	);

	$effect(() => {
		void filter.query;
		void filter.hiddenKinds;
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
	let addNodeAnchor = $state<{ x: number; y: number } | null>(null);

	function openAddNode(
		position: CanvasPosition | null,
		client?: { x: number; y: number } | null,
	): void {
		pendingAddPosition = position;
		addNodeAnchor = client ?? null;
		addNodeOpen = true;
	}

	function handleAddNodeChoice(nodeType: string, name: string | null): void {
		onaddnode?.(
			pendingAddPosition ?? canvas?.viewportCenter() ?? { x: 0, y: 0 },
			nodeType,
			name,
		);
		pendingAddPosition = null;
		addNodeAnchor = null;
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
			? t('domain.graph.menuCanvas')
			: contextMenu.kind === 'group'
				? t('domain.graph.menuGroup', {
						label: foldedView.titles[contextMenu.id ?? '']?.label ?? '',
					})
				: contextMenu.kind === 'edge'
					? t('domain.graph.menuEdge', { id: contextMenu.id ?? '' })
					: t('domain.graph.menuNode', { id: contextMenu.id ?? '' }),
	);

	const paramReason = $derived.by(() => {
		if (!contextMenu || !contextMenu.id) return t('domain.graph.noTargetNode');
		if (!onjumpparam) return t('domain.graph.noDefinitionPanel');
		return null;
	});

	const readOnlyReason = $derived(t('domain.graph.readOnlyDelete'));

	const menuItems = $derived.by((): ContextMenuItem[] => {
		if (!contextMenu) return [];
		const write = editMode;
		if (contextMenu.kind === 'node') {
			return [
				{ id: 'locate', label: t('domain.graph.focusNode') },
				{ id: 'copy-id', label: t('domain.graph.copyNodeId') },
				{
					id: 'params',
					label: t('domain.graph.locateInDefinition'),
					disabled: paramReason !== null,
					...(paramReason ? { reason: paramReason } : {}),
				},
				{
					id: 'delete-node',
					label: t('domain.graph.deleteNode'),
					danger: true,
					disabled: !write,
					...(!write ? { reason: readOnlyReason } : {}),
				},
			];
		}
		if (contextMenu.kind === 'group') {
			const title = contextMenu.id
				? foldedView.titles[contextMenu.id]
				: undefined;
			const collapsedNow = title ? folded.collapsed.has(title.groupId) : false;
			return [
				{ id: 'focus-group', label: t('domain.graph.focusWholeGroup') },
				{
					id: 'toggle-group',
					label: collapsedNow
						? t('domain.graph.expandGroup')
						: t('domain.graph.collapseGroup'),
				},
				{
					id: 'delete-group',
					label: t('domain.graph.deleteGroup'),
					danger: true,
					disabled: !write,
					...(!write ? { reason: readOnlyReason } : {}),
				},
			];
		}
		if (contextMenu.kind === 'edge') {
			return [
				{ id: 'focus-source', label: t('domain.graph.focusSource') },
				{ id: 'focus-target', label: t('domain.graph.focusTarget') },
				{
					id: 'delete-edge',
					label: t('domain.graph.deleteEdge'),
					danger: true,
					disabled: !write,
					...(!write ? { reason: readOnlyReason } : {}),
				},
			];
		}
		return [
			{ id: 'fit', label: t('domain.graph.fitView') },
			{ id: 'relayout', label: t('domain.graph.rerunLayout') },
			{
				id: 'add-node',
				label: t('domain.graph.addNodeHere'),
				disabled: !write,
				...(!write ? { reason: readOnlyReason } : {}),
			},
			{
				id: 'edit-toggle',
				label: editMode
					? t('domain.graph.exitEditMode')
					: t('domain.graph.enterEditMode'),
				disabled: !editable,
				...(!editable ? { reason: t('domain.graph.notEditable') } : {}),
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
				if (id) void copyText(id, t('domain.graph.nodeId'));
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
			case 'add-node':
				openAddNode(null, { x: menu.x, y: menu.y });
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
			nextNode: () => stepSelection(1),
			prevNode: () => stepSelection(-1),
			toggleLogs: () => ontogglelogs?.(),
			toggleFilters: () => {
				showFilters = !showFilters;
			},
			canWrite: () => editMode,
			onreadonlywrite: () =>
				toasts.info(
					t('domain.graph.readOnlyCanvas'),
					t('domain.graph.enterEditToChange'),
				),
		}));
	});

	function toggleKind(kind: string): void {
		filter.toggleKind(kind);
	}

	function isolateKind(kind: string): void {
		filter.hiddenKinds = kinds.filter((entry) => entry !== kind);
	}

	/** Move the selection across the filtered node order. */
	function stepSelection(delta: number): void {
		const ids = filtered.nodes.map((node) => node.id);
		if (ids.length === 0) return;
		const current = selectedId ?? canvas?.selectedNodeIds()[0] ?? null;
		const index = current ? ids.indexOf(current) : -1;
		const next =
			ids[(index + delta + ids.length) % ids.length] ?? ids[0] ?? null;
		if (next) onselect?.(next);
	}

	const aggregatedCounts = $derived(kindCounts(nodes));

	function toggleOverlay(id: string): void {
		onoverlay?.(activeOverlay === id ? null : id);
	}

	function handleExport(): void {
		const done = canvas?.exportPng() ?? false;
		if (done) {
			toasts.success(t('domain.graph.exported'));
		} else {
			toasts.error(
				t('domain.graph.exportFailed'),
				t('domain.graph.exportFailedDetail'),
			);
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
			t('domain.graph.boxSelection'),
			t('domain.graph.boxSelectionDetail', { count: ids.length }),
		);
	}

	const layoutOptions = $derived([
		{ value: 'layered', label: t('domain.graph.layoutLayered') },
		{ value: 'columns', label: t('domain.graph.layoutColumns') },
		{ value: 'rows', label: t('domain.graph.layoutRows') },
		{ value: 'compact', label: t('domain.graph.layoutCompact') },
	]);
</script>

<div
	bind:this={explorerRoot}
	class={cn('flex min-h-0 flex-col gap-2', className)}
>
	<div class="flex flex-wrap items-center gap-1.5">
		<IconButton
			icon="plus"
			label={shortcutTitle('canvas.zoomIn', t('domain.graph.zoomIn'))}
			onclick={() => canvas?.zoomIn()}
		/>
		<IconButton
			icon="minus"
			label={shortcutTitle('canvas.zoomOut', t('domain.graph.zoomOut'))}
			onclick={() => canvas?.zoomOut()}
		/>
		<IconButton
			icon="maximize"
			label={shortcutTitle('canvas.fit', t('domain.graph.fitView'))}
			onclick={() => canvas?.fit()}
		/>
		<IconButton
			icon="refresh"
			label={shortcutTitle('canvas.relayout', t('domain.graph.rerunLayout'))}
			onclick={() => canvas?.relayout()}
		/>
		{#if layoutId}
			<IconButton
				icon="undo"
				label={t('domain.graph.resetLayout')}
				onclick={() => void handleResetLayout()}
			/>
		{/if}
		<Select
			bind:value={layout}
			options={layoutOptions}
			size="sm"
			placeholder=""
			class="w-28"
		/>
		<IconButton
			icon="download"
			label={t('domain.graph.exportPng')}
			onclick={handleExport}
		/>
		<IconButton
			icon="filter"
			label={showFilters
				? t('domain.graph.hideFilters')
				: t('domain.graph.showFilters')}
			onclick={() => (showFilters = !showFilters)}
		/>
		<Button
			variant={preferences.minimapMode === 'off' ? 'outline' : 'default'}
			size="sm"
			onclick={cycleMinimap}
			title={t('domain.graph.cycleMinimap')}
		>
			{t('domain.graph.minimap')}
			{preferences.minimapMode}
		</Button>
		<Button
			variant="outline"
			size="sm"
			disabled={!selectedId}
			onclick={() => selectedId && canvas?.zoomTo(selectedId)}
		>
			{t('domain.graph.zoomToSelection')}
		</Button>
		<Button
			variant="outline"
			size="sm"
			disabled={activeIds.size === 0}
			onclick={() => canvas?.fitTo([...activeIds])}
		>
			{t('domain.graph.fitHighlight')}
		</Button>
		{#if actions}
			<span class="mx-1 h-5 w-px bg-border"></span>
			{@render actions()}
		{/if}
		{#if editable}
			<span class="mx-1 h-5 w-px bg-border"></span>
			{#if !editMode}
				<Button variant="outline" size="sm" onclick={() => onenteredit?.()}>
					{t('domain.graph.enterEditMode')}
				</Button>
			{:else}
				<Badge variant="warning"
					>{editDirty
						? t('domain.graph.editingUnsaved')
						: t('domain.graph.editing')}</Badge
				>
				<Button
					variant="outline"
					size="sm"
					disabled={!canUndo}
					onclick={() => onundo?.()}
				>
					{t('domain.graph.undo')}
				</Button>
				<Button
					variant="outline"
					size="sm"
					disabled={!canRedo}
					onclick={() => onredo?.()}
				>
					{t('domain.graph.redo')}
				</Button>
				<Button variant="outline" size="sm" onclick={() => openAddNode(null)}>
					{t('domain.graph.addNode')}
				</Button>
				<Button variant="outline" size="sm" onclick={handleDeleteSelected}>
					{t('domain.graph.deleteSelected')}
				</Button>
				<Button
					size="sm"
					disabled={!editDirty || editBusy}
					onclick={() => onsave?.()}
				>
					{editBusy ? t('domain.graph.saving') : t('domain.graph.saveDraft')}
				</Button>
				<Button variant="ghost" size="sm" onclick={() => onvalidate?.()}>
					{t('domain.graph.validate')}
				</Button>
				<Button variant="ghost" size="sm" onclick={() => onpromote?.()}>
					{t('domain.graph.promote')}
				</Button>
				<Button variant="ghost" size="sm" onclick={() => onexitedit?.()}>
					{t('domain.graph.exit')}
				</Button>
			{/if}
		{/if}
		<span class="ml-auto text-micro text-muted-foreground">
			{#if folded.auto.length > 0}
				{t('domain.graph.foldedGroups', { count: folded.auto.length })} ·
			{/if}
			{#if capped.truncated}
				{t('domain.graph.showingOf', {
					shown: capped.nodes.length,
					total: capped.total,
				})}
				· {t('domain.graph.retentionOrder')} ·
			{/if}
			{t('domain.canvas.summary', {
				nodes: filtered.nodes.length,
				edges: filtered.edges.length,
			})}
		</span>
	</div>

	{#if editMode}
		<p class="text-micro text-muted-foreground">
			{t('domain.graph.editHint')}
		</p>
	{/if}
	{#if overlays.length > 0}
		<div class="flex flex-wrap items-center gap-1.5">
			<span class="text-micro text-muted-foreground"
				>{t('domain.graph.highlight')}</span
			>
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
			<span class="text-micro text-muted-foreground"
				>{t('domain.graph.groups')}</span
			>
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
				{t('domain.graph.collapseAll')}
			</Button>
			<Button variant="ghost" size="sm" onclick={expandAllGroups}>
				{t('domain.graph.expandAll')}
			</Button>
		</div>
	{/if}

	{#if showFilters}
		<Card title={t('domain.graph.filters')} class="shrink-0">
			<div class="flex flex-wrap items-center gap-2">
				<Input
					bind:value={filter.query}
					placeholder={t('domain.graph.searchPlaceholder')}
					onkeydown={(event) => {
						if (event.key === 'Enter') {
							event.preventDefault();
							stepMatch(event.shiftKey ? -1 : 1);
						}
					}}
					class="h-7 w-44"
				/>
				{#if filter.query.trim()}
					<span class="text-micro text-muted-foreground">{matchLabel}</span>
					<IconButton
						icon="chevron-left"
						label={t('domain.graph.previousMatch')}
						disabled={matchIds.length === 0}
						onclick={() => stepMatch(-1)}
					/>
					<IconButton
						icon="chevron-right"
						label={t('domain.graph.nextMatch')}
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
							checked={!filter.hiddenKinds.includes(kind)}
							onchange={() => toggleKind(kind)}
							class="accent-current"
						/>
						<span class="font-mono">{kind}</span>
					</label>
				{/each}
				{#if filter.hiddenKinds.length > 0 || filter.query}
					<Button
						variant="ghost"
						size="sm"
						onclick={() => {
							filter.clear();
						}}
					>
						{t('domain.inspector.clearFilter')}
					</Button>
				{/if}
			</div>
		</Card>
	{/if}

	{#if loading}
		<Skeleton class="h-96 w-full rounded-lg" />
	{:else if error}
		<ErrorState
			title={t('domain.graph.loadFailedTitle')}
			description={error}
			{onretry}
			class="rounded-lg border border-border bg-card"
		/>
	{:else if nodes.length === 0}
		<EmptyState
			icon="workflow"
			title={t('domain.graph.noDataTitle')}
			description={t('domain.graph.noDataDescription')}
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
				positions={{ ...savedPositions, ...(positions ?? {}) }}
				onviewmove={handleViewMove}
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
							t('domain.graph.hiddenMember'),
							t('domain.graph.hiddenMemberMove'),
						);
						return;
					}
					onmovenode?.(id, position);
				}}
				ongroupmove={(moves) => {
					const visible = guardMoves(moves);
					if (visible.length > 0) onmovenodes?.(visible);
				}}
				onbackgrounddoubleclick={(position, client) =>
					openAddNode(position, client)}
				ondeleteedge={(id) => ondeleteedge?.(id)}
				onconnect={(source, target) => {
					if (guardConnect(source, target)) onconnect?.(source, target);
				}}
				onconnectreject={(reason) =>
					toasts.info(t('domain.graph.cannotConnect'), reason)}
				oncontext={openContext}
				class="min-h-0"
			/>
			<div class="flex min-h-0 flex-col gap-2">
				{#if capped.truncated || folded.auto.length > 0}
					<Card title={t('domain.graph.largeGraph')}>
						<p class="text-caption text-muted-foreground">
							{t('domain.graph.retentionOrderDetail')}
							{#if folded.auto.length > 0}
								{t('domain.graph.foldedNonCritical', {
									count: folded.auto.length,
									kinds: folded.auto.join(', '),
								})}
							{/if}
							{#if capped.truncated}
								{t('domain.graph.sampledAcrossKinds', {
									shown: capped.nodes.length,
									total: capped.total,
									counts: aggregatedCounts
										.map((entry) => `${entry.kind} ${entry.count}`)
										.join(' · '),
								})}
							{/if}
						</p>
						<div class="mt-2 flex flex-wrap gap-1.5">
							{#each aggregatedCounts.slice(0, 4) as entry (entry.kind)}
								<Button
									variant="outline"
									size="sm"
									onclick={() => isolateKind(entry.kind)}
								>
									{t('domain.graph.foldTo', { kind: entry.kind })}
								</Button>
							{/each}
							{#if filter.hiddenKinds.length > 0}
								<Button
									variant="ghost"
									size="sm"
									onclick={() => {
										filter.hiddenKinds = [];
									}}
								>
									{t('domain.graph.showAllKinds')}
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
								label={t('domain.graph.clearSelection')}
								onclick={() => onselect?.(null)}
							/>
						{/snippet}
						{#if isGroupTitleId(selected.id)}
							{@const title = foldedView.titles[selected.id]}
							<p class="text-caption text-muted-foreground">
								{t('domain.graph.groupSummary', {
									label: title?.label ?? selected.id,
									members: title?.memberIds.length ?? 0,
									in: selectedNeighbors.predecessors,
									out: selectedNeighbors.successors,
								})}
							</p>
							{#if title}
								<div class="mt-2 flex flex-wrap gap-1.5">
									<Button
										variant="outline"
										size="sm"
										onclick={() => toggleGroup(title.groupId)}
									>
										{folded.collapsed.has(title.groupId)
											? t('domain.graph.expandGroup')
											: t('domain.graph.collapseGroup')}
									</Button>
									<Button
										variant="outline"
										size="sm"
										onclick={() => focusGroup(selected.id)}
									>
										{t('domain.graph.focusWholeGroup')}
									</Button>
								</div>
								<p
									class="mt-1 text-micro text-muted-foreground"
									title={t('domain.graph.groupsNoDefinition')}
								>
									{t('domain.graph.locateUnavailableForGroups')}
								</p>
							{/if}
						{:else}
							<dl class="space-y-1 text-caption">
								<div class="flex justify-between gap-2">
									<dt class="text-muted-foreground">
										{t('domain.graph.kind')}
									</dt>
									<dd class="font-mono">{selected.kind}</dd>
								</div>
								{#if selected.status}
									<div class="flex justify-between gap-2">
										<dt class="text-muted-foreground">
											{t('domain.graph.status')}
										</dt>
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
										<dt class="text-muted-foreground">
											{t('domain.history.iterations')}
										</dt>
										<dd class="font-mono">{selected.iteration}</dd>
									</div>
								{/if}
								{#if heatLabels[selected.id]}
									<div class="flex justify-between gap-2">
										<dt class="text-muted-foreground">
											{t('domain.inspector.duration')}
										</dt>
										<dd class="font-mono tabular-nums">
											{heatLabels[selected.id]}
										</dd>
									</div>
								{/if}
								{#if decisionLabels[selected.id]}
									<div class="flex justify-between gap-2">
										<dt class="text-muted-foreground">
											{t('domain.graph.branches')}
										</dt>
										<dd class="text-right font-mono">
											{decisionLabels[selected.id]}
										</dd>
									</div>
								{/if}
								<div class="flex justify-between gap-2">
									<dt class="text-muted-foreground">
										{t('domain.graph.links')}
									</dt>
									<dd class="font-mono">
										{t('domain.graph.inOut', {
											in: selectedNeighbors.predecessors,
											out: selectedNeighbors.successors,
										})}
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
										selected &&
										void copyText(selected.id, t('domain.graph.nodeId'))}
								>
									{t('domain.graph.copyNodeId')}
								</Button>
								{#if onjumpparam}
									<Button
										variant="outline"
										size="sm"
										onclick={() => selected && onjumpparam?.(selected.id)}
									>
										{t('domain.graph.locateInDefinition')}
									</Button>
								{:else}
									<Button
										variant="outline"
										size="sm"
										disabled
										title={t('domain.graph.noDefinitionPanel')}
									>
										{t('domain.graph.locateInDefinition')}
									</Button>
								{/if}
							</div>
							{#if !onjumpparam}
								<p class="mt-1 text-micro text-muted-foreground">
									{t('domain.graph.noDefinitionPanel')}
								</p>
							{/if}
						{/if}
					</Card>
				{/if}
				<Card title={t('domain.graph.legend')}>
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
		anchor={addNodeAnchor}
		onselect={handleAddNodeChoice}
		onclose={() => {
			pendingAddPosition = null;
			addNodeAnchor = null;
		}}
	/>
</div>
