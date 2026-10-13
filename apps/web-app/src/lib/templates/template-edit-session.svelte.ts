import { goto } from '$app/navigation';
import { resolve } from '$app/paths';
import {
	saveWorkflowDraft,
	deleteWorkflowDraft,
	validateWorkflowDraft,
} from '$lib/services/graph';
import { GraphEditStore } from '$lib/graph/edit-store.svelte';
import type { CanvasPosition } from '$lib/graph/canvas-model';
import { createWorkflow } from '$lib/services/workflows';
import {
	allocateGraphNodeId,
	cloneTemplate,
	deleteTemplate,
	exportTemplate,
	localTemplateIssues,
	recordTemplateUsage,
	saveTemplate,
	serverTemplateIssues,
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
import { slugify } from '$lib/utils/format';
import type { TemplateEditTab } from '$lib/components/domain/TemplateEditPanel.svelte';

export const VIEWS = ['preview', 'edit', 'graph'] as const;
export type DetailView = (typeof VIEWS)[number];

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
	loadDetail: (
		activeKind: TemplateKind,
		id: string,
		view: DetailView,
	) => Promise<void>;
}

/**
 * Edit-side controller for the template detail page. Holds every editing
 * concern (JSON/form/graph state, validation, save and lifecycle actions)
 * so the route stays a thin composition layer that only owns navigation
 * guards and detail loading.
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
	openWorkflowBusy = $state(false);

	// Controlled graph edit state for workflow templates. The canvas only
	// emits intents; every mutation lands here and re-renders from it.
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

		// Keep the session document fresh while typing; failed parses retain
		// the last valid document so the form and canvas never clear.
		$effect(() => {
			if (this.editMode && this.context.kind() !== null) {
				this.session.applyText(this.editText);
				this.syntaxError = this.session.syntaxError;
			}
		});

		// Explicit check results only describe the validated text; any later
		// edit returns the badge and server issues to idle.
		$effect(() => {
			if (
				this.validatedText !== null &&
				this.editText !== this.validatedText
			) {
				this.validatedText = null;
				this.serverIssues = [];
				this.validationState = 'idle';
			}
		});
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

	get formFields() {
		return templateFormFields(this.kind ?? 'node');
	}

	get viewItems() {
		return this.kind === 'workflow'
			? [
					{ id: 'preview', label: 'Preview' },
					{ id: 'edit', label: 'Edit' },
					{ id: 'graph', label: 'Graph' },
				]
			: [
					{ id: 'preview', label: 'Preview' },
					{ id: 'edit', label: 'Edit' },
				];
	}

	get kindOptions() {
		return [
			{ value: 'node', label: 'Node' },
			{ value: 'trigger', label: 'Trigger' },
			{ value: 'agent', label: 'Agent' },
			{ value: 'workflow', label: 'Workflow' },
		];
	}

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
		if (tab === 'graph') this.graphRequest += 1;
	}

	exitEdit(): void {
		this.editMode = false;
		this.validationState = 'idle';
		this.serverIssues = [];
		this.validatedText = null;
		this.syntaxError = null;
	}

	/** Fold the active tab back into the session before validate or save. */
	flushEditState(): void {
		const kind = this.kind;
		if (!kind) return;
		if (this.editTab === 'form') {
			this.session.applyForm(this.formState);
			this.editText = this.session.text;
		} else if (
			this.editTab === 'graph' &&
			kind === 'workflow' &&
			this.templateEditStore.dirty
		) {
			this.session.mergeGraph(
				this.templateEditStore.nodes,
				this.templateEditStore.edges,
			);
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

	handleTemplateMoveNodes(
		moves: Array<{ id: string; position: CanvasPosition }>,
	): void {
		this.templateEditStore.applyMoves(moves);
	}

	handleTemplateAddNode(position: CanvasPosition): void {
		const store = this.templateEditStore;
		const existing = new Set(store.nodes.map((node) => node.id));
		const id = allocateGraphNodeId(existing);
		store.addNode({ id, label: id, kind: 'STEP' }, position);
		store.selectedId = id;
	}

	handleTemplateConnect(source: string, target: string): void {
		const store = this.templateEditStore;
		if (
			store.edges.some(
				(edge) => edge.source === source && edge.target === target,
			)
		) {
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
		toasts.info(
			'Groups deleted',
			`${ids.length} group(s) removed from the canvas.`,
		);
	}

	editedPayload(parsed: unknown): unknown {
		const kind = this.kind;
		if (kind === 'node' || kind === 'trigger') return parsed;
		const detail = this.detail;
		const base =
			detail?.raw && typeof detail.raw === 'object'
				? (detail.raw as Record<string, unknown>)
				: {};
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
			toasts.error(
				'Template invalid',
				e instanceof Error ? e.message : undefined,
			);
			return false;
		}
		this.draftBusy = true;
		let draftId: string | null = null;
		try {
			draftId = await saveWorkflowDraft(definition);
			const issues = await validateWorkflowDraft(draftId);
			this.serverIssues = serverTemplateIssues(
				issues,
				this.session.topology().nodes,
			);
			this.validatedText = this.editText;
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
		const issues = await validateTemplateDefinition(kind, value);
		if (kind === 'workflow' || kind === 'agent') {
			this.serverIssues = issues;
		} else {
			this.serverIssues = [];
		}
		const combined = [
			...localTemplateIssues(kind, value),
			...this.serverIssues,
		];
		this.validationState = combined.length === 0 ? 'valid' : 'invalid';
		this.validatedText = this.editText;
		return combined.length === 0;
	}

	async runSave(): Promise<void> {
		const kind = this.kind;
		if (!kind) return;
		const valid = await this.runValidate();
		if (!valid) {
			toasts.error('Template invalid', 'Fix the reported issues first.');
			return;
		}
		if (kind === 'workflow') {
			const serverOk = await this.runTemplateServerGate();
			if (!serverOk) {
				this.editTab = 'graph';
				return;
			}
		}
		const value = this.session.definition();
		const isNew = this.isNew;
		this.saveBusy = true;
		try {
			if (isNew) {
				const payload = value as Record<string, unknown>;
				const id = this.createId.trim() || slugify(this.createName);
				if (!id) throw new Error('Template id is required');
				const savedId = await saveTemplate(this.createKind, null, {
					...payload,
					id,
					name: this.createName.trim() || id,
				});
				toasts.success('Template created');
				await goto(
					resolve('/templates/[kind]/[id]', {
						kind: this.createKind,
						id: savedId || id,
					}),
				);
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

	async runOpenAsWorkflow(): Promise<void> {
		const kind = this.kind;
		if (kind !== 'workflow' || this.isNew) return;
		this.flushEditState();
		this.session.applyText(this.editText);
		this.syntaxError = this.session.syntaxError;
		const value = this.session.definition();
		const error = this.session.syntaxError;
		if (error || value === undefined) {
			toasts.error('Template invalid', 'Fix the JSON syntax first.');
			return;
		}
		let definition: Record<string, unknown>;
		try {
			definition = this.buildBackendDefinition(value);
		} catch (e) {
			toasts.error(
				'Template invalid',
				e instanceof Error ? e.message : undefined,
			);
			return;
		}
		if (this.templateEditStore.dirty) {
			toasts.error('Unsaved graph changes', 'Save the template first.');
			return;
		}
		this.openWorkflowBusy = true;
		try {
			const name =
				typeof definition.name === 'string' && definition.name
					? definition.name
					: `template-${this.routeIdParam}`;
			const created = await createWorkflow(name, definition);
			void recordTemplateUsage(this.routeIdParam).catch(() => undefined);
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
			this.openWorkflowBusy = false;
		}
	}

	async runClone(): Promise<void> {
		const kind = this.kind;
		const detail = this.detail;
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
		// eslint-disable-next-line svelte/no-navigation-without-resolve
		await goto('/templates');
	}

	async runDelete(): Promise<void> {
		const kind = this.kind;
		if (!kind || this.isNew) return;
		try {
			await deleteTemplate(kind, this.routeIdParam);
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
	get insertNodeType(): string | null {
		const kind = this.kind;
		const detail = this.detail;
		if (this.isNew || kind !== 'node' || !detail) return null;
		const raw =
			detail.raw && typeof detail.raw === 'object'
				? (detail.raw as Record<string, unknown>)
				: null;
		const value = raw?.node_type;
		return typeof value === 'string' ? normalizeNodeType(value) : null;
	}

	/**
	 * Queue the current node template for insertion into a workflow canvas,
	 * then send the user to the workflow list to pick the target. Only node
	 * templates can land on a canvas, and only when the stored `node_type` is
	 * a non-blank name.
	 */
	insertIntoCanvas(): void {
		const detail = this.detail;
		const nodeType = this.insertNodeType;
		if (!detail || nodeType === null) return;
		nodeInsertStore.set({ nodeType, name: detail.template.name });
		toasts.success(
			`${detail.template.name} queued`,
			'Open a workflow and enter edit mode to place it.',
		);
		void goto(resolve('/workflows'));
	}

	discardEditsAndLeave(): void {
		this.unsavedOpen = false;
		this.exitEdit();
		const target = this.pendingNavUrl;
		this.pendingNavUrl = null;
		if (target) {
			// The target replays an intercepted navigation URL, which resolve() cannot rebuild.
			// eslint-disable-next-line svelte/no-navigation-without-resolve
			void goto(target);
		}
	}
}
