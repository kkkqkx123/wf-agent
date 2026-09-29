<script module lang="ts">
	export type TemplateEditTab = 'json' | 'form' | 'graph';
</script>

<script lang="ts">
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import Textarea from '$lib/components/ui/Textarea.svelte';
	import GraphExplorer from '$lib/components/domain/GraphExplorer.svelte';
	import IssueList from '$lib/components/domain/IssueList.svelte';
	import JsonEditor from '$lib/components/ui/JsonEditor.svelte';
	import type { CanvasPosition } from '$lib/components/domain/GraphCanvas.svelte';
	import { GraphEditStore } from '$lib/graph/edit-store.svelte';
	import type { DisplayNode } from '$lib/graph/display-model';
	import {
		jsonErrorLine,
		jsonFieldLine,
		localTemplateIssues,
		parseTemplateTopology,
		skeletonNodeHints,
		templateEditTarget,
		TemplateEditSession,
		type TemplateFormField,
		type TemplateIssue,
	} from '$lib/services/templates';
	import type { TemplateKind } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';

	interface Props {
		kind: TemplateKind;
		tab: TemplateEditTab;
		isNew: boolean;
		validationState: 'idle' | 'valid' | 'invalid';
		fields: TemplateFormField[];
		formState: Record<string, string>;
		editText: string;
		syntaxError: string | null;
		serverIssues: TemplateIssue[];
		session: TemplateEditSession;
		store: GraphEditStore;
		draftBusy: boolean;
		saveBusy: boolean;
		openWorkflowBusy: boolean;
		graphRequest: number;
		ontabchange: (tab: TemplateEditTab) => void;
		onvalidate: () => void;
		onsave: () => void;
		oncancel: () => void;
		onservercheck: () => void;
		onopenworkflow: () => void;
		onmovenode: (id: string, position: CanvasPosition) => void;
		onmovenodes: (
			moves: Array<{ id: string; position: CanvasPosition }>,
		) => void;
		onaddnode: (position: CanvasPosition) => void;
		onconnect: (source: string, target: string) => void;
		ondeletenodes: (ids: string[]) => void;
		ondeletegroups: (ids: string[]) => void;
	}

	let {
		kind,
		tab,
		isNew,
		validationState,
		fields,
		formState = $bindable({}),
		editText = $bindable(''),
		syntaxError,
		serverIssues,
		session,
		store,
		draftBusy,
		saveBusy,
		openWorkflowBusy,
		graphRequest,
		ontabchange,
		onvalidate,
		onsave,
		oncancel,
		onservercheck,
		onopenworkflow,
		onmovenode,
		onmovenodes,
		onaddnode,
		onconnect,
		ondeletenodes,
		ondeletegroups,
	}: Props = $props();

	let editor = $state<{ scrollToLine: (line: number) => void } | null>(null);
	let explorer = $state<{ focus: (id: string) => void } | null>(null);
	let conflictOpen = $state(false);
	let highlightField = $state<string | null>(null);
	let lastGraphRequest = 0;

	// Programmatic graph entry (for example the preview "Edit graph"
	// button) loads the canvas the same way a tab switch does.
	$effect(() => {
		if (graphRequest === lastGraphRequest) return;
		lastGraphRequest = graphRequest;
		if (tab !== 'graph' || kind !== 'workflow') return;
		if (session.hasGraphConflict(store.dirty)) {
			conflictOpen = true;
			return;
		}
		loadStore();
	});

	const errorLine = $derived(
		syntaxError ? jsonErrorLine(editText, syntaxError) : null,
	);

	/** Single safe parse of the JSON projection shared by hints, topology and snippet. */
	const parsedProjection = $derived.by<{
		value: unknown;
		error: string | null;
	}>(() => {
		try {
			return { value: JSON.parse(editText) as unknown, error: null };
		} catch (e) {
			return {
				value: undefined,
				error: e instanceof Error ? e.message : 'Invalid JSON',
			};
		}
	});

	/** Live local hints from the JSON projection; server issues arrive via gate. */
	const localIssues = $derived.by<TemplateIssue[]>(() => {
		if (syntaxError || parsedProjection.value === undefined) return [];
		return localTemplateIssues(kind, parsedProjection.value);
	});

	const combinedIssues = $derived([...localIssues, ...serverIssues]);

	/** Parsed workflow topology from the JSON projection; syntax errors stay on JSON. */
	const topology = $derived.by(() => {
		if (kind !== 'workflow') {
			return { nodes: [], edges: [], error: null as string | null };
		}
		if (parsedProjection.value !== undefined) {
			const parsedTopology = parseTemplateTopology(parsedProjection.value);
			return { ...parsedTopology, error: null as string | null };
		}
		const fallback = session.topology();
		return {
			...fallback,
			error: session.syntaxError ?? parsedProjection.error ?? 'Invalid JSON',
		};
	});

	const graphNodes = $derived<DisplayNode[]>(topology.nodes);

	const serverIssueIds = $derived(
		serverIssues.map((issue) => issue.nodeId).filter((id) => id !== null),
	);

	const skeletonHints = $derived(
		tab === 'graph' && kind === 'workflow'
			? skeletonNodeHints(store.nodes)
			: [],
	);

	const selectedNode = $derived(
		store.selectedId
			? (store.nodes.find((node) => node.id === store.selectedId) ?? null)
			: null,
	);

	const nodeSnippet = $derived.by(() => {
		if (!store.selectedId) return null;
		const parsed: unknown =
			parsedProjection.value === undefined
				? session.fullValue()
				: parsedProjection.value;
		const target = templateEditTarget(kind, parsed);
		const parsedTopology = parseTemplateTopology(
			kind === 'node' || kind === 'trigger'
				? { definition: { nodes: [target], edges: [] } }
				: parsed,
		);
		const match = [...parsedTopology.nodes, ...parsedTopology.edges].find(
			(entry) => entry.id === store.selectedId,
		);
		if (!match) return null;
		const stored = store.nodes.find((node) => node.id === store.selectedId);
		return JSON.stringify(stored ?? match, null, 2);
	});

	function enterForm(): void {
		formState = session.formSnapshot();
	}

	function syncFormToText(): void {
		session.applyForm(formState);
		editText = session.text;
	}

	function switchTab(next: TemplateEditTab): boolean {
		if (next === 'graph' && kind !== 'workflow') return false;
		highlightField = null;
		if (tab === 'graph' && next !== 'graph') {
			// Document wins on conflict: reload the canvas and drop the
			// unmerged intents instead of merging stale topology.
			if (session.hasGraphConflict(store.dirty)) {
				loadStore();
				toasts.info(
					'Graph reloaded',
					'The document changed; unmerged canvas edits were discarded.',
				);
			} else {
				syncGraphToText();
			}
		}
		if (tab === 'form' && next !== 'form') syncFormToText();
		if (next === 'form' && tab !== 'form') {
			if (tab === 'graph') syncGraphToText();
			enterForm();
		}
		if (next === 'graph') {
			if (session.hasGraphConflict(store.dirty)) {
				conflictOpen = true;
				return false;
			}
			loadStore();
		}
		ontabchange(next);
		return true;
	}

	/** Load the graph store from the session snapshot. */
	function loadStore(): void {
		const snapshot = session.topology();
		store.load(snapshot.nodes, snapshot.edges);
		session.noteGraphLoaded();
		store.selectedId = null;
	}

	/**
	 * Merge the graph store back into the session document with lossless
	 * entry preservation; every other template field stays untouched.
	 */
	function syncGraphToText(): void {
		if (kind !== 'workflow') return;
		if (!store.dirty) return;
		session.mergeGraph(store.nodes, store.edges);
		editText = session.text;
	}

	function discardGraphChanges(): void {
		conflictOpen = false;
		loadStore();
		ontabchange('graph');
	}

	/**
	 * Route one issue to its editor: canvas focus for node hits, form tab
	 * for known field keys, JSON line for everything else.
	 */
	function locateIssue(issue: TemplateIssue): void {
		if (issue.nodeId && kind === 'workflow') {
			if (tab !== 'graph' && !switchTab('graph')) return;
			explorer?.focus(issue.nodeId);
			return;
		}
		if (issue.field && fields.some((field) => field.key === issue.field)) {
			if (tab !== 'form') switchTab('form');
			highlightField = issue.field;
			return;
		}
		if (tab !== 'json') switchTab('json');
		if (issue.field) {
			const line = jsonFieldLine(editText, issue.field);
			if (line) editor?.scrollToLine(line);
		}
	}
