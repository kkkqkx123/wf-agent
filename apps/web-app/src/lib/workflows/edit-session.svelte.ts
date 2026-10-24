import { SvelteSet } from 'svelte/reactivity';
import { goto } from '$app/navigation';
import { page } from '$app/state';
import type { CanvasPosition } from '#lib/graph/canvas-model';
import type { DisplayEdge, DisplayNode } from '#lib/graph/display-model';
import { GraphEditStore } from '#lib/graph/edit-store.svelte';
import {
	promoteDefinitionDraft,
	saveDefinitionDraft,
	validateDefinitionDraft,
} from '#lib/services/definitions';
import {
	resetWorkflowLayout,
	saveWorkflowLayout,
} from '#lib/services/layout';
import {
	allocateGraphNodeId,
	serverTemplateIssues,
	type TemplateIssue,
} from '#lib/services/templates';
import type { ValidationIssue, WorkflowDetail } from '#lib/types/models';
import { toasts } from '#lib/stores/toast.svelte';
import { nodeInsertStore } from '#lib/stores/node-insert.svelte';
import { EditLockStore } from '#lib/stores/edit-lock.svelte';
import { presence } from '#lib/stores/presence.svelte';

/**
 * Edit-session controller for the workflow detail page. Holds the controlled
 * canvas edit state (edit store, lock, draft and unsaved-navigation state) so
 * the route stays a thin composition layer. The route drives reactivity by
 * calling {@link watchTab} and the other effect hooks from `$effect`.
 */
export class WorkflowEditSession {
	editStore = new GraphEditStore();
	lockStore = new EditLockStore('workflow');

	/** Detail reload callback provided by the route, used after promotion. */
	private reload: (id: string) => Promise<void>;
	/** Tab setter provided by the route; the route owns the tab state. */
	private applyTab: (id: string) => void;

	tab = $state('graph');
	detail = $state<WorkflowDetail | null>(null);
	editSeededId = $state('');
	editMode = $state(false);
	editBusy = $state(false);
	layoutBusy = $state(false);
	editDraftId = $state<string | null>(null);
	editIssues = $state<ValidationIssue[]>([]);

	unsavedOpen = $state(false);
	unsavedBusy = $state(false);
	pendingTab = $state<string | null>(null);
	pendingNavUrl = $state<string | null>(null);

	constructor(options: {
		reload: (id: string) => Promise<void>;
		applyTab: (id: string) => void;
	}) {
		this.reload = options.reload;
		this.applyTab = options.applyTab;
	}

	get editTemplateIssues(): TemplateIssue[] {
		return serverTemplateIssues(this.editIssues, this.editStore.nodes);
	}

	/** Route `$effect`: mirror the active tab and seed the edit canvas. */
	watchTab(
		tab: string,
		detail: WorkflowDetail | null,
		nodes: DisplayNode[],
		edges: DisplayEdge[],
	): void {
		this.tab = tab;
		this.detail = detail;
		if (tab !== 'edit' || !detail) return;
		this.lockStore.watch(detail.id);
		if (this.editSeededId === detail.id) return;
		this.seed(detail, nodes, edges);
	}

	seed(
		detail: WorkflowDetail,
		nodes: DisplayNode[],
		edges: DisplayEdge[],
	): void {
		this.editStore.load(nodes, edges);
		this.editSeededId = detail.id;
		this.editDraftId = null;
		this.editIssues = [];
		this.editMode = false;
		// A node queued from the template library lands as the first edit so
		// the user sees it without re-picking the template manually.
		const pendingInsert = nodeInsertStore.consume();
		if (pendingInsert) {
			this.handleAddNode(
				{ x: 0, y: 0 },
				pendingInsert.nodeType,
				pendingInsert.name,
			);
		}
	}

	/** Route `$effect`: warn once when a held lease is lost mid-edit. */
	notifyLockLoss(): void {
		// A lost lease keeps the canvas read-only with a standing notice;
		// dirty content stays so it can be saved after re-acquiring.
		if (this.editMode && this.lockStore.lockLost) {
			if (this.editStore.dirty) {
				toasts.warning(
					'Edit lock lost with unsaved changes',
					this.lockStore.lockLostBy
						? `Held by ${this.lockStore.lockLostBy}. Canvas kept your edits read-only; re-acquire the lock to save.`
						: 'The lease expired. Canvas kept your edits read-only; re-acquire the lock to save.',
				);
			} else {
				toasts.warning(
					'Edit lock lost',
					this.lockStore.lockLostBy
						? `Held by ${this.lockStore.lockLostBy}. Canvas is read-only.`
						: 'The lease expired. Canvas is read-only.',
				);
			}
		}
	}

