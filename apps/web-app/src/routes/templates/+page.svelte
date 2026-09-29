<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import Sheet from '$lib/components/ui/Sheet.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Textarea from '$lib/components/ui/Textarea.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import JsonEditor from '$lib/components/domain/JsonEditor.svelte';
	import KeyValueList from '$lib/components/domain/KeyValueList.svelte';
	import GraphExplorer from '$lib/components/domain/GraphExplorer.svelte';
	import {
		saveWorkflowDraft,
		validateWorkflowDraft,
	} from '$lib/services/graph';
	import { issueNodeIds } from '$lib/graph/execution-projection';
	import { GraphEditStore } from '$lib/graph/edit-store.svelte';
	import type { DisplayEdge, DisplayNode } from '$lib/graph/display-model';
	import type { CanvasPosition } from '$lib/components/domain/GraphCanvas.svelte';
	import { createWorkflow } from '$lib/services/workflows';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import {
		cloneTemplate,
		allocateGraphNodeId,
		deleteTemplate,
		exportTemplate,
		formFromDefinition,
		formToDefinition,
		getTemplateDetail,
		importTemplate,
		jsonErrorLine,
		listFeaturedTemplates,
		listTemplates,
		mergeTemplateGraph,
		parseTemplateTopology,
		saveTemplate,
		summarizeTemplate,
		templateBackendDefinition,
		templateEditTarget,
		templateFormFields,
		validateTemplateDefinition,
		type TemplateDetail,
	} from '$lib/services/templates';
	import type { Template, TemplateKind } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatNumber } from '$lib/utils/format';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';

	const TABS = [
		{ id: 'all', label: 'All' },
		{ id: 'node', label: 'Node' },
		{ id: 'trigger', label: 'Trigger' },
		{ id: 'agent', label: 'Agent' },
		{ id: 'workflow', label: 'Workflow' },
	];

	const requestedTab = parseListParams(page.url).tab;
	let kind = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab)
			? requestedTab
			: 'all',
	);

	$effect(() => {
		gotoWithParams(page.url, { tab: kind === 'all' ? '' : kind });
	});

	let featuredOnly = $state(false);
	let templates = $state<Template[]>([]);
	let featuredIds = $state<Set<string>>(new Set());
	let listLoading = $state(true);
	let listError = $state<string | null>(null);

	let drawerOpen = $state(false);
	let drawerKind = $state<TemplateKind>('node');
	let drawerId = $state<string | null>(null);
	let detail = $state<TemplateDetail | null>(null);
	let detailLoading = $state(false);
	let detailError = $state<string | null>(null);

	let editMode = $state(false);
	type TemplateEditTab = 'json' | 'form' | 'graph';
	let editTab = $state<TemplateEditTab>('json');
	let editText = $state('');
	let formState = $state<Record<string, string>>({});
	let editor = $state<{
		scrollToLine: (line: number) => void;
	} | null>(null);
	let syntaxError = $state<string | null>(null);
	let validationState = $state<'idle' | 'valid' | 'invalid'>('idle');
	let validationIssues = $state<string[]>([]);
	let serverIssues = $state<string[]>([]);
	let serverIssueIds = $state<string[]>([]);
	let templateNodeId = $state<string | null>(null);
	let draftBusy = $state(false);
	let saveBusy = $state(false);
	let deleteArmed = $state(false);
	let openWorkflowBusy = $state(false);

	// Controlled graph edit state for workflow templates. The canvas only
	// emits intents; every mutation lands here and re-renders from it.
	const templateEditStore = new GraphEditStore();
	let graphSeed = $state('');
	let graphConflictOpen = $state(false);
	let templateExplorer = $state<{ focus: (id: string) => void } | null>(
		null,
	);

	let createName = $state('');
	let createId = $state('');

	let importOpen = $state(false);
	let importKind = $state<TemplateKind>('node');
	let importText = $state('');
	let importError = $state<string | null>(null);
	let importBusy = $state(false);
	let importEditor = $state<{
		scrollToLine: (line: number) => void;
	} | null>(null);

	const errorLine = $derived(
		syntaxError ? jsonErrorLine(editText, syntaxError) : null,
	);
	const importErrorLine = $derived(
		importError ? jsonErrorLine(importText, importError) : null,
	);
	const formFields = $derived(templateFormFields(drawerKind));
	const summary = $derived(
		detail ? summarizeTemplate(drawerKind, detail.raw) : [],
	);

	async function reload(): Promise<void> {
		listLoading = true;
		listError = null;
		try {
			const [all, featured] = await Promise.all([
				listTemplates({ kind: 'all' }),
				listFeaturedTemplates(),
			]);
			featuredIds = new Set(featured.map((t) => t.id));
			templates = all.map((t) => ({
				...t,
				featured: t.featured || featuredIds.has(t.id),
			}));
		} catch (e) {
			listError = e instanceof Error ? e.message : 'Templates failed.';
			templates = [];
		} finally {
			listLoading = false;
		}
	}

	async function openDetail(row: {
		id: string;
		kind: TemplateKind;
	}): Promise<void> {
		drawerKind = row.kind;
		drawerId = row.id;
		editMode = false;
		editTab = 'json';
		formState = {};
		detail = null;
		detailError = null;
		deleteArmed = false;
		validationState = 'idle';
		validationIssues = [];
		serverIssues = [];
		serverIssueIds = [];
		templateNodeId = null;
		syntaxError = null;
		graphSeed = '';
		graphConflictOpen = false;
		templateEditStore.load([], []);
		drawerOpen = true;
		await loadDetail(row.kind, row.id);
	}

	async function loadDetail(kind: TemplateKind, id: string): Promise<void> {
		detailLoading = true;
		detailError = null;
		try {
			detail = await getTemplateDetail(id, kind);
		} catch (e) {
			detailError = e instanceof Error ? e.message : 'Template detail failed.';
			detail = null;
		} finally {
			detailLoading = false;
		}
	}

	function startCreate(): void {
		drawerKind = 'node';
		drawerId = null;
		detail = null;
		editMode = true;
		editTab = 'json';
		formState = {};
		createName = '';
		createId = '';
		editText = '';
		syntaxError = null;
		validationState = 'idle';
		validationIssues = [];
		serverIssues = [];
		serverIssueIds = [];
		templateNodeId = null;
		deleteArmed = false;
		graphSeed = '';
		graphConflictOpen = false;
		templateEditStore.load([], []);
		drawerOpen = true;
	}

	function parseEditText(): { value: unknown; error: string | null } {
		try {
			return { value: JSON.parse(editText), error: null };
		} catch (e) {
			return {
				value: null,
				error: e instanceof Error ? e.message : 'Invalid JSON',
			};
		}
	}

	function enterForm(): void {
		const { value } = parseEditText();
		formState = formFromDefinition(drawerKind, templateEditTarget(drawerKind, value));
		editTab = 'form';
	}

	function syncFormToText(): void {
		const { value } = parseEditText();
		const current = templateEditTarget(drawerKind, value);
		const merged = formToDefinition(
			drawerKind,
			formState,
			current && typeof current === 'object' ? current : {},
		);
		if (drawerKind === 'node' || drawerKind === 'trigger') {
			editText = JSON.stringify(merged, null, 2);
			return;
		}
		const base =
			value && typeof value === 'object' && !Array.isArray(value)
				? (value as Record<string, unknown>)
				: {};
		editText = JSON.stringify({ ...base, definition: merged }, null, 2);
	}

	function switchEditTab(tab: TemplateEditTab): void {
		if (tab === 'graph' && drawerKind !== 'workflow') return;
		if (editTab === 'graph' && tab !== 'graph') {
			syncGraphToText();
		}
		if (editTab === 'form' && tab !== 'form') syncFormToText();
		if (tab === 'form' && editTab !== 'form') {
			if (editTab === 'graph') syncGraphToText();
			enterForm();
		}
		if (tab === 'graph') {
			if (
				templateEditStore.dirty &&
				editText !== graphSeed &&
				graphSeed !== ''
			) {
				graphConflictOpen = true;
				return;
			}
			loadTemplateStore();
		}
		editTab = tab;
	}

	/** Load the graph store from the current JSON text. */
	function loadTemplateStore(): void {
		templateEditStore.load(templateGraphNodes, templateGraphEdges);
		graphSeed = editText;
		templateNodeId = templateEditStore.selectedId;
	}

	/**
	 * Merge the graph store back into the JSON text with lossless entry
	 * preservation; every other template field stays untouched.
	 */
	function syncGraphToText(): void {
		if (drawerKind !== 'workflow') return;
		if (!templateEditStore.dirty && editText === graphSeed) return;
		let parsed: unknown;
		try {
			parsed = JSON.parse(editText);
		} catch {
			return;
		}
		const merged = mergeTemplateGraph(
			parsed,
			templateEditStore.nodes,
			templateEditStore.edges,
		);
		editText = JSON.stringify(merged, null, 2);
		graphSeed = editText;
	}

	function discardGraphChanges(): void {
		graphConflictOpen = false;
		loadTemplateStore();
		editTab = 'graph';
	}

	function keepGraphChanges(): void {
		graphConflictOpen = false;
		syncGraphToText();
		loadTemplateStore();
		editTab = 'graph';
	}

	function handleTemplateMoveNode(id: string, position: CanvasPosition): void {
		templateEditStore.applyMove(id, position);
	}

	function handleTemplateMoveNodes(
		moves: Array<{ id: string; position: CanvasPosition }>,
	): void {
		templateEditStore.applyMoves(moves);
	}

	function handleTemplateAddNode(position: CanvasPosition): void {
		const existing = new Set(templateEditStore.nodes.map((node) => node.id));
		const id = allocateGraphNodeId(existing);
		templateEditStore.addNode({ id, label: id, kind: 'STEP' }, position);
		templateEditStore.selectedId = id;
		templateNodeId = id;
	}

	function handleTemplateConnect(source: string, target: string): void {
		if (
			templateEditStore.edges.some(
				(edge) => edge.source === source && edge.target === target,
			)
		) {
			return;
		}
		const reason = templateEditStore.connect(source, target);
		if (reason) {
			toasts.info('Cannot connect', reason);
			return;
		}
		templateNodeId = target;
		templateEditStore.selectedId = target;
	}

	function handleTemplateDeleteNodes(ids: string[]): void {
		templateEditStore.removeNodes(ids);
		if (templateNodeId && ids.includes(templateNodeId)) templateNodeId = null;
	}

	function handleTemplateDeleteGroups(ids: string[]): void {
		for (const id of ids) templateEditStore.removeGroup(id);
		toasts.info(
			'Groups deleted',
			`${ids.length} group(s) removed from the canvas.`,
		);
	}

	const templateSelectedNode = $derived(
		templateNodeId
			? (templateEditStore.nodes.find((node) => node.id === templateNodeId) ?? null)
			: null,
	);

	const combinedTemplateIssues = $derived([
		...validationIssues,
		...serverIssues,
	]);

	function editedPayload(parsed: unknown): unknown {
		if (drawerKind === 'node' || drawerKind === 'trigger') return parsed;
		const base =
			detail?.raw && typeof detail.raw === 'object'
				? (detail.raw as Record<string, unknown>)
				: {};
		return { ...base, definition: parsed };
	}

	/** Parsed workflow topology from the JSON tab; errors stay on JSON. */
	const templateTopology = $derived.by(() => {
		if (drawerKind !== 'workflow') {
			return { nodes: [], edges: [], error: null as string | null };
		}
		let parsed: unknown;
		try {
			parsed = JSON.parse(editText);
		} catch (e) {
			return {
				nodes: [],
				edges: [],
				error: e instanceof Error ? e.message : 'Invalid JSON',
			};
		}
		const topology = parseTemplateTopology(parsed);
		return { ...topology, error: null as string | null };
	});

	const templateGraphNodes = $derived<DisplayNode[]>(templateTopology.nodes);
	const templateGraphEdges = $derived<DisplayEdge[]>(
		templateTopology.edges.filter((edge) => edge.source && edge.target),
	);

	const templateNodeSnippet = $derived.by(() => {
		if (!templateNodeId) return null;
		let parsed: unknown;
		try {
			parsed = JSON.parse(editText);
		} catch {
			return null;
		}
		const target = templateEditTarget(drawerKind, parsed);
		const topology = parseTemplateTopology(
			drawerKind === 'node' || drawerKind === 'trigger'
				? { definition: { nodes: [target], edges: [] } }
				: parsed,
		);
		const match = [...topology.nodes, ...topology.edges].find(
			(entry) => entry.id === templateNodeId,
		);
		if (!match) return null;
		const stored = templateEditStore.nodes.find(
			(node) => node.id === templateNodeId,
		);
		const live = stored ?? match;
		return JSON.stringify(live, null, 2);
	});

	function buildBackendDefinition(value: unknown): Record<string, unknown> {
		return templateBackendDefinition(
			drawerKind,
			value,
			`template-${drawerId ?? createId.trim() ?? 'new'}`,
			`template-${drawerId ?? 'new'}`,
		);
	}

	/**
	 * Server compile gate for workflow templates: save the JSON as a draft,
	 * run the server rule set, and map issues back to graph nodes. Returns
	 * true when the server reports no issues.
	 */
	async function runTemplateServerGate(): Promise<boolean> {
		if (editTab === 'form') syncFormToText();
		if (editTab === 'graph') syncGraphToText();
		const { value, error } = parseEditText();
		if (error || value === undefined) {
			syntaxError = error;
			toasts.error('Template invalid', 'Fix the JSON syntax first.');
			return false;
		}
		let definition: Record<string, unknown>;
		try {
			definition = buildBackendDefinition(value);
		} catch (e) {
			toasts.error(
				'Template invalid',
				e instanceof Error ? e.message : undefined,
			);
			return false;
		}
		draftBusy = true;
		try {
			const draftId = await saveWorkflowDraft(definition);
			const issues = await validateWorkflowDraft(draftId);
			serverIssues = issues.map((issue) => `${issue.field}: ${issue.message}`);
			serverIssueIds = [...issueNodeIds(issues, templateGraphNodes).keys()];
			if (issues.length > 0) {
				toasts.warning(`Server reports ${issues.length} issue(s)`);
				return false;
			}
			toasts.success('Server validation passed', `Draft ${draftId} saved.`);
			return true;
		} catch (e) {
			toasts.error(
				'Server validation failed',
				e instanceof Error ? e.message : undefined,
			);
			return false;
		} finally {
			draftBusy = false;
		}
	}

	async function runValidate(): Promise<boolean> {
		if (editTab === 'form') syncFormToText();
		if (editTab === 'graph') syncGraphToText();
		const { value, error } = parseEditText();
		syntaxError = error;
		if (error || value === undefined) {
			validationState = 'invalid';
			validationIssues = [];
			return false;
		}
		const issues = await validateTemplateDefinition(
			drawerKind,
			templateEditTarget(drawerKind, value),
		);
		validationIssues = issues;
		validationState = issues.length === 0 ? 'valid' : 'invalid';
		return issues.length === 0;
	}

	async function runSave(): Promise<void> {
		const valid = await runValidate();
		if (!valid) {
			toasts.error('Template invalid', 'Fix the reported issues first.');
			return;
		}
		if (drawerKind === 'workflow') {
			const serverOk = await runTemplateServerGate();
			if (!serverOk) {
				editTab = 'graph';
				return;
			}
		}
		const { value } = parseEditText();
		saveBusy = true;
		try {
			if (drawerId === null) {
				const payload = value as Record<string, unknown>;
				const id = createId.trim() || slugify(createName);
				if (!id) throw new Error('Template id is required');
				const savedId = await saveTemplate(drawerKind, null, {
					...payload,
					id,
					name: createName.trim() || id,
				});
				toasts.success('Template created');
				await reload();
				await openDetail({ id: savedId || id, kind: drawerKind });
			} else {
				await saveTemplate(drawerKind, drawerId, editedPayload(value));
				toasts.success('Template saved');
				editMode = false;
				await reload();
				await loadDetail(drawerKind, drawerId);
			}
		} catch (e) {
			toasts.error('Save failed', e instanceof Error ? e.message : undefined);
		} finally {
			saveBusy = false;
		}
	}

	function slugify(name: string): string {
		return name
			.trim()
			.toLowerCase()
			.replace(/[^a-z0-9]+/g, '-')
			.replace(/^-+|-+$/g, '');
	}

	async function runOpenAsWorkflow(): Promise<void> {
		if (drawerKind !== 'workflow') return;
		if (editTab === 'form') syncFormToText();
		if (editTab === 'graph') syncGraphToText();
		const { value, error } = parseEditText();
		if (error || value === undefined) {
			toasts.error('Template invalid', 'Fix the JSON syntax first.');
			return;
		}
		let definition: Record<string, unknown>;
		try {
			definition = buildBackendDefinition(value);
		} catch (e) {
			toasts.error(
				'Template invalid',
				e instanceof Error ? e.message : undefined,
			);
			return;
		}
		if (templateEditStore.dirty) {
			toasts.error('Unsaved graph changes', 'Save the template first.');
			return;
		}
		openWorkflowBusy = true;
		try {
			const name =
				typeof definition.name === 'string' && definition.name
					? definition.name
					: `template-${drawerId ?? 'new'}`;
			const created = await createWorkflow(name, definition);
			toasts.success('Workflow created from template');
			drawerOpen = false;
			// The path is resolve()d; resolve() cannot append the query string.
			// eslint-disable-next-line svelte/no-navigation-without-resolve
			await goto(resolve('/workflows/[id]', { id: created.id }) + '?tab=edit');
		} catch (e) {
			toasts.error(
				'Open as workflow failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			openWorkflowBusy = false;
		}
	}

	async function runClone(row: Template): Promise<void> {
		try {
			const newId = await cloneTemplate(row.id, row.kind, `${row.name} (copy)`);
			toasts.success('Template cloned');
			await reload();
			await openDetail({ ...row, id: newId });
		} catch (e) {
			toasts.error('Clone failed', e instanceof Error ? e.message : undefined);
		}
	}

	async function runExport(): Promise<void> {
		if (!drawerId) return;
		try {
			await exportTemplate(drawerKind, drawerId);
			toasts.success('Template exported');
		} catch (e) {
			toasts.error('Export failed', e instanceof Error ? e.message : undefined);
		}
	}

	async function runDelete(): Promise<void> {
		if (!drawerId) return;
		try {
			await deleteTemplate(drawerKind, drawerId);
			toasts.success('Template deleted');
			drawerOpen = false;
			await reload();
		} catch (e) {
			toasts.error('Delete failed', e instanceof Error ? e.message : undefined);
		} finally {
			deleteArmed = false;
		}
	}

	async function runImport(): Promise<void> {
		importError = null;
		let parsed: unknown;
		try {
			parsed = JSON.parse(importText);
		} catch (e) {
			importError = e instanceof Error ? e.message : 'Invalid JSON';
			return;
		}
		if (!parsed || typeof parsed !== 'object') {
			importError = 'Import body must be a JSON object';
			return;
		}
		importBusy = true;
		try {
			const newId = await importTemplate(importKind, importText);
			toasts.success('Template imported');
			importOpen = false;
			importText = '';
			await reload();
			await openDetail({ id: newId, kind: importKind });
		} catch (e) {
			importError = e instanceof Error ? e.message : 'Import failed.';
		} finally {
			importBusy = false;
		}
	}

	onMount(() => {
		void reload();
	});

	const filtered = $derived(
		templates.filter((template) => {
			const matchesKind = kind === 'all' || template.kind === kind;
			const matchesFeatured = !featuredOnly || template.featured;
			return matchesKind && matchesFeatured;
		}),
	);

	const KIND_ICON: Record<
		TemplateKind,
		'blocks' | 'zap' | 'sparkles' | 'workflow'
	> = {
		node: 'blocks',
		trigger: 'zap',
		agent: 'sparkles',
		workflow: 'workflow',
	};

	const kindOptions = [
		{ value: 'node', label: 'Node' },
		{ value: 'trigger', label: 'Trigger' },
		{ value: 'agent', label: 'Agent' },
		{ value: 'workflow', label: 'Workflow' },
	];
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Template library"
		description="Reusable node, trigger, agent and workflow templates from the registry."
	>
		{#snippet actions()}
			<IconButton
				icon="refresh"
				label="Refresh"
				onclick={() => void reload()}
			/>
			<Button variant="outline" size="sm" onclick={() => (importOpen = true)}>
				<Icon name="upload" size={13} />
				Import
			</Button>
			<Button size="sm" onclick={startCreate}>
				<Icon name="plus" size={13} />
				New template
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={kind} class="px-4">
		{#snippet trailing()}
			<Button
				variant={featuredOnly ? 'secondary' : 'ghost'}
				size="sm"
				onclick={() => (featuredOnly = !featuredOnly)}
			>
				<Icon name="star" size={13} />
				Featured
			</Button>
		{/snippet}
	</Segmented>

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if listLoading}
			<Skeleton lines={5} class="rounded-lg border border-border bg-card p-4" />
		{:else if listError}
			<ErrorState
				title="Templates failed to load"
				description={listError}
				onretry={() => void reload()}
				class="rounded-lg border border-border bg-card"
			/>
		{:else if filtered.length === 0}
			<EmptyState
				icon="template"
				title="No templates match"
				description="Switch the kind filter or disable the featured toggle."
				class="rounded-lg border border-border bg-card"
			/>
		{:else}
			<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
				{#each filtered as template (template.id)}
					<Card title={template.name}>
						{#snippet actions()}
							{#if template.featured}
								<Badge variant="warning" class="text-[0.625rem]">
									<Icon name="star" size={10} />
									featured
								</Badge>
							{/if}
						{/snippet}
						<p class="text-caption text-muted-foreground">
							{template.description}
						</p>
						<div class="mt-2 flex flex-wrap items-center gap-1.5">
							<span
								class="inline-flex items-center gap-1 rounded border border-border px-1.5 py-0.5 text-micro text-muted-foreground"
							>
								<Icon name={KIND_ICON[template.kind]} size={11} />
								{template.kind}
							</span>
							<span class="text-micro text-muted-foreground"
								>{template.category}</span
							>
						</div>
						<div class="mt-2 flex flex-wrap gap-1">
							{#each template.tags as tag (tag)}
								<Badge variant="outline" class="text-[0.625rem]">{tag}</Badge>
							{/each}
						</div>
						{#snippet footer()}
							<div class="flex items-center justify-between">
								<span class="tabular-nums"
									>{formatNumber(template.usage)} uses</span
								>
								<div class="flex items-center gap-1">
									<Button
										variant="ghost"
										size="sm"
										onclick={() => void runClone(template)}
									>
										Clone
									</Button>
									<Button
										variant="ghost"
										size="sm"
										onclick={() => void openDetail(template)}
									>
										Details
									</Button>
								</div>
							</div>
						{/snippet}
					</Card>
				{/each}
			</div>
		{/if}
	</div>
</div>

<Sheet
	bind:open={drawerOpen}
	title={drawerId === null
		? 'New template'
		: (detail?.template.name ?? 'Template detail')}
	width="32rem"
>
	{#if drawerId !== null && detailLoading}
		<Skeleton lines={6} />
	{:else if drawerId !== null && detailError}
		<ErrorState
			title="Template detail failed to load"
			description={detailError}
			onretry={() => {
				if (drawerId) void loadDetail(drawerKind, drawerId);
			}}
		/>
	{:else}
		<div class="space-y-3">
			{#if drawerId === null}
				<div class="grid grid-cols-2 gap-2">
					<Select
						value={drawerKind}
						options={kindOptions}
						size="sm"
						placeholder="Kind"
						onchange={(value) => {
							drawerKind = value as TemplateKind;
							formState = {};
						}}
					/>
					<input
						bind:value={createId}
						placeholder="Template id"
						class="h-7 rounded-md border border-input bg-card px-2.5 text-small"
					/>
				</div>
				<input
					bind:value={createName}
					placeholder="Template name"
					class="h-8 w-full rounded-md border border-input bg-card px-2.5 text-body"
				/>
			{:else if detail}
				<div class="flex flex-wrap items-center gap-1.5 text-caption">
					<StatusBadge status="active" size="sm" dot={false} />
					<span class="text-muted-foreground">{detail.template.kind}</span>
					<span class="text-muted-foreground">·</span>
					<span class="text-muted-foreground">
						version {detail.version ?? '—'}
					</span>
					<span class="text-muted-foreground">·</span>
					<span class="text-muted-foreground">
						{formatNumber(detail.template.usage)} uses
					</span>
				</div>
				{#if detail.template.description}
					<p class="text-caption text-muted-foreground">
						{detail.template.description}
					</p>
				{/if}
			{/if}

			{#if editMode || drawerId === null}
				<div class="flex items-center justify-between gap-2">
					<Segmented
						items={drawerKind === 'workflow'
							? [
									{ id: 'json', label: 'JSON' },
									{ id: 'form', label: 'Form' },
									{ id: 'graph', label: 'Graph' },
								]
							: [
									{ id: 'json', label: 'JSON' },
									{ id: 'form', label: 'Form' },
								]}
						value={editTab}
						size="sm"
						onchange={(id) => switchEditTab(id as TemplateEditTab)}
					/>
					{#if validationState === 'valid'}
						<Badge variant="success">Valid</Badge>
					{:else if validationState === 'invalid'}
						<Badge variant="danger">Invalid</Badge>
					{/if}
				</div>
				{#if editTab === 'form'}
					<div class="space-y-2">
						{#each formFields as field (field.key)}
							<label class="block">
								<span class="mb-1 block text-caption text-muted-foreground">
									{field.label}{#if field.required}<span
											class="text-destructive"
										>
											*</span
										>{/if}
								</span>
								{#if field.multiline}
									<Textarea
										bind:value={formState[field.key]}
										placeholder={field.label}
										class="min-h-16 text-small"
									/>
								{:else}
									<Input
										bind:value={formState[field.key]}
										placeholder={field.label}
										size="sm"
										class="w-full"
									/>
								{/if}
							</label>
						{/each}
						<p class="text-micro text-muted-foreground">
							Form edits the core fields; the remaining definition stays in JSON
							mode.
						</p>
					</div>
				{:else if editTab === 'graph'}
					{#if drawerKind !== 'workflow'}
						<p class="text-caption text-muted-foreground">
							Graph preview is only available for workflow templates.
						</p>
					{:else if templateTopology.error}
						<p class="text-caption text-destructive">
							JSON error: {templateTopology.error}. Fix it in the JSON tab
							first.
						</p>
						<Button
							variant="outline"
							size="sm"
							onclick={() => switchEditTab('json')}
						>
							Back to JSON
						</Button>
					{:else if templateGraphNodes.length === 0}
						<p class="text-caption text-muted-foreground">
							No nodes array in the definition yet.
						</p>
					{:else}
						<p class="text-micro text-muted-foreground">
							Drag nodes to move · double-click empty canvas to add a node ·
							click an edge to delete it · shift-click another node to connect.
						</p>
						<GraphExplorer
							bind:this={templateExplorer}
							nodes={templateEditStore.nodes}
							edges={templateEditStore.edges}
							preset="workflow"
							selectedId={templateNodeId}
							onselect={(id) => {
								templateNodeId = id;
								templateEditStore.selectedId = id;
							}}
							editable
							editMode
							editDirty={templateEditStore.dirty}
							canUndo={templateEditStore.canUndo}
							canRedo={templateEditStore.canRedo}
							positions={templateEditStore.positions}
							issueIds={serverIssueIds}
							onundo={() => templateEditStore.undo()}
							onredo={() => templateEditStore.redo()}
							onsave={() => void runSave()}
							onmovenode={handleTemplateMoveNode}
							onmovenodes={handleTemplateMoveNodes}
							onaddnode={handleTemplateAddNode}
							ondeleteedge={(id) => templateEditStore.removeEdge(id)}
							onconnect={handleTemplateConnect}
							ondeletenodes={handleTemplateDeleteNodes}
							ondeletegroups={handleTemplateDeleteGroups}
							onjumpparam={(id) => (templateNodeId = id)}
						/>
						{#if templateSelectedNode}
							{#key templateNodeId}
								<div class="grid grid-cols-2 gap-2">
									<label class="block">
										<span class="mb-1 block text-caption text-muted-foreground"
											>Node name</span
										>
										<input
											value={templateSelectedNode.label}
											placeholder="Node name"
											class="h-7 w-full rounded-md border border-input bg-card px-2.5 text-small"
											oninput={(event) => {
												const next = (event.currentTarget as HTMLInputElement)
													.value;
												if (templateNodeId) {
													templateEditStore.updateNode(templateNodeId, {
														label: next || templateNodeId,
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
											value={templateSelectedNode.kind}
											placeholder="STEP"
											class="h-7 w-full rounded-md border border-input bg-card px-2.5 text-small"
											oninput={(event) => {
												const next = (event.currentTarget as HTMLInputElement)
													.value;
												if (templateNodeId) {
													templateEditStore.updateNode(templateNodeId, {
														kind: next.trim() || 'STEP',
													});
												}
											}}
										/>
									</label>
								</div>
							{/key}
						{/if}
						{#if templateNodeSnippet}
							<pre
								class="max-h-48 overflow-auto rounded-md border border-border bg-muted/40 p-2 font-mono text-micro">{templateNodeSnippet}</pre>
							<p class="text-micro text-muted-foreground">
								Topology fields edit above; other stored fields merge back on
								save. {combinedTemplateIssues.length > 0
									? `${combinedTemplateIssues.length} issue(s) need attention.`
									: ''}
							</p>
						{/if}
						{#if serverIssues.length > 0}
							<ul
								class="space-y-1 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5"
							>
								{#each serverIssues as issue, index (index)}
									<li
										class="flex items-center justify-between gap-2 text-caption text-destructive"
									>
										<span class="min-w-0 truncate">{issue}</span>
										<button
											type="button"
											class="shrink-0 underline-offset-2 hover:underline"
											onclick={() => {
												const target = serverIssueIds.find((id) =>
													issue.includes(id),
												);
												if (target) templateExplorer?.focus(target);
											}}
										>
											Locate
										</button>
									</li>
								{/each}
							</ul>
						{/if}
						<div class="flex flex-wrap items-center gap-2">
							<Button
								variant="outline"
								size="sm"
								disabled={draftBusy}
								onclick={() => void runTemplateServerGate()}
							>
								{draftBusy ? 'Checking…' : 'Check with server rules'}
							</Button>
							<Button
								variant="outline"
								size="sm"
								disabled={openWorkflowBusy}
								onclick={() => void runOpenAsWorkflow()}
							>
								{openWorkflowBusy ? 'Opening…' : 'Open as workflow'}
							</Button>
							<span class="text-micro text-muted-foreground">
								Graph edits merge into JSON on save; issues map back to
								graph nodes.
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
				{#if validationIssues.length > 0}
					<ul
						class="space-y-1 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5"
					>
						{#each validationIssues as issue, index (index)}
							<li class="text-caption text-destructive">{issue}</li>
						{/each}
					</ul>
				{/if}
				<div class="flex items-center gap-2">
					<Button
						variant="outline"
						size="sm"
						onclick={() => void runValidate()}
					>
						Validate
					</Button>
					<Button size="sm" disabled={saveBusy} onclick={() => void runSave()}>
						{saveBusy ? 'Saving…' : drawerId === null ? 'Create' : 'Save'}
					</Button>
					{#if drawerId !== null}
						<Button
							variant="ghost"
							size="sm"
							onclick={() => {
								editMode = false;
								syntaxError = null;
								validationState = 'idle';
								validationIssues = [];
							}}
						>
							Cancel
						</Button>
					{/if}
				</div>
			{:else if detail}
				{@const current = detail}
				{#if summary.length > 0}
					<div>
						<span class="text-caption font-medium">Summary</span>
						<KeyValueList items={summary} dense />
					</div>
				{/if}
				<div>
					<span class="text-caption font-medium">Definition preview</span>
					<pre
						class="mt-1 max-h-72 overflow-auto rounded-md border border-border bg-muted/40 p-2 font-mono text-micro">{current.definitionJson}</pre>
				</div>
				<div class="flex flex-wrap items-center gap-2">
					<Button
						variant="outline"
						size="sm"
						onclick={() => {
							editMode = true;
							editTab = 'json';
							formState = {};
							editText =
								drawerKind === 'node' || drawerKind === 'trigger'
									? JSON.stringify(current.raw, null, 2)
									: current.definitionJson;
							syntaxError = null;
							validationState = 'idle';
							validationIssues = [];
						}}
					>
						<Icon name="pencil" size={13} />
						Edit
					</Button>
					<Button variant="outline" size="sm" onclick={() => void runExport()}>
						<Icon name="download" size={13} />
						Export
					</Button>
					{#if drawerKind === 'workflow'}
						<Button
							variant="outline"
							size="sm"
							onclick={() => {
								editMode = true;
								formState = {};
								editText = current.definitionJson;
								syntaxError = null;
								validationState = 'idle';
								validationIssues = [];
								serverIssues = [];
								serverIssueIds = [];
								templateNodeId = null;
								templateEditStore.load([], []);
								graphSeed = '';
								switchEditTab('graph');
							}}
						>
							Edit graph
						</Button>
					{/if}
					{#if deleteArmed}
						<Button size="sm" onclick={() => void runDelete()}>
							Confirm delete
						</Button>
						<Button
							variant="ghost"
							size="sm"
							onclick={() => (deleteArmed = false)}
						>
							Keep
						</Button>
					{:else}
						<Button
							variant="ghost"
							size="sm"
							onclick={() => (deleteArmed = true)}
						>
							Delete
						</Button>
					{/if}
				</div>
			{/if}
		</div>
	{/if}
	{#snippet footer()}
		{#if detail}
			{@const snapshot = detail}
			<Button
				variant="outline"
				size="sm"
				onclick={() => {
					drawerOpen = false;
					void runClone(snapshot.template);
				}}
			>
				Clone as new
			</Button>
		{/if}
	{/snippet}
</Sheet>

<Dialog
	bind:open={graphConflictOpen}
	title="Graph and JSON diverge"
	description="The graph has unsaved changes and the JSON changed underneath. Choose which side to keep."
>
	<p class="text-caption text-muted-foreground">
		Overwrite the JSON with the graph, or discard the graph and reload from
		the JSON. No automatic merge is attempted.
	</p>
	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button variant="ghost" size="sm" onclick={discardGraphChanges}>
				Discard graph
			</Button>
			<Button size="sm" onclick={keepGraphChanges}>Keep graph</Button>
		</div>
	{/snippet}
</Dialog>

<Dialog
	bind:open={importOpen}
	title="Import template"
	description="Import any template kind from JSON text."
>
	<Select
		value={importKind}
		options={[
			{ value: 'node', label: 'Node' },
			{ value: 'trigger', label: 'Trigger' },
			{ value: 'agent', label: 'Agent' },
			{ value: 'workflow', label: 'Workflow' },
		]}
		size="sm"
		placeholder="Kind"
		class="mb-2 w-40"
		onchange={(value) => (importKind = value as TemplateKind)}
	/>
	<JsonEditor
		bind:this={importEditor}
		bind:value={importText}
		errorLine={importErrorLine}
		placeholder={'{\n  "id": "my-template",\n  …\n}'}
		label="Import template JSON"
		minHeight="12rem"
	/>
	{#if importError}
		<div
			class="mt-2 flex items-center justify-between gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5"
		>
			<p class="text-caption text-destructive">
				JSON syntax{#if importErrorLine}
					(line {importErrorLine}){/if}: {importError}
			</p>
			{#if importErrorLine}
				<Button
					variant="ghost"
					size="sm"
					onclick={() => importEditor?.scrollToLine(importErrorLine ?? 1)}
				>
					Go to line
				</Button>
			{/if}
		</div>
	{/if}
	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button variant="ghost" size="sm" onclick={() => (importOpen = false)}>
				Cancel
			</Button>
			<Button size="sm" disabled={importBusy} onclick={() => void runImport()}>
				{importBusy ? 'Importing…' : 'Import'}
			</Button>
		</div>
	{/snippet}
</Dialog>
