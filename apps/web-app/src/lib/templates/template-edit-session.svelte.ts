import { SvelteSet } from 'svelte/reactivity';
import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import {
	deleteDefinitionDraft,
	saveDefinitionDraft,
	validateDefinitionDraft,
} from '#lib/services/definitions';
import { GraphEditStore } from '#lib/graph/edit-store.svelte';
import { EditLockStore } from '#lib/stores/edit-lock.svelte';
import type { CanvasPosition } from '#lib/graph/canvas-model';
import {
	cloneWorkflow,
	createWorkflow,
	deleteWorkflow,
	replaceWorkflow,
} from '#lib/services/workflows';
import {
	allocateGraphNodeId,
	cloneTemplate,
	deleteTemplate,
	exportTemplate,
	localTemplateIssues,
	recordTemplateUsage,
	requiresServerCompileGate,
	saveTemplate,
	serverTemplateIssues,
	supportsDefinitionLock,
	templateBackendDefinition,
	templateFormFields,
	TemplateEditSession,
	validateTemplateDefinition,
	type TemplateDetail,
	type TemplateIssue,
} from '#lib/services/templates';
import type { TemplateEditTab, TemplateKind } from '#lib/types/models';
import { normalizeNodeType } from '#lib/graph/node-kind';
import { nodeInsertStore } from '#lib/stores/node-insert.svelte';
import { toasts } from '#lib/stores/toast.svelte';
import { slugify } from '#lib/utils/format';

export const VIEWS = ['preview', 'edit', 'graph'] as const;
export type DetailView = (typeof VIEWS)[number];

const KIND_OPTIONS = [
	{ value: 'node', label: 'Node' },
	{ value: 'trigger', label: 'Trigger' },
	{ value: 'agent', label: 'Agent' },
	{ value: 'workflow', label: 'Workflow' },
];

/**
 * Read access to the route identity the controller acts on. The route owns
 * the params and detail loading; the controller only observes them through
 * these getters and drives reloads via loadDetail.
 */
export interface TemplateEditRouteContext {
	kind: () => TemplateKind | null;
	isNew: () => boolean;
	detail: () => TemplateDetail | null;
	routeIdParam: () => string;
	loadDetail: (activeKind: TemplateKind, id: string, view: DetailView) => Promise<void>;
}

/**
 * Edit-side controller for the template detail page. Holds every editing
 * concern (JSON/form/graph state, validation, save and lifecycle actions)
 * so the route stays a thin composition layer that only owns navigation
 * guards and detail loading.
 *
 * Editing follows the unified definition state machine (lock, draft,
 * validate, promote): every kind acquires the single-writer lock and
 * passes the server compile gate before save.
 */
export class TemplateEditController {
	private context: TemplateEditRouteContext;

	editMode = $state(false);
	editTab = $state<TemplateEditTab>('json');
	editText = $state('');
	formState = $state<Record<string, string>>({});
	syntaxError = $state<string | null>(null);
	validationState = $state<'idle' | 'valid' | 'invalid'>('idle');
	serverIssues = $state<TemplateIssue[]>([]);
	validatedText = $state<string | null>(null);
	draftBusy = $state(false);
	saveBusy = $state(false);
	deleteArmed = $state(false);

	// Single-writer lock for the definition being edited. Every kind answers
	// the same lock endpoint shape; null only while creating or when no
	// detail is loaded.
	lockStore = $state<EditLockStore | null>(null);

	// Controlled graph edit state for workflow templates. The canvas only
	// emits intents; every mutation lands here and re-renders from it.
	// Positions stay ephemeral here: templates have no layout endpoint, so
	// `layoutDirty` is intentionally unwired and only the session document
	// guards unsaved changes.
	templateEditStore = new GraphEditStore();
	session = new TemplateEditSession();
	graphRequest = $state(0);

	createName = $state('');
	createId = $state('');
	createKind = $state<TemplateKind>('node');

	unsavedOpen = $state(false);
	pendingNavUrl = $state<string | null>(null);

	constructor(context: TemplateEditRouteContext) {
		this.context = context;
	}