	/** Route `$effect`: leaving the edit tab releases the lease. */
	syncLeaveEdit(): void {
		// Leaving the edit tab releases the lease; re-entering re-acquires.
		// Release failures only surface as a notice and never block navigation.
		if (this.tab !== 'edit' && this.editMode) this.exitEdit();
	}

	/**
	 * Route `$effect`: presence follows the edit tab — cursors broadcast while
	 * editing and stop when leaving, so read-only views never emit. Returns
	 * the cleanup the effect runs on teardown.
	 */
	presenceEffect(): (() => void) | undefined {
		if (this.tab !== 'edit' || !this.detail) return;
		presence.join('workflow', this.detail.id);
		return () => presence.leave();
	}

	async enterEdit(): Promise<void> {
		const ok = await this.lockStore.acquire();
		if (!ok) {
			toasts.warning(
				'Workflow is being edited',
				`Held by ${this.lockStore.displayHolder}.`,
			);
			return;
		}
		this.editMode = true;
	}

	exitEdit(): void {
		this.editMode = false;
		this.lockStore.acknowledgeLockLoss();
		void this.lockStore.release();
	}

	/** Unsaved canvas edits block leaving the edit tab or the page. */
	editDirty(): boolean {
		if (this.tab !== 'edit' || !this.editMode) return false;
		return this.editStore.dirty || this.editStore.layoutDirty;
	}

	/** Layout drift against the last layout save; topology dirt stays separate. */
	get layoutDirty(): boolean {
		return this.editMode && this.editStore.layoutDirty;
	}

	requestTab(id: string): void {
		if (id === this.tab) return;
		if (this.editDirty()) {
			this.pendingTab = id;
			this.pendingNavUrl = null;
			this.unsavedOpen = true;
			return;
		}
		this.applyTab(id);
	}

	proceedPending(): void {
		if (this.pendingNavUrl) {
			const target = this.pendingNavUrl;
			this.pendingNavUrl = null;
			this.pendingTab = null;
			// The target replays an intercepted navigation URL, which resolve() cannot rebuild.

			void goto(target);
		} else if (this.pendingTab) {
			this.applyTab(this.pendingTab);
			this.pendingTab = null;
		}
	}

	discardWorkflowEdits(): void {
		this.unsavedOpen = false;
		this.editStore.markClean();
		this.editStore.markLayoutClean();
		this.exitEdit();
		this.proceedPending();
	}

	async saveAndProceed(): Promise<void> {
		this.unsavedBusy = true;
		try {
			await this.saveEditDraft();
			if (this.editStore.dirty) return;
			await this.saveLayout();
			if (this.editStore.layoutDirty) return;
			this.unsavedOpen = false;
			this.proceedPending();
		} finally {
			this.unsavedBusy = false;
		}
	}

