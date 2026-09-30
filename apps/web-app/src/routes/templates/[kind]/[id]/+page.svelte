<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate, goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import StatusBadge from '$lib/components/ui/StatusBadge.svelte';
	import KeyValueList from '$lib/components/domain/KeyValueList.svelte';
	import TemplateEditPanel, {
		type TemplateEditTab,
	} from '$lib/components/domain/TemplateEditPanel.svelte';
	import UnsavedChangesDialog from '$lib/components/ui/UnsavedChangesDialog.svelte';
	import {
		deleteWorkflowDraft,
		saveWorkflowDraft,
		validateWorkflowDraft,
	} from '$lib/services/graph';
	import { GraphEditStore } from '$lib/graph/edit-store.svelte';
	import type { CanvasPosition } from '$lib/components/domain/GraphCanvas.svelte';
	import { createWorkflow } from '$lib/services/workflows';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import {
		allocateGraphNodeId,
		cloneTemplate,
		deleteTemplate,
		exportTemplate,
		getTemplateDetail,
		localTemplateIssues,
		recordTemplateUsage,
		saveTemplate,
		serverTemplateIssues,
		summarizeTemplate,
		templateBackendDefinition,
		templateFormFields,
		TemplateEditSession,
		validateTemplateDefinition,
		type TemplateDetail,
		type TemplateIssue,
	} from '$lib/services/templates';
	import type { TemplateKind } from '$lib/types/models';
	import { normalizeNodeType } from '$lib/graph/node-kind';
	import { nodeInsertStore } from '$lib/stores/node-insert.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatNumber, slugify } from '$lib/utils/format';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';

	const KINDS: readonly TemplateKind[] = [
		'node',
		'trigger',
		'agent',
		'workflow',
	];
	const VIEWS = ['preview', 'edit', 'graph'] as const;
	type DetailView = (typeof VIEWS)[number];

	function isKind(raw: string): raw is TemplateKind {
		return (KINDS as readonly string[]).includes(raw);
	}

	function isView(raw: string | undefined): raw is DetailView {
		return raw !== undefined && (VIEWS as readonly string[]).includes(raw);
	}

	const routeKindParam = $derived(page.params.kind ?? '');
	const routeIdParam = $derived(page.params.id ?? '');
	const kind = $derived<TemplateKind | null>(
		isKind(routeKindParam) ? routeKindParam : null,
	);
	const isNew = $derived(routeIdParam === 'new');

	let detail = $state<TemplateDetail | null>(null);
	let detailLoading = $state(true);
	let detailError = $state<string | null>(null);
	let loadedKey = $state('');

	let editMode = $state(false);
	let editTab = $state<TemplateEditTab>('json');
	let editText = $state('');
	let formState = $state<Record<string, string>>({});
	let syntaxError = $state<string | null>(null);
	let validationState = $state<'idle' | 'valid' | 'invalid'>('idle');
	let serverIssues = $state<TemplateIssue[]>([]);
	let validatedText = $state<string | null>(null);
	let draftBusy = $state(false);
	let saveBusy = $state(false);
	let deleteArmed = $state(false);
	let openWorkflowBusy = $state(false);

	// Controlled graph edit state for workflow templates. The canvas only
	// emits intents; every mutation lands here and re-renders from it.
	const templateEditStore = new GraphEditStore();
	const session = new TemplateEditSession();
	let graphRequest = $state(0);

	let createName = $state('');
	let createId = $state('');
	let createKind = $state<TemplateKind>('node');

	let unsavedOpen = $state(false);
	let pendingNavUrl = $state<string | null>(null);

	const formFields = $derived(templateFormFields(kind ?? 'node'));
	const summary = $derived(
		kind && detail ? summarizeTemplate(kind, detail.raw) : [],
	);
	const viewItems = $derived(
		kind === 'workflow'
			? [
					{ id: 'preview', label: 'Preview' },
					{ id: 'edit', label: 'Edit' },
					{ id: 'graph', label: 'Graph' },
				]
			: [
					{ id: 'preview', label: 'Preview' },
					{ id: 'edit', label: 'Edit' },
				],
	);

	// Mirror the editing state back to the address bar so the detail view is
	// refresh-safe and shareable. Gated on the settled detail so the entry
	// URL (?tab=edit) survives until the first load applies it.
	$effect(() => {
		if (detailLoading || kind === null || (detail === null && !isNew)) return;
		const view: DetailView = !editMode
			? 'preview'
			: editTab === 'graph'
				? 'graph'
				: 'edit';
		void gotoWithParams(page.url, { tab: view === 'preview' ? '' : view });
	});

	// Keep the session document fresh while typing; failed parses retain
	// the last valid document so the form and canvas never clear.
	$effect(() => {
		if (editMode && kind !== null) {
			session.applyText(editText);
			syntaxError = session.syntaxError;
		}
	});

	// Explicit check results only describe the validated text; any later
	// edit returns the badge and server issues to idle.
	$effect(() => {
		if (validatedText !== null && editText !== validatedText) {
			validatedText = null;
			serverIssues = [];
			validationState = 'idle';
		}
	});

	// Reload when the route identity changes (list navigation, clone jumps).
	// The view intent always comes from the live URL so clone and save jumps
	// land on the view their address carries.
	$effect(() => {
		const activeKind = kind;
		if (activeKind === null) {
			detailLoading = false;
			return;
		}
		const viewParam = parseListParams(page.url).tab;
		const view: DetailView = isView(viewParam)
			? viewParam
			: isNew
				? 'edit'
				: 'preview';
		if (isNew) {
			createKind = activeKind;
			session.loadEmpty(activeKind);
			formState = session.formSnapshot();
			editText = session.text;
			resetEditState();
			editMode = true;
			editTab = 'json';
			templateEditStore.load([], []);
			detail = null;
			detailError = null;
			detailLoading = false;
			loadedKey = `${activeKind}/new`;
			return;
		}
		const key = `${activeKind}/${routeIdParam}`;
		if (key === loadedKey) return;
		loadedKey = key;
		resetEditState();
		templateEditStore.load([], []);
		void loadDetail(activeKind, routeIdParam, view);
	});

	async function loadDetail(
		activeKind: TemplateKind,
		id: string,
		view: DetailView,
	): Promise<void> {
		detailLoading = true;
		detailError = null;
		try {
			detail = await getTemplateDetail(id, activeKind);
			if (view !== 'preview') {
				enterEdit(view === 'graph' ? 'graph' : 'json');
			} else {
				editMode = false;
			}
		} catch (e) {
			detailError = e instanceof Error ? e.message : 'Template detail failed.';
			detail = null;
		} finally {
			detailLoading = false;
		}
	}

	function enterEdit(tab: TemplateEditTab): void {
		if (!kind || !detail) return;
		if (tab === 'graph' && kind !== 'workflow') tab = 'json';
		editMode = true;
		editTab = tab;
		session.load(kind, detail.raw);
		formState = session.formSnapshot();
		editText = session.text;
		syntaxError = null;
		validationState = 'idle';
		serverIssues = [];
		validatedText = null;
		if (tab === 'graph') graphRequest += 1;
	}

	function exitEdit(): void {
		editMode = false;
		validationState = 'idle';
		serverIssues = [];
		validatedText = null;
		syntaxError = null;
	}

	/** Fold the active tab back into the session before validate or save. */
	function flushEditState(): void {
		if (!kind) return;
		if (editTab === 'form') {
			session.applyForm(formState);
			editText = session.text;
		} else if (
			editTab === 'graph' &&
			kind === 'workflow' &&
			templateEditStore.dirty
		) {
			session.mergeGraph(templateEditStore.nodes, templateEditStore.edges);
			editText = session.text;
		}
	}

	/**
	 * The session document is the single source of truth; canvas and form
	 * states are projections that fold back into it. Leaving the page only
	 * checks the session text against its baseline.
	 */
	function editDirty(): boolean {
		return editMode && session.textDirty;
	}

	function resetEditState(): void {
		syntaxError = null;
		validationState = 'idle';
		serverIssues = [];
		validatedText = null;
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
		templateEditStore.selectedId = target;
	}

	function handleTemplateDeleteNodes(ids: string[]): void {
		templateEditStore.removeNodes(ids);
	}

	function handleTemplateDeleteGroups(ids: string[]): void {
		for (const id of ids) templateEditStore.removeGroup(id);
		toasts.info(
			'Groups deleted',
			`${ids.length} group(s) removed from the canvas.`,
		);
	}

	function editedPayload(parsed: unknown): unknown {
		if (kind === 'node' || kind === 'trigger') return parsed;
		const base =
			detail?.raw && typeof detail.raw === 'object'
				? (detail.raw as Record<string, unknown>)
				: {};
		return { ...base, definition: parsed };
	}

	function buildBackendDefinition(value: unknown): Record<string, unknown> {
		return templateBackendDefinition(
			kind ?? 'node',
			value,
			`template-${isNew ? createId.trim() || 'new' : routeIdParam}`,
			`template-${isNew ? 'new' : routeIdParam}`,
		);
	}

	/**
	 * Server compile gate for workflow templates: save the JSON as a check
	 * draft, run the server rule set, map issues back to graph nodes, then
	 * delete the check draft so validation leaves no residue. Returns true
	 * when the server reports no issues.
	 */
	async function runTemplateServerGate(): Promise<boolean> {
		flushEditState();
		session.applyText(editText);
		syntaxError = session.syntaxError;
		const value = session.definition();
		const error = session.syntaxError;
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
		let draftId: string | null = null;
		try {
			draftId = await saveWorkflowDraft(definition);
			const issues = await validateWorkflowDraft(draftId);
			serverIssues = serverTemplateIssues(issues, session.topology().nodes);
			validatedText = editText;
			if (issues.length > 0) {
				toasts.warning(`Server reports ${issues.length} issue(s)`);
				return false;
			}
			toasts.success('Server validation passed');
			return true;
		} catch (e) {
			toasts.error(
				'Server validation failed',
				e instanceof Error ? e.message : undefined,
			);
			return false;
		} finally {
			if (draftId !== null) {
				await deleteWorkflowDraft(draftId).catch(() => undefined);
			}
			draftBusy = false;
		}
	}

	async function runValidate(): Promise<boolean> {
		if (!kind) return false;
		flushEditState();
		session.applyText(editText);
		const value = session.definition();
		const error = session.syntaxError;
		syntaxError = error;
		if (error || value === undefined) {
			validationState = 'invalid';
			return false;
		}
		const issues = await validateTemplateDefinition(kind, value);
		if (kind === 'workflow' || kind === 'agent') {
			serverIssues = issues;
		} else {
			serverIssues = [];
		}
		const combined = [...localTemplateIssues(kind, value), ...serverIssues];
		validationState = combined.length === 0 ? 'valid' : 'invalid';
		validatedText = editText;
		return combined.length === 0;
	}

	async function runSave(): Promise<void> {
		if (!kind) return;
		const valid = await runValidate();
		if (!valid) {
			toasts.error('Template invalid', 'Fix the reported issues first.');
			return;
		}
		if (kind === 'workflow') {
			const serverOk = await runTemplateServerGate();
			if (!serverOk) {
				editTab = 'graph';
				return;
			}
		}
		const value = session.definition();
		saveBusy = true;
		try {
			if (isNew) {
				const payload = value as Record<string, unknown>;
				const id = createId.trim() || slugify(createName);
				if (!id) throw new Error('Template id is required');
				const savedId = await saveTemplate(createKind, null, {
					...payload,
					id,
					name: createName.trim() || id,
				});
				toasts.success('Template created');
				await goto(
					resolve('/templates/[kind]/[id]', {
						kind: createKind,
						id: savedId || id,
					}),
				);
			} else {
				await saveTemplate(kind, routeIdParam, editedPayload(value));
				toasts.success('Template saved');
				session.markClean();
				exitEdit();
				await loadDetail(kind, routeIdParam, 'preview');
			}
		} catch (e) {
			toasts.error('Save failed', e instanceof Error ? e.message : undefined);
		} finally {
			saveBusy = false;
		}
	}

	async function runOpenAsWorkflow(): Promise<void> {
		if (kind !== 'workflow' || isNew) return;
		flushEditState();
		session.applyText(editText);
		syntaxError = session.syntaxError;
		const value = session.definition();
		const error = session.syntaxError;
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
					: `template-${routeIdParam}`;
			const created = await createWorkflow(name, definition);
			void recordTemplateUsage(routeIdParam).catch(() => undefined);
			toasts.success('Workflow created from template');
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

	async function runClone(): Promise<void> {
		if (!kind || !detail) return;
		try {
			const newId = await cloneTemplate(
				detail.template.id,
				kind,
				`${detail.template.name} (copy)`,
			);
			void recordTemplateUsage(detail.template.id).catch(() => undefined);
			toasts.success('Template cloned');
			await goto(resolve('/templates/[kind]/[id]', { kind, id: newId }));
		} catch (e) {
			toasts.error('Clone failed', e instanceof Error ? e.message : undefined);
		}
	}

	async function runExport(): Promise<void> {
		if (!kind || isNew) return;
		try {
			await exportTemplate(kind, routeIdParam);
			void recordTemplateUsage(routeIdParam).catch(() => undefined);
			toasts.success('Template exported');
		} catch (e) {
			toasts.error('Export failed', e instanceof Error ? e.message : undefined);
		}
	}

	async function backToLibrary(): Promise<void> {
		// eslint-disable-next-line svelte/no-navigation-without-resolve
		await goto('/templates');
	}

	async function runDelete(): Promise<void> {
		if (!kind || isNew) return;
		try {
			await deleteTemplate(kind, routeIdParam);
			toasts.success('Template deleted');
			await backToLibrary();
		} catch (e) {
			toasts.error('Delete failed', e instanceof Error ? e.message : undefined);
		} finally {
			deleteArmed = false;
		}
	}

	function requestView(next: DetailView): void {
		if (next === 'preview') {
			if (editDirty()) {
				unsavedOpen = true;
				return;
			}
			exitEdit();
			return;
		}
		if (isNew) {
			editTab = 'json';
			return;
		}
		if (!detail) return;
		enterEdit(next === 'graph' ? 'graph' : 'json');
	}

	/**
	 * Node type this template inserts as: the stored `node_type` normalised
	 * the way the backend will store it, or null when it is blank. Only node
	 * templates carry `node_type`; trigger templates describe a trigger, never
	 * a canvas node.
	 */
	const insertNodeType = $derived.by(() => {
		if (isNew || kind !== 'node' || !detail) return null;
		const raw =
			detail.raw && typeof detail.raw === 'object'
				? (detail.raw as Record<string, unknown>)
				: null;
		const value = raw?.node_type;
		return typeof value === 'string' ? normalizeNodeType(value) : null;
	});

	/**
	 * Queue the current node template for insertion into a workflow canvas,
	 * then send the user to the workflow list to pick the target. Only node
	 * templates can land on a canvas, and only when the stored `node_type` is
	 * a non-blank name.
	 */
	function insertIntoCanvas(): void {
		const nodeType = insertNodeType;
		if (!detail || nodeType === null) return;
		nodeInsertStore.set({ nodeType, name: detail.template.name });
		toasts.success(
			`${detail.template.name} queued`,
			'Open a workflow and enter edit mode to place it.',
		);
		void goto(resolve('/workflows'));
	}

	function discardEditsAndLeave(): void {
		unsavedOpen = false;
		exitEdit();
		const target = pendingNavUrl;
		pendingNavUrl = null;
		if (target) {
			// The target replays an intercepted navigation URL, which resolve() cannot rebuild.
			// eslint-disable-next-line svelte/no-navigation-without-resolve
			void goto(target);
		}
	}

	onMount(() => {
		const onBeforeUnload = (event: BeforeUnloadEvent): void => {
			if (editDirty()) event.preventDefault();
		};
		window.addEventListener('beforeunload', onBeforeUnload);
		return () => window.removeEventListener('beforeunload', onBeforeUnload);
	});

	beforeNavigate((navigation) => {
		if (navigation.willUnload) return;
		if (!editDirty()) return;
		navigation.cancel();
		pendingNavUrl = navigation.to?.url.toString() ?? null;
		unsavedOpen = true;
	});

	const kindOptions = [
		{ value: 'node', label: 'Node' },
		{ value: 'trigger', label: 'Trigger' },
		{ value: 'agent', label: 'Agent' },
		{ value: 'workflow', label: 'Workflow' },
	];
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={isNew
			? 'New template'
			: (detail?.template.name ?? 'Template detail')}
		description={isNew
			? 'Create a template in the library.'
			: kind
				? `${kind} template · ${routeIdParam}`
				: 'Template detail'}
	>
		{#snippet actions()}
			{#if insertNodeType !== null}
				<Button variant="outline" size="sm" onclick={insertIntoCanvas}>
					<Icon name="blocks" size={13} />
					Insert as {insertNodeType}
				</Button>
			{/if}
			<Button variant="ghost" size="sm" href="/templates">
				<Icon name="arrow-left" size={13} />
				Library
			</Button>
		{/snippet}
	</PageHeader>

	{#if kind === null}
		<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
			<ErrorState
				title="Unknown template kind"
				description={`Expected one of node, trigger, agent, workflow; got "${routeKindParam}".`}
			/>
		</div>
	{:else if detailLoading}
		<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
			<Skeleton lines={6} />
		</div>
	{:else if !isNew && detailError}
		<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
			<ErrorState
				title="Template detail failed to load"
				description={detailError ?? 'Template detail failed.'}
				onretry={() => {
					if (kind) void loadDetail(kind, routeIdParam, 'preview');
				}}
				class="rounded-lg border border-border bg-card"
			/>
		</div>
	{:else}
		{#if !isNew}
			<Segmented
				items={viewItems}
				value={!editMode ? 'preview' : editTab === 'graph' ? 'graph' : 'edit'}
				class="px-4"
				onchange={(id) => requestView(id as DetailView)}
			/>
		{/if}
		<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
			<div class="space-y-3">
				{#if isNew}
					<div class="grid grid-cols-2 gap-2">
						<Select
							value={createKind}
							options={kindOptions}
							size="sm"
							placeholder="Kind"
							onchange={(value) => {
								const next = value as TemplateKind;
								if (next === createKind) return;
								/* eslint-disable svelte/no-navigation-without-resolve -- resolve() cannot append the query string */
								void goto(
									resolve('/templates/[kind]/[id]', { kind: next, id: 'new' }) +
										'?tab=edit',
								);
								/* eslint-enable svelte/no-navigation-without-resolve */
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
					<TemplateEditPanel
						kind={createKind}
						tab={editTab}
						isNew
						{validationState}
						fields={templateFormFields(createKind)}
						bind:formState
						bind:editText
						{syntaxError}
						{serverIssues}
						{session}
						store={templateEditStore}
						{draftBusy}
						{saveBusy}
						{openWorkflowBusy}
						{graphRequest}
						ontabchange={(next) => (editTab = next)}
						onvalidate={() => void runValidate()}
						onsave={() => void runSave()}
						oncancel={() => void backToLibrary()}
						onservercheck={() => void runTemplateServerGate()}
						onopenworkflow={() => void runOpenAsWorkflow()}
						onmovenode={handleTemplateMoveNode}
						onmovenodes={handleTemplateMoveNodes}
						onaddnode={handleTemplateAddNode}
						onconnect={handleTemplateConnect}
						ondeletenodes={handleTemplateDeleteNodes}
						ondeletegroups={handleTemplateDeleteGroups}
					/>
				{:else if detail && !editMode}
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
					{#if summary.length > 0}
						<div>
							<span class="text-caption font-medium">Summary</span>
							<KeyValueList items={summary} dense />
						</div>
					{/if}
					<div>
						<span class="text-caption font-medium">Definition preview</span>
						<pre
							class="mt-1 max-h-72 overflow-auto rounded-md border border-border bg-muted/40 p-2 font-mono text-micro">{detail.definitionJson}</pre>
					</div>
					<div class="flex flex-wrap items-center gap-2">
						<Button
							variant="outline"
							size="sm"
							onclick={() => requestView('edit')}
						>
							<Icon name="pencil" size={13} />
							Edit
						</Button>
						<Button
							variant="outline"
							size="sm"
							onclick={() => void runExport()}
						>
							<Icon name="download" size={13} />
							Export
						</Button>
						<Button variant="outline" size="sm" onclick={() => void runClone()}>
							Clone as new
						</Button>
						{#if kind === 'workflow'}
							<Button
								variant="outline"
								size="sm"
								onclick={() => requestView('graph')}
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
				{:else if detail && kind}
					<TemplateEditPanel
						{kind}
						tab={editTab}
						isNew={false}
						{validationState}
						fields={formFields}
						bind:formState
						bind:editText
						{syntaxError}
						{serverIssues}
						{session}
						store={templateEditStore}
						{draftBusy}
						{saveBusy}
						{openWorkflowBusy}
						{graphRequest}
						ontabchange={(next) => (editTab = next)}
						onvalidate={() => void runValidate()}
						onsave={() => void runSave()}
						oncancel={exitEdit}
						onservercheck={() => void runTemplateServerGate()}
						onopenworkflow={() => void runOpenAsWorkflow()}
						onmovenode={handleTemplateMoveNode}
						onmovenodes={handleTemplateMoveNodes}
						onaddnode={handleTemplateAddNode}
						onconnect={handleTemplateConnect}
						ondeletenodes={handleTemplateDeleteNodes}
						ondeletegroups={handleTemplateDeleteGroups}
					/>
				{:else}
					<EmptyState
						icon="template"
						title="Template not found"
						description="It may have been deleted."
						class="rounded-lg border border-border bg-card"
					/>
				{/if}
			</div>
		</div>
	{/if}
</div>

<UnsavedChangesDialog
	bind:open={unsavedOpen}
	description="This template has unsaved edits. Leaving discards the JSON, form and canvas changes."
	ondiscard={discardEditsAndLeave}
/>