</script>

<div class="flex items-center justify-between gap-2">
	<Segmented
		items={kind === 'workflow'
			? [
					{ id: 'json', label: 'JSON' },
					{ id: 'form', label: 'Form' },
					{ id: 'graph', label: 'Graph' },
				]
			: [
					{ id: 'json', label: 'JSON' },
					{ id: 'form', label: 'Form' },
				]}
		value={tab}
		size="sm"
		onchange={(id) => switchTab(id as TemplateEditTab)}
	/>
	{#if validationState === 'valid'}
		<Badge variant="success">Valid</Badge>
	{:else if validationState === 'invalid'}
		<Badge variant="danger">Invalid</Badge>
	{/if}
</div>
{#if tab === 'form'}
	<div class="space-y-2">
		{#each fields as field (field.key)}
			<label class="block">
				<span
					class={highlightField === field.key
						? 'mb-1 block rounded bg-warning/10 px-1 text-caption text-muted-foreground ring-1 ring-warning'
						: 'mb-1 block text-caption text-muted-foreground'}
				>
					{field.label}{#if field.required}<span class="text-destructive">
							*</span
						>{/if}
				</span>
				{#if field.multiline}
					<Textarea
						bind:value={formState[field.key]}
						placeholder={field.label}
						class="min-h-16 text-small"
						oninput={() => {
							highlightField = null;
							syncFormToText();
						}}
					/>
				{:else}
					<Input
						bind:value={formState[field.key]}
						placeholder={field.label}
						size="sm"
						class="w-full"
						oninput={() => {
							highlightField = null;
							syncFormToText();
						}}
					/>
				{/if}
			</label>
		{/each}
		<p class="text-micro text-muted-foreground">
			Form edits the core fields; the remaining definition stays in JSON mode.
		</p>
	</div>
{:else if tab === 'graph'}
	{#if kind !== 'workflow'}
		<p class="text-caption text-muted-foreground">
			Graph preview is only available for workflow templates.
		</p>
	{:else if topology.error}
		<p class="text-caption text-destructive">
			JSON error: {topology.error}. Fix it in the JSON tab first.
		</p>
		<Button variant="outline" size="sm" onclick={() => switchTab('json')}>
			Back to JSON
		</Button>
	{:else if graphNodes.length === 0}
		<p class="text-caption text-muted-foreground">
			No nodes array in the definition yet.
		</p>
	{:else}
		<p class="text-micro text-muted-foreground">
			Drag nodes to move · double-click empty canvas to add a node · click an
			edge to delete it · shift-click another node to connect.
		</p>
		<GraphExplorer
			bind:this={explorer}
			nodes={store.nodes}
			edges={store.edges}
			preset="workflow"
			selectedId={store.selectedId}
			onselect={(id) => {
				store.selectedId = id;
			}}
			editable
			editMode
			editDirty={store.dirty}
			canUndo={store.canUndo}
			canRedo={store.canRedo}
			positions={store.positions}
			issueIds={serverIssueIds}
			onundo={() => store.undo()}
			onredo={() => store.redo()}
			onsave={() => onsave()}
			onmovenode={(id, position) => onmovenode(id, position)}
			onmovenodes={(moves) => onmovenodes(moves)}
			onaddnode={(position) => onaddnode(position)}
			ondeleteedge={(id) => store.removeEdge(id)}
			onconnect={(source, target) => onconnect(source, target)}
			ondeletenodes={(ids) => ondeletenodes(ids)}
			ondeletegroups={(ids) => ondeletegroups(ids)}
			onjumpparam={(id) => (store.selectedId = id)}
		/>
		{#if selectedNode}
			{#key store.selectedId}
				<div class="grid grid-cols-2 gap-2">
					<label class="block">
						<span class="mb-1 block text-caption text-muted-foreground"
							>Node name</span
						>
						<input
							value={selectedNode.label}
							placeholder="Node name"
							class="h-7 w-full rounded-md border border-input bg-card px-2.5 text-small"
							oninput={(event) => {
								const next = (event.currentTarget as HTMLInputElement).value;
								if (store.selectedId) {
									store.updateNode(store.selectedId, {
										label: next || store.selectedId,
									});
								}
							}}
						/>
					</label>
					<label class="block">
						<span class="mb-1 block text-caption text-muted-foreground"
							>Node type</span
						>
						<input
							value={selectedNode.kind}
							placeholder="STEP"
							class="h-7 w-full rounded-md border border-input bg-card px-2.5 text-small"
							oninput={(event) => {
								const next = (event.currentTarget as HTMLInputElement).value;
								if (store.selectedId) {
									store.updateNode(store.selectedId, {
										kind: next.trim() || 'STEP',
									});
								}
							}}
						/>
					</label>
				</div>
			{/key}
		{/if}
		{#if nodeSnippet}
			<pre
				class="max-h-48 overflow-auto rounded-md border border-border bg-muted/40 p-2 font-mono text-micro">{nodeSnippet}</pre>
			<p class="text-micro text-muted-foreground">
				Topology fields edit above; other stored fields merge back on save. {combinedIssues.length >
				0
					? `${combinedIssues.length} issue(s) need attention.`
					: ''}
			</p>
		{/if}
		{#if skeletonHints.length > 0}
			<ul
				class="space-y-1 rounded-md border border-warning/40 bg-warning/10 px-2 py-1.5"
			>
				{#each skeletonHints as hint, index (index)}
					<li class="text-caption text-warning">{hint}</li>
				{/each}
			</ul>
		{/if}
		<div class="flex flex-wrap items-center gap-2">
			<Button
				variant="outline"
				size="sm"
				disabled={draftBusy}
				onclick={() => onservercheck()}
			>
				{draftBusy ? 'Checking…' : 'Check with server rules'}
			</Button>
			<Button
				variant="outline"
				size="sm"
				disabled={openWorkflowBusy}
				onclick={() => onopenworkflow()}
			>
				{openWorkflowBusy ? 'Opening…' : 'Open as workflow'}
			</Button>
			<span class="text-micro text-muted-foreground">
				Graph edits merge into JSON on save; issues map back to graph nodes.
			</span>
		</div>
	{/if}
{:else}
	<JsonEditor
		bind:this={editor}
		bind:value={editText}
		{errorLine}
		placeholder={'{\n  "name": "my-template",\n  …\n}'}
		minHeight="16rem"
	/>
{/if}
{#if syntaxError}
	<div
		class="flex items-center justify-between gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5"
	>
		<p class="text-caption text-destructive">
			JSON syntax{#if errorLine}
				(line {errorLine}){/if}: {syntaxError}
		</p>
		{#if errorLine}
			<Button
				variant="ghost"
				size="sm"
				onclick={() => editor?.scrollToLine(errorLine ?? 1)}
			>
				Go to line
			</Button>
		{/if}
	</div>
{/if}
<IssueList
	issues={combinedIssues}
	onlocate={locateIssue}
	locatable={(issue) => issue.nodeId !== null || issue.field !== null}
/>
<div class="flex items-center gap-2">
	<Button variant="outline" size="sm" onclick={() => onvalidate()}>
		Validate
	</Button>
	<Button size="sm" disabled={saveBusy} onclick={() => onsave()}>
		{saveBusy ? 'Saving…' : isNew ? 'Create' : 'Save'}
	</Button>
	{#if !isNew}
		<Button variant="ghost" size="sm" onclick={() => oncancel()}>Cancel</Button>
	{/if}
</div>

<Dialog
	bind:open={conflictOpen}
	title="Graph reloaded from document"
	description="The document changed while the canvas had unsaved edits. The canvas reloaded from the document."
>
	<p class="text-caption text-muted-foreground">
		Unmerged canvas edits were discarded; the document is the single source of
		truth. No automatic merge is attempted.
	</p>
	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button size="sm" onclick={discardGraphChanges}>Got it</Button>
		</div>
	{/snippet}
</Dialog>