	/**
	 * Persist canvas positions through the layout endpoint, separate from
	 * the draft payload. Topology saves mark the draft clean; layout saves
	 * mark the layout clean. Template graphs have no layout endpoint and
	 * intentionally leave `layoutDirty` unwired.
	 */
	async saveLayout(): Promise<void> {
		const id = this.detail?.id ?? page.params.id;
		if (!id || !this.editStore.layoutDirty) return;
		this.layoutBusy = true;
		try {
			await saveWorkflowLayout(id, { positions: this.editStore.positions });
			this.editStore.markLayoutClean();
			toasts.success('Layout saved');
		} catch (e) {
			toasts.error(
				'Layout save failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			this.layoutBusy = false;
		}
	}

	/** Drop persisted positions; the canvas falls back to its default layout. */
	async resetLayout(): Promise<void> {
		const id = this.detail?.id ?? page.params.id;
		if (!id) return;
		this.layoutBusy = true;
		try {
			await resetWorkflowLayout(id);
			this.editStore.positions = {};
			this.editStore.markLayoutClean();
			toasts.success('Layout reset');
		} catch (e) {
			toasts.error(
				'Layout reset failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			this.layoutBusy = false;
		}
	}

	handleMoveNode(id: string, position: CanvasPosition): void {
		this.editStore.applyMove(id, position);
	}

	handleMoveNodes(
		moves: Array<{ id: string; position: CanvasPosition }>,
	): void {
		this.editStore.applyMoves(moves);
	}

	handleDeleteGroups(ids: string[]): void {
		for (const id of ids) this.editStore.removeGroup(id);
		toasts.info(
			'Groups deleted',
			`${ids.length} group(s) removed from the canvas.`,
		);
	}

	handleAddNode(
		position: CanvasPosition,
		nodeType: string,
		name: string | null,
	): void {
		const existing = new SvelteSet(this.editStore.nodes.map((node) => node.id));
		const id = allocateGraphNodeId(existing);
		const label = name?.trim() || id;
		this.editStore.addNode({ id, label, kind: nodeType }, position);
		this.editStore.selectedId = id;
		toasts.success(
			`Node ${id} added`,
			`${nodeType}. Save the draft to keep it.`,
		);
	}

	handleDeleteEdge(id: string): void {
		this.editStore.removeEdge(id);
	}

	handleConnect(source: string, target: string): void {
		if (
			this.editStore.edges.some(
				(edge) => edge.source === source && edge.target === target,
			)
		) {
			return;
		}
		const reason = this.editStore.connect(source, target);
		if (reason) {
			toasts.info('Cannot connect', reason);
			return;
		}
		this.editStore.selectedId = target;
		toasts.success(
			`Edge ${source} → ${target} added`,
			'Save the draft to keep it.',
		);
	}

	handleDeleteNodes(ids: string[]): void {
		this.editStore.removeNodes(ids);
		toasts.info(
			'Nodes deleted',
			`${ids.length} node(s) removed from the canvas.`,
		);
	}

	async saveEditDraft(): Promise<void> {
		const id = page.params.id;
		if (!id || !this.detail) return;
		if (!this.lockStore.canWrite) {
			toasts.warning(
				'Save blocked',
				`Held by ${this.lockStore.displayHolder}.`,
			);
			return;
		}
		this.editBusy = true;
		try {
			const savedId = await saveDefinitionDraft(
				'workflow',
				this.editStore.toDraftDefinition(
					id,
					this.detail.name,
					this.detail.definition,
				),
			);
			this.editDraftId = savedId;
			this.editStore.markClean();
			await this.validateEditDraft(savedId);
			toasts.success('Draft saved', `Draft ${savedId} updated.`);
		} catch (e) {
			toasts.error('Save failed', e instanceof Error ? e.message : undefined);
		} finally {
			this.editBusy = false;
		}
	}

	async validateEditDraft(draftId?: string): Promise<void> {
		const target = draftId ?? this.editDraftId;
		if (!target) {
			toasts.info('Nothing to validate', 'Save the draft first.');
			return;
		}
		try {
			this.editIssues = await validateDefinitionDraft('workflow', target);
			if (this.editIssues.length === 0) {
				toasts.success('Draft is valid');
			} else {
				toasts.warning(`Draft has ${this.editIssues.length} issue(s)`);
			}
		} catch (e) {
			toasts.error(
				'Validation failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async promoteEditDraft(): Promise<void> {
		if (!this.lockStore.canWrite) {
			toasts.warning(
				'Promotion blocked',
				`Held by ${this.lockStore.displayHolder}.`,
			);
			return;
		}
		if (!this.editDraftId) {
			toasts.info('Nothing to promote', 'Save the draft first.');
			return;
		}
		if (this.editStore.dirty) {
			toasts.error('Unsaved changes', 'Save the draft before promoting.');
			return;
		}
		try {
			const report = await promoteDefinitionDraft('workflow', this.editDraftId);
			toasts.success(
				`Draft promoted (${report.passCount} passed, ${report.warningCount} warnings)`,
			);
			const id = page.params.id;
			this.editSeededId = '';
			if (id) await this.reload(id);
		} catch (e) {
			toasts.error(
				'Promotion failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}
}