	/**
	 * Route `$effect`: keep the session document fresh while typing; failed
	 * parses retain the last valid document so the form and canvas never
	 * clear.
	 */
	watchSessionText(): void {
		if (this.editMode && this.kind !== null) {
			this.session.applyText(this.editText);
			this.syntaxError = this.session.syntaxError;
		}
	}

	/**
	 * Route `$effect`: explicit check results only describe the validated
	 * text; any later edit returns the badge and server issues to idle.
	 */
	watchValidationReset(): void {
		if (this.validatedText !== null && this.editText !== this.validatedText) {
			this.validatedText = null;
			this.serverIssues = [];
			this.validationState = 'idle';
		}
	}

	get kind(): TemplateKind | null {
		return this.context.kind();
	}

	get isNew(): boolean {
		return this.context.isNew();
	}

	get detail(): TemplateDetail | null {
		return this.context.detail();
	}

	get routeIdParam(): string {
		return this.context.routeIdParam();
	}

	/** Form fields for the kind being edited (detail kind, or the create kind). */
	formFields = $derived(templateFormFields(this.kind ?? 'node'));

	/** Segmented switcher entries; only workflow templates expose the graph. */
	viewItems = $derived(
		this.kind === 'workflow'
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

	kindOptions = KIND_OPTIONS;

	enterEdit(tab: TemplateEditTab): void {
		const kind = this.kind;
		const detail = this.detail;
		if (!kind || !detail) return;
		if (tab === 'graph' && kind !== 'workflow') tab = 'json';
		this.editMode = true;
		this.editTab = tab;
		this.session.load(kind, detail.raw);
		this.formState = this.session.formSnapshot();
		this.editText = this.session.text;
		this.syntaxError = null;
		this.validationState = 'idle';
		this.serverIssues = [];
		this.validatedText = null;
		if (tab === 'graph') {
			this.graphRequest += 1;
			this.consumePendingInsert();
		}
		void this.acquireEditLock();
	}

	exitEdit(): void {
		this.editMode = false;
		this.validationState = 'idle';
		this.serverIssues = [];
		this.validatedText = null;
		this.syntaxError = null;
		if (this.lockStore) {
			this.lockStore.acknowledgeLockLoss();
			void this.lockStore.release();
		}
	}

	/** Whether the current edit may write: unlocked kinds always may. */
	get canWrite(): boolean {
		if (!this.lockStore) return true;
		return this.lockStore.canWrite;
	}

	/** Human-readable holder for the lock badge; empty when lock-free. */
	get lockHolder(): string {
		return this.lockStore?.displayHolder ?? '';
	}

	private lockKind(): 'workflow' | 'agent' | 'node' | 'trigger' | null {
		// Template kinds share values with definition kinds; the lock endpoint
		// shape is identical for every kind.
		return this.kind;
	}

	private async acquireEditLock(): Promise<void> {
		const kind = this.kind;
		if (!kind || !supportsDefinitionLock(kind)) {
			this.lockStore = null;
			return;
		}
		if (this.isNew) {
			this.lockStore = null;
			return;
		}
		const lockKind = this.lockKind();
		if (!lockKind) {
			this.lockStore = null;
			return;
		}
		const store = new EditLockStore(lockKind);
		this.lockStore = store;
		store.watch(this.routeIdParam);
		const ok = await store.acquire();
		if (!ok) {
			toasts.warning(
				'Template is being edited',
				`Held by ${store.displayHolder}. Canvas is read-only until the lock is free.`,
			);
		}
	}

	/** Fold the active tab back into the session before validate or save. */
	flushEditState(): void {
		const kind = this.kind;
		if (!kind) return;
		if (this.editTab === 'form') {
			this.session.applyForm(this.formState);
			this.editText = this.session.text;
		} else if (this.editTab === 'graph' && kind === 'workflow' && this.templateEditStore.dirty) {
			this.session.mergeGraph(this.templateEditStore.nodes, this.templateEditStore.edges);
			this.editText = this.session.text;
		}
	}

	/**
	 * The session document is the single source of truth; canvas and form
	 * states are projections that fold back into it. Leaving the page only
	 * checks the session text against its baseline.
	 */
	editDirty(): boolean {
		return this.editMode && this.session.textDirty;
	}

	resetEditState(): void {
		this.syntaxError = null;
		this.validationState = 'idle';
		this.serverIssues = [];
		this.validatedText = null;
	}

	/** Seed the controller for the /new route: an empty session in edit mode. */
	startNewTemplate(activeKind: TemplateKind): void {
		this.createKind = activeKind;
		this.session.loadEmpty(activeKind);
		this.formState = this.session.formSnapshot();
		this.editText = this.session.text;
		this.resetEditState();
		this.editMode = true;
		this.editTab = 'json';
		this.templateEditStore.load([], []);
	}

	handleTemplateMoveNode(id: string, position: CanvasPosition): void {
		this.templateEditStore.applyMove(id, position);
	}

	handleTemplateMoveNodes(moves: Array<{ id: string; position: CanvasPosition }>): void {
		this.templateEditStore.applyMoves(moves);
	}

	handleTemplateAddNode(position: CanvasPosition): void {
		const store = this.templateEditStore;
		const existing = new SvelteSet(store.nodes.map((node) => node.id));
		const id = allocateGraphNodeId(existing);
		store.addNode({ id, label: id, kind: 'STEP' }, position);
		store.selectedId = id;
	}

	handleTemplateConnect(source: string, target: string): void {
		const store = this.templateEditStore;
		if (store.edges.some((edge) => edge.source === source && edge.target === target)) {
			return;
		}
		const reason = store.connect(source, target);
		if (reason) {
			toasts.info('Cannot connect', reason);
			return;
		}
		store.selectedId = target;
	}

	handleTemplateDeleteNodes(ids: string[]): void {
		this.templateEditStore.removeNodes(ids);
	}

	handleTemplateDeleteGroups(ids: string[]): void {
		const store = this.templateEditStore;
		for (const id of ids) store.removeGroup(id);
		toasts.info('Groups deleted', `${ids.length} group(s) removed from the canvas.`);
	}

	editedPayload(parsed: unknown): unknown {
		const kind = this.kind;
		if (kind === 'node' || kind === 'trigger') return parsed;
		const detail = this.detail;
		const base =
			detail?.raw && typeof detail.raw === 'object' ? (detail.raw as Record<string, unknown>) : {};
		return { ...base, definition: parsed };
	}

	buildBackendDefinition(value: unknown): Record<string, unknown> {
		const isNew = this.isNew;
		return templateBackendDefinition(
			this.kind ?? 'node',
			value,
			`template-${isNew ? this.createId.trim() || 'new' : this.routeIdParam}`,
			`template-${isNew ? 'new' : this.routeIdParam}`,
		);
	}

	/**
	 * Server compile gate for workflow templates: save the JSON as a check
	 * draft, run the server rule set, map issues back to graph nodes, then
	 * delete the check draft so validation leaves no residue. Returns true
	 * when the server reports no issues.
	 */
	async runTemplateServerGate(): Promise<boolean> {
		this.flushEditState();
		this.session.applyText(this.editText);
		this.syntaxError = this.session.syntaxError;
		const value = this.session.definition();
		const error = this.session.syntaxError;
		if (error || value === undefined) {
			this.syntaxError = error;
			toasts.error('Template invalid', 'Fix the JSON syntax first.');
			return false;
		}
		let definition: Record<string, unknown>;
		try {
			definition = this.buildBackendDefinition(value);
		} catch (e) {
			toasts.error('Template invalid', e instanceof Error ? e.message : undefined);
			return false;
		}
		this.draftBusy = true;
		let draftId: string | null = null;
		try {
			draftId = await saveDefinitionDraft('workflow', definition);
			const issues = await validateDefinitionDraft('workflow', draftId);
			this.serverIssues = serverTemplateIssues(issues, this.session.topology().nodes);
			this.validatedText = this.editText;
			if (issues.length > 0) {
				toasts.warning(`Server reports ${issues.length} issue(s)`);
				return false;
			}
			toasts.success('Server validation passed');
			return true;
		} catch (e) {
			toasts.error('Server validation failed', e instanceof Error ? e.message : undefined);
			return false;
		} finally {
			if (draftId !== null) {
				await deleteDefinitionDraft('workflow', draftId).catch(() => undefined);
			}
			this.draftBusy = false;
		}
	}

	async runValidate(): Promise<boolean> {
		const kind = this.kind;
		if (!kind) return false;
		this.flushEditState();
		this.session.applyText(this.editText);
		const value = this.session.definition();
		const error = this.session.syntaxError;
		this.syntaxError = error;
		if (error || value === undefined) {
			this.validationState = 'invalid';
			return false;
		}
		// Every kind validates through a server dry-run endpoint; local field
		// checks are instant hints only and never replace the gate.
		const issues = await validateTemplateDefinition(kind, value);
		this.serverIssues = issues;
		const combined = [...localTemplateIssues(kind, value), ...this.serverIssues];
		this.validationState = combined.length === 0 ? 'valid' : 'invalid';
		this.validatedText = this.editText;
		return combined.length === 0;
	}

	async runSave(): Promise<void> {
		const kind = this.kind;
		if (!kind) return;
		if (this.lockStore && !this.lockStore.canWrite) {
			toasts.warning('Save blocked', `Held by ${this.lockStore.displayHolder}.`);
			return;
		}
		const valid = await this.runValidate();
		if (!valid) {
			toasts.error('Template invalid', 'Fix the reported issues first.');
			return;
		}
		// The dry-run gate already ran inside runValidate for every kind.
		// Workflow graphs additionally pass the draft compile gate below
		// before the save lands, so graph rule violations map back to nodes.
		if (requiresServerCompileGate(kind) && kind === 'workflow') {
			const serverOk = await this.runTemplateServerGate();
			if (!serverOk) {
				this.editTab = 'graph';
				return;
			}
		}
		const value = this.session.definition();
		this.saveBusy = true;
		try {
			if (this.isNew) {
				const id = this.createId.trim() || slugify(this.createName);
				if (!id) throw new Error('Template id is required');
				const name = this.createName.trim() || id;
				let savedId: string;
				if (this.createKind === 'workflow') {
					const definition = templateBackendDefinition(this.createKind, value, id, name);
					const created = await createWorkflow(name, { ...definition, id, name });
					savedId = created.id || id;
				} else {
					const payload = value as Record<string, unknown>;
					savedId = await saveTemplate(this.createKind, null, {
						...payload,
						id,
						name,
					});
					if (!savedId) savedId = id;
				}
				toasts.success('Template created');
				await goto(
					resolve('/templates/[kind]/[id]', {
						kind: this.createKind,
						id: savedId,
					}),
				);
			} else if (kind === 'workflow') {
				const definition = templateBackendDefinition(kind, value, this.routeIdParam, this.routeIdParam);
				await replaceWorkflow(this.routeIdParam, { ...definition, id: this.routeIdParam });
				toasts.success('Template saved');
				this.session.markClean();
				this.exitEdit();
				await this.context.loadDetail(kind, this.routeIdParam, 'preview');
			} else {
				await saveTemplate(kind, this.routeIdParam, this.editedPayload(value));
				toasts.success('Template saved');
				this.session.markClean();
				this.exitEdit();
				await this.context.loadDetail(kind, this.routeIdParam, 'preview');
			}
		} catch (e) {
			toasts.error('Save failed', e instanceof Error ? e.message : undefined);
		} finally {
			this.saveBusy = false;
		}
	}

	async runClone(): Promise<void> {
		const kind = this.kind;
		const detail = this.detail;
		if (!kind || !detail) return;
		try {
			const newId =
				kind === 'workflow'
					? await cloneWorkflow(detail.template.id)
					: await cloneTemplate(detail.template.id, kind, `${detail.template.name} (copy)`);
			void recordTemplateUsage(detail.template.id).catch(() => undefined);
			toasts.success('Template cloned');
			await goto(resolve('/templates/[kind]/[id]', { kind, id: newId }));
		} catch (e) {
			toasts.error('Clone failed', e instanceof Error ? e.message : undefined);
		}
	}

	async runExport(): Promise<void> {
		const kind = this.kind;
		if (!kind || this.isNew) return;
		try {
			await exportTemplate(kind, this.routeIdParam);
			void recordTemplateUsage(this.routeIdParam).catch(() => undefined);
			toasts.success('Template exported');
		} catch (e) {
			toasts.error('Export failed', e instanceof Error ? e.message : undefined);
		}
	}

	async backToLibrary(): Promise<void> {
		await goto('/templates');
	}

	async runDelete(): Promise<void> {
		const kind = this.kind;
		if (!kind || this.isNew) return;
		try {
			if (kind === 'workflow') {
				await deleteWorkflow(this.routeIdParam);
			} else {
				await deleteTemplate(kind, this.routeIdParam);
			}
			toasts.success('Template deleted');
			await this.backToLibrary();
		} catch (e) {
			toasts.error('Delete failed', e instanceof Error ? e.message : undefined);
		} finally {
			this.deleteArmed = false;
		}
	}

	requestView(next: DetailView): void {
		if (next === 'preview') {
			if (this.editDirty()) {
				this.unsavedOpen = true;
				return;
			}
			this.exitEdit();
			return;
		}
		if (this.isNew) {
			this.editTab = 'json';
			return;
		}
		if (!this.detail) return;
		this.enterEdit(next === 'graph' ? 'graph' : 'json');
	}

	/**
	 * Node type this template inserts as: the stored `node_type` normalised
	 * the way the backend will store it, or null when it is blank. Only node
	 * templates carry `node_type`; trigger templates describe a trigger, never
	 * a canvas node.
	 */
	insertNodeType = $derived.by(() => {
		const kind = this.kind;
		const detail = this.detail;
		if (this.isNew || kind !== 'node' || !detail) return null;
		const raw =
			detail.raw && typeof detail.raw === 'object' ? (detail.raw as Record<string, unknown>) : null;
		const value = raw?.node_type;
		return typeof value === 'string' ? normalizeNodeType(value) : null;
	});

	/**
	 * Queue the current node template for insertion into a workflow definition,
	 * then deep-link to the target. Only node templates can land on a
	 * canvas, and only when the stored `node_type` is a non-blank name.
	 */
	insertIntoCanvas(targetWorkflowId?: string | null): void {
		const detail = this.detail;
		const nodeType = this.insertNodeType;
		if (!detail || nodeType === null) return;
		const target = targetWorkflowId?.trim() || null;
		nodeInsertStore.set({
			nodeType,
			name: detail.template.name,
			targetWorkflowId: target,
			sourceTemplateId: this.routeIdParam,
		});
		toasts.success(
			`${detail.template.name} queued`,
			target
				? 'Opening the target definition in graph mode.'
				: 'Open a workflow definition and enter graph mode to place it.',
		);
		if (target) {
			void goto(resolve('/templates/[kind]/[id]', { kind: 'workflow', id: target }) + '?tab=graph');
			return;
		}
		void goto('/templates?tab=workflow');
	}

	/**
	 * Apply a queued node insertion to the session document before the canvas
	 * loads, so the graph tab shows it without a second pick. A queued target
	 * pins the insert to one definition; a mismatch keeps the queue.
	 */
	consumePendingInsert(): void {
		if (this.kind !== 'workflow') return;
		const pendingInsert = nodeInsertStore.peek();
		if (!pendingInsert) return;
		if (pendingInsert.targetWorkflowId && pendingInsert.targetWorkflowId !== this.routeIdParam) {
			toasts.info(
				`Node queued for ${pendingInsert.targetWorkflowId}`,
				'Open the target definition to place it.',
			);
			return;
		}
		nodeInsertStore.consume();
		const topology = this.session.topology();
		const existing = new SvelteSet(topology.nodes.map((node) => node.id));
		const id = allocateGraphNodeId(existing);
		const nodes = [...topology.nodes, { id, label: pendingInsert.name || id, kind: 'STEP' }];
		this.session.mergeGraph(nodes, topology.edges);
		this.editText = this.session.text;
	}

	discardEditsAndLeave(): void {
		this.unsavedOpen = false;
		this.exitEdit();
		const target = this.pendingNavUrl;
		this.pendingNavUrl = null;
		if (target) {
			// The target replays an intercepted navigation URL, which resolve() cannot rebuild.

			void goto(target);
		}
	}
}
