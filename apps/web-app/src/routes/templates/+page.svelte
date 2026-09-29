<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate, goto } from '$app/navigation';
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
	import Sheet from '$lib/components/ui/Sheet.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import KeyValueList from '$lib/components/domain/KeyValueList.svelte';
	import TemplateEditPanel, {
		type TemplateEditTab,
	} from '$lib/components/domain/TemplateEditPanel.svelte';
	import TemplateImportDialog from '$lib/components/domain/TemplateImportDialog.svelte';
	import UnsavedChangesDialog from '$lib/components/domain/UnsavedChangesDialog.svelte';
	import {
		saveWorkflowDraft,
		validateWorkflowDraft,
	} from '$lib/services/graph';
	import { GraphEditStore } from '$lib/graph/edit-store.svelte';
	import type { CanvasPosition } from '$lib/components/domain/GraphCanvas.svelte';
	import { createWorkflow } from '$lib/services/workflows';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import {
		cloneTemplate,
		allocateGraphNodeId,
		deleteTemplate,
		exportTemplate,
		getTemplateDetail,
		importTemplate,
		listFeaturedTemplates,
		listTemplates,
		localTemplateIssues,
		normalizeTemplateForm,
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
	import type { Template, TemplateKind } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatNumber, slugify } from '$lib/utils/format';
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

	let unsavedOpen = $state(false);
	let pendingNavUrl = $state<string | null>(null);

	// Keep the session document fresh while typing; failed parses retain
	// the last valid document so the form and canvas never clear.
	$effect(() => {
		if (editMode || drawerId === null) {
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

	let createName = $state('');
	let createId = $state('');

	let importOpen = $state(false);
	let importKind = $state<TemplateKind>('node');
	let importText = $state('');
	let importError = $state<string | null>(null);
	let importBusy = $state(false);

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
		session.loadEmpty(row.kind);
		detail = null;
		detailError = null;
		deleteArmed = false;
		resetEditState();
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
		session.loadEmpty('node');
		formState = session.formSnapshot();
		createName = '';
		createId = '';
		editText = '';
		resetEditState();
		deleteArmed = false;
		templateEditStore.load([], []);
		drawerOpen = true;
	}

	/** Fold the active tab back into the session before validate or save. */
	function flushEditState(): void {
		if (editTab === 'form') {
			session.applyForm(formState);
			editText = session.text;
		} else if (
			editTab === 'graph' &&
			drawerKind === 'workflow' &&
			templateEditStore.dirty
		) {
			session.mergeGraph(templateEditStore.nodes, templateEditStore.edges);
			editText = session.text;
		}
	}

	/** Unsaved JSON, form and canvas changes block implicit drawer closes. */
	function editDirty(): boolean {
		if (!editMode) return false;
		if (session.textDirty) return true;
		if (templateEditStore.dirty) return true;
		return (
			JSON.stringify(normalizeTemplateForm(formState)) !==
			JSON.stringify(normalizeTemplateForm(session.formSnapshot()))
		);
	}

	function resetEditState(): void {
		editMode = false;
		syntaxError = null;
		validationState = 'idle';
		serverIssues = [];
		validatedText = null;
	}

	function requestDrawerClose(): void {
		if (editDirty()) {
			drawerOpen = true;
			unsavedOpen = true;
		}
	}

	function discardDrawerEdits(): void {
		unsavedOpen = false;
		resetEditState();
		drawerOpen = false;
		const target = pendingNavUrl;
		pendingNavUrl = null;
		// The target replays an intercepted navigation URL, which resolve() cannot rebuild.
		// eslint-disable-next-line svelte/no-navigation-without-resolve
		if (target) void goto(target);
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
		if (drawerKind === 'node' || drawerKind === 'trigger') return parsed;
		const base =
			detail?.raw && typeof detail.raw === 'object'
				? (detail.raw as Record<string, unknown>)
				: {};
		return { ...base, definition: parsed };
	}

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
		try {
			const draftId = await saveWorkflowDraft(definition);
			const issues = await validateWorkflowDraft(draftId);
			serverIssues = serverTemplateIssues(issues, session.topology().nodes);
			validatedText = editText;
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
		flushEditState();
		session.applyText(editText);
		const value = session.definition();
		const error = session.syntaxError;
		syntaxError = error;
		if (error || value === undefined) {
			validationState = 'invalid';
			return false;
		}
		const issues = await validateTemplateDefinition(drawerKind, value);
		if (drawerKind === 'workflow' || drawerKind === 'agent') {
			serverIssues = issues;
		} else {
			serverIssues = [];
		}
		const combined = [
			...localTemplateIssues(drawerKind, value),
			...serverIssues,
		];
		validationState = combined.length === 0 ? 'valid' : 'invalid';
		validatedText = editText;
		return combined.length === 0;
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
		const value = session.definition();
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

	async function runOpenAsWorkflow(): Promise<void> {
		if (drawerKind !== 'workflow') return;
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
		const onBeforeUnload = (event: BeforeUnloadEvent): void => {
			if (drawerOpen && editDirty()) event.preventDefault();
		};
		window.addEventListener('beforeunload', onBeforeUnload);
		return () => window.removeEventListener('beforeunload', onBeforeUnload);
	});

	beforeNavigate((navigation) => {
		if (navigation.willUnload) return;
		if (!drawerOpen || !editDirty()) return;
		navigation.cancel();
		pendingNavUrl = navigation.to?.url.toString() ?? null;
		unsavedOpen = true;
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
	onclose={requestDrawerClose}
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
							session.setKind(drawerKind);
							formState = session.formSnapshot();
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
				<TemplateEditPanel
					kind={drawerKind}
					tab={editTab}
					isNew={drawerId === null}
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
					oncancel={resetEditState}
					onservercheck={() => void runTemplateServerGate()}
					onopenworkflow={() => void runOpenAsWorkflow()}
					onmovenode={handleTemplateMoveNode}
					onmovenodes={handleTemplateMoveNodes}
					onaddnode={handleTemplateAddNode}
					onconnect={handleTemplateConnect}
					ondeletenodes={handleTemplateDeleteNodes}
					ondeletegroups={handleTemplateDeleteGroups}
				/>
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
							session.load(drawerKind, current.raw);
							formState = session.formSnapshot();
							editText = session.text;
							syntaxError = null;
							validationState = 'idle';
							serverIssues = [];
							validatedText = null;
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
								editTab = 'graph';
								session.load(drawerKind, current.raw);
								formState = session.formSnapshot();
								editText = session.text;
								syntaxError = null;
								validationState = 'idle';
								serverIssues = [];
								validatedText = null;
								graphRequest += 1;
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

<TemplateImportDialog
	bind:open={importOpen}
	bind:kind={importKind}
	bind:text={importText}
	error={importError}
	busy={importBusy}
	onimport={() => void runImport()}
	ondiscard={() => (importError = null)}
/>

<UnsavedChangesDialog
	bind:open={unsavedOpen}
	description="The template drawer has unsaved edits. Leaving discards the JSON, form and canvas changes."
	ondiscard={discardDrawerEdits}
/>
