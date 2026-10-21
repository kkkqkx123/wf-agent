<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate, goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Segmented from '@wf-agent/ui/components/Segmented.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import WorkflowGraphTab from '#lib/components/domain/workflow-detail/WorkflowGraphTab.svelte';
	import WorkflowEditSection from '#lib/components/domain/workflow-detail/WorkflowEditSection.svelte';
	import WorkflowUnsavedDialog from '#lib/components/domain/workflow-detail/WorkflowUnsavedDialog.svelte';
	import DefinitionVersionsPanel from '#lib/components/domain/DefinitionVersionsPanel.svelte';
	import DefinitionDraftsPanel from '#lib/components/domain/DefinitionDraftsPanel.svelte';
	import RunsPanel from '#lib/components/domain/RunsPanel.svelte';
	import {
		executeWorkflow,
		exportWorkflow,
		getWorkflowDetail,
	} from '#lib/services/workflows';
	import {
		promoteDefinitionDraft,
		validateDefinitionDraft,
	} from '#lib/services/definitions';
	import type { WorkflowDetail } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { formatNumber } from '#lib/utils/format';
	import { gotoWithParams, parseListParams } from '#lib/utils/route';
	import { t, message } from '#lib/i18n';
	import type { DisplayEdge, DisplayNode } from '#lib/graph/display-model';
	import { WorkflowEditSession } from '#lib/workflows/edit-session.svelte';
	import { WorkflowGraphState } from '#lib/workflows/graph-state.svelte';

	const TABS = [
		{ id: 'graph', label: message('page.workflowDetail.tabGraph') },
		{ id: 'edit', label: message('page.workflowDetail.tabEdit') },
		{ id: 'versions', label: message('page.workflowDetail.tabVersions') },
		{ id: 'drafts', label: message('page.workflowDetail.tabDrafts') },
		{ id: 'runs', label: message('page.workflowDetail.tabRuns') },
	];

	const requestedTab = parseListParams(page.url as unknown as URL).tab;
	let tab = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab)
			? requestedTab
			: 'graph',
	);

	$effect(() => {
		gotoWithParams(page.url as unknown as URL, { tab: tab === 'graph' ? '' : tab });
	});

	let detail = $state<WorkflowDetail | null>(null);
	let detailError = $state<string | null>(null);
	let detailLoading = $state(true);

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

	async function load(id: string): Promise<void> {
		detailLoading = true;
		detailError = null;
		try {
			detail = await getWorkflowDetail(id);
			// Clear the graph selection and re-run the analysis with each
			// detail load, as the page-level state did before the split.
			void graphState.load(id);
		} catch (e) {
			detailError = e instanceof Error ? e.message : t('page.workflowDetail.loadFailed');
			detail = null;
		} finally {
			detailLoading = false;
		}
	}

	// Route-owned so tab switches keep the selection and analysis; the detail
	// load resets it.
	const graphState = new WorkflowGraphState();

	// Edit state lives on the controller; the route only reloads detail for it
	// and owns the active tab, which the controller drives through applyTab.
	const session = new WorkflowEditSession({
		reload: load,
		applyTab: (id) => (tab = id),
	});

	async function runPromote(draftId: string): Promise<void> {
		try {
			const report = await promoteDefinitionDraft('workflow', draftId);
			toasts.success(
				`Draft promoted (${report.passCount} passed, ${report.warningCount} warnings)`,
			);
			const id = page.params.id;
			if (id) await load(id);
		} catch (e) {
			toasts.error(
				t('page.workflowDetail.promotionFailed'),
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function runValidate(draftId: string): Promise<void> {
		try {
			const issues = await validateDefinitionDraft('workflow', draftId);
			if (detail) {
				detail = {
					...detail,
					drafts: detail.drafts.map((draft) =>
						draft.id === draftId
							? {
									...draft,
									valid: issues.length === 0,
									issues,
								}
							: draft,
					),
				};
			}
			toasts.success(
				issues.length === 0
					? t('page.workflowDetail.draftValid')
					: t('page.workflowDetail.draftIssues', { count: issues.length }),
			);
		} catch (e) {
			toasts.error(
				t('page.workflowDetail.validationFailed'),
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function runExport(id: string): Promise<void> {
		try {
			await exportWorkflow(id);
			toasts.success(t('page.workflowDetail.exported'));
		} catch (e) {
			toasts.error(t('page.workflowDetail.exportFailed'), e instanceof Error ? e.message : undefined);
		}
	}

	async function runExecute(id: string): Promise<void> {
		try {
			const executionId = await executeWorkflow(id);
			toasts.success(t('page.workflowDetail.executionStarted'));
			await goto(resolve('/executions/[id]', { id: executionId }));
		} catch (e) {
			toasts.error(
				t('page.workflowDetail.executionFailedToStart'),
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
			void session.lockStore.release();
		};
		const guardUnload = (event: BeforeUnloadEvent): void => {
			if (session.editDirty()) event.preventDefault();
		};
		window.addEventListener('pagehide', release);
		window.addEventListener('beforeunload', release);
		window.addEventListener('beforeunload', guardUnload);
		return () => {
			window.removeEventListener('pagehide', release);
			window.removeEventListener('beforeunload', release);
			window.removeEventListener('beforeunload', guardUnload);
			void session.lockStore.release();
		};
	});

	beforeNavigate((navigation) => {
		if (navigation.willUnload) return;
		if (!session.editDirty()) return;
		navigation.cancel();
		session.pendingNavUrl = navigation.to?.url.toString() ?? null;
		session.pendingTab = null;
		session.unsavedOpen = true;
	});

	$effect(() => {
		const id = page.params.id;
		if (!id) return;
		if (detail && detail.id === id) return;
		void load(id);
	});

	// Reactive hooks into the edit session: lock watch and canvas seeding,
	// lock-loss notice, lease release on tab leave and presence lifecycle.
	$effect(() => session.watchTab(tab, detail, nodes, edges));
	$effect(() => session.notifyLockLoss());
	$effect(() => session.syncLeaveEdit());
	$effect(() => session.presenceEffect());
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={workflow?.name ?? t('page.workflowDetail.title')}
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
			<Button variant="ghost" size="sm" href="/workflows">
				<Icon name="arrow-left" size={13} />
				{t('page.workflowDetail.backToWorkflows')}
			</Button>
			<IconButton
				icon="pencil"
				label={t('page.workflowDetail.editGraph')}
				onclick={() => session.requestTab('edit')}
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
				{t('domain.template.export')}
			</Button>
			<Button
				variant="brand"
				size="sm"
				onclick={() => {
					const id = page.params.id;
					if (id) void runExecute(id);
				}}
			>
				<Icon name="play" size={13} />
				{t('domain.git.run')}
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented
		items={TABS}
		value={tab}
		onchange={(id) => session.requestTab(id)}
		class="px-4"
	/>

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if detailLoading}
			<Skeleton lines={6} class="rounded-lg border border-border bg-card p-4" />
		{:else if detailError || !detail}
			<ErrorState
				title={t('page.workflowDetail.loadFailed')}
				description={detailError ?? t('page.workflowDetail.unknownError')}
				onretry={() => {
					const id = page.params.id;
					if (id) void load(id);
				}}
				class="rounded-lg border border-border bg-card"
			/>
		{:else if tab === 'graph'}
			<WorkflowGraphTab {detail} {nodes} {edges} state={graphState} />
		{:else if tab === 'edit'}
			<WorkflowEditSection controller={session} workflowId={detail.id} />
		{:else if tab === 'versions'}
			{#if detail}
				<DefinitionVersionsPanel
					kind="workflow"
					definitionId={detail.id}
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
			<DefinitionDraftsPanel
				kind="workflow"
				drafts={detail.drafts}
				onpromote={(draftId) => void runPromote(draftId)}
				onvalidate={(draftId) => void runValidate(draftId)}
			/>
		{:else}
			<RunsPanel kind="workflow" definitionId={detail.id} />
		{/if}
	</div>
</div>

<WorkflowUnsavedDialog controller={session} />
