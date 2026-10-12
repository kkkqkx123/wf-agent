<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Segmented from '@wf-agent/ui/components/Segmented.svelte';
	import Dialog from '@wf-agent/ui/components/Dialog.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import Textarea from '@wf-agent/ui/components/Textarea.svelte';
	import { toneText } from '@wf-agent/ui/components/variants';
	import type { StatusTone } from '@wf-agent/ui/status';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import PageState from '$lib/components/layout/PageState.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import DiffView from '@wf-agent/ui/components/DiffView.svelte';
	import JsonViewer from '@wf-agent/ui/components/JsonViewer.svelte';
	import CursorPager from '@wf-agent/ui/components/CursorPager.svelte';
	import StreamMarkdown from '$lib/components/chat/StreamMarkdown.svelte';
	import { downloadFile } from '$lib/api/client';
	import {
		listCheckpoints,
		getFileChangesPage,
		getApprovalRequests,
		getMainDiffs,
		getDiffActors,
		getFileContent,
		getFileTree,
		getFileTimeline,
		approveApproval,
		rejectApproval,
		deleteCheckpoint,
		listPartitions,
		listConflicts,
		resolveConflicts,
		submitForReview,
		mergeToFeature,
		mergeToMain,
		rollbackFeature,
		listEditSessions,
		beginEditSession,
		rollbackEditSession,
		undoEdit,
		redoEdit,
		getActorWorkspace,
		getIndexStatus,
		rebuildIndex,
		createFileCheckpoint,
		restoreFromCheckpoint,
		resumeFromCheckpoint,
		type ConflictFile,
		type EditSession,
		type FileContent,
		type FileDiff,
		type FileTimeline,
		type FileTree,
		type IndexStatus,
		type PartitionView,
		type WorkspaceFile,
	} from '$lib/services/checkpoints';
	import type { Approval, Checkpoint, FileChange } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { checkpoints as checkpointStore } from '$lib/stores/checkpoints.svelte';
	import {
		formatBytes,
		formatDateTime,
		formatRelativeTime,
	} from '$lib/utils/format';
	import { cn } from '@wf-agent/ui/cn';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';

	const TABS = [
		{ id: 'chain', label: 'Checkpoint chain' },
		{ id: 'files', label: 'File workspace' },
		{ id: 'approvals', label: 'Approvals' },
	];

	const requestedTab = parseListParams(page.url).tab;
	let tab = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab)
			? requestedTab
			: 'chain',
	);

	$effect(() => {
		gotoWithParams(page.url, { tab: tab === 'chain' ? '' : tab });
	});

	// ── chain tab ──
	let chain = $state<Checkpoint[]>([]);
	// ── files tab ──
	let fileChanges = $state<FileChange[]>([]);
	let partitions = $state<PartitionView[]>([]);
	let conflicts = $state<ConflictFile[]>([]);
	let sessions = $state<EditSession[]>([]);
	let workspaceFiles = $state<WorkspaceFile[]>([]);
	let indexStatus = $state<IndexStatus | null>(null);
	// ── approvals tab ──
	let approvals = $state<Approval[]>([]);

	// ── selection (mirrored into the store so tab switches keep it) ──
	let selectedChangeId = $state<string | null>(null);
	let mainDiff = $state<FileDiff | null>(null);
	let diffNote = $state<string | null>(null);
	let diffLoading = $state(false);
	let previewContent = $state<FileContent | null>(null);
	let previewTimeline = $state<FileTimeline | null>(null);
	let previewTree = $state<FileTree | null>(null);
	let timelineShown = $state(0);
	let treePrefix = $state('');
	let workspaceActor = $state('');
	let undoActor = $state('');
	let sessionActor = $state('');
	let reviewActor = $state('');
	let reviewFeature = $state('');
	let reviewFeaturesText = $state('');
	let rollbackHead = $state('');

	let changesHasMore = $state(false);
	let changesLoading = $state(false);
	let changesOffset = $state(0);
	let loading = $state(false);
	let loadError = $state<string | null>(null);
	let liveDirty = $state(false);
	let stopLive: (() => void) | null = null;

	// ── dialogs ──
	let createOpen = $state(false);
	let createActor = $state('manual');
	let restoreOpen = $state(false);
	let restoreTarget = $state<Checkpoint | null>(null);
	let deleteOpen = $state(false);
	let deleteTarget = $state<Checkpoint | null>(null);
	let rejectOpen = $state(false);
	let rejectTarget = $state<Approval | null>(null);
	let rejectReason = $state('');
	let resolveOpen = $state(false);
	let resolveTarget = $state<ConflictFile | null>(null);
	let resolveActor = $state('');
	let resolveFeature = $state('');
	let resolveContent = $state('');

	const CHANGES_PAGE = 100;
	const TIMELINE_PAGE = 50;

	/** Segment sources already pulled, so a tab loads once. */
	let seenChain = $state(false);
	let seenFiles = $state(false);
	let seenApprovals = $state(false);

	onMount(() => {
		stopLive = checkpointStore.subscribeLive(() => {
			if (loading) {
				liveDirty = true;
				return;
			}
			void reloadCurrent();
		});
	});

	onDestroy(() => {
		stopLive?.();
		stopLive = null;
	});

	$effect(() => {
		void loadTab(tab);
	});

	async function loadTab(current: string): Promise<void> {
		const pending =
			current === 'chain'
				? !seenChain
				: current === 'files'
					? !seenFiles
					: !seenApprovals;
		if (!pending) return;
		loading = true;
		loadError = null;
		try {
			if (current === 'chain') {
				seenChain = true;
				chain = (await listCheckpoints({ limit: 200 })).items;
			} else if (current === 'files') {
				seenFiles = true;
				await loadFilesSegment();
			} else {
				seenApprovals = true;
				approvals = await getApprovalRequests();
			}
		} catch (e) {
			console.error('Failed to load checkpoints segment:', e);
			loadError = e instanceof Error ? e.message : 'Checkpoint request failed';
		} finally {
			loading = false;
			if (liveDirty) {
				liveDirty = false;
				void reloadCurrent();
			}
		}
	}

	async function loadFilesSegment(): Promise<void> {
		const [changes, parts, found, groups, index] = await Promise.all([
			getFileChangesPage({ limit: CHANGES_PAGE }),
			listPartitions().catch(() => []),
			listConflicts().catch(() => []),
			listEditSessions().catch(() => []),
			getIndexStatus().catch(() => null),
		]);
		fileChanges = changes.items;
		changesOffset = changes.items.length;
		changesHasMore = changes.hasMore;
		partitions = parts;
		conflicts = found;
		sessions = groups;
		indexStatus = index;
	}

	/** Refresh the visible tab after a live checkpoint event. */
	async function reloadCurrent(): Promise<void> {
		if (tab === 'chain') {
			seenChain = false;
			chain = [];
		} else if (tab === 'files') {
			seenFiles = false;
			fileChanges = [];
			changesOffset = 0;
			changesHasMore = false;
		} else {
			seenApprovals = false;
			approvals = [];
		}
		await loadTab(tab);
	}

	async function loadMoreChanges(): Promise<void> {
		if (changesLoading || !changesHasMore) return;
		changesLoading = true;
		try {
			const page = await getFileChangesPage({
				limit: CHANGES_PAGE,
				offset: changesOffset,
			});
			fileChanges = [...fileChanges, ...page.items];
			changesOffset += page.items.length;
			changesHasMore = page.hasMore;
		} catch (e) {
			console.error('Failed to load more changes:', e);
			toasts.error('More changes unavailable');
		} finally {
			changesLoading = false;
		}
	}

	async function reload(): Promise<void> {
		seenChain = false;
		seenFiles = false;
		seenApprovals = false;
		chain = [];
		fileChanges = [];
		approvals = [];
		partitions = [];
		conflicts = [];
		sessions = [];
		workspaceFiles = [];
		indexStatus = null;
		changesOffset = 0;
		changesHasMore = false;
		await loadTab(tab);
	}

	const CHANGE_TONE: Record<string, StatusTone> = {
		added: 'success',
		modified: 'info',
		renamed: 'warning',
		deleted: 'danger',
	};

	async function selectChange(change: FileChange): Promise<void> {
		selectedChangeId = change.id;
		checkpointStore.select('files', change.id);
		mainDiff = null;
		previewContent = null;
		previewTimeline = null;
		previewTree = null;
		timelineShown = 0;
		diffNote = null;
		if (!change.actor) {
			diffNote = 'No actor recorded for this change';
			return;
		}
		if (!undoActor) undoActor = change.actor;
		if (!workspaceActor) workspaceActor = change.actor;
		if (!sessionActor) sessionActor = change.actor;
		diffLoading = true;
		try {
			const [diffs, content, timeline, tree] = await Promise.all([
				(async () => {
					if (change.session && change.session !== change.actor) {
						try {
							const paired = await getDiffActors(change.actor, change.session);
							const hit = paired.find((diff) => diff.path === change.path);
							if (hit) return [hit];
						} catch {
							// Fall through to the single-actor diff against main.
						}
					}
					const diffs = await getMainDiffs(change.actor);
					return diffs.filter((diff) => diff.path === change.path);
				})(),
				getFileContent(change.actor, change.path).catch(() => null),
				getFileTimeline(change.path).catch(() => null),
				getFileTree(change.actor, treePrefix || undefined).catch(() => null),
			]);
			const match = diffs[0] ?? null;
			if (!match) {
				diffNote = 'No diff against main for this path';
			} else {
				mainDiff = match;
			}
			previewContent = content;
			previewTimeline = timeline;
			previewTree = tree;
			timelineShown = Math.min(TIMELINE_PAGE, timeline?.entries.length ?? 0);
		} catch (e) {
			console.error('Failed to load diff against main:', e);
			diffNote = 'Diff against main unavailable';
			toasts.error('Diff against main unavailable');
		} finally {
			diffLoading = false;
		}
	}

	async function reloadTree(): Promise<void> {
		if (!workspaceActor.trim()) {
			toasts.error('Enter an actor to browse the workspace tree');
			return;
		}
		try {
			previewTree = await getFileTree(
				workspaceActor.trim(),
				treePrefix || undefined,
			);
		} catch (e) {
			console.error('Failed to load workspace tree:', e);
			toasts.error(
				'Workspace tree unavailable',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function loadWorkspace(): Promise<void> {
		if (!workspaceActor.trim()) {
			toasts.error('Enter an actor to inspect the workspace');
			return;
		}
		try {
			workspaceFiles = await getActorWorkspace(workspaceActor.trim());
		} catch (e) {
			console.error('Failed to load actor workspace:', e);
			toasts.error(
				'Workspace unavailable',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function runUndo(): Promise<void> {
		const actor = undoActor.trim();
		if (!actor) {
			toasts.error('Enter an actor for undo');
			return;
		}
		checkpointStore.beginAction('undo');
		try {
			const snapshot = await undoEdit(actor);
			toasts.success('Undo recorded', `Snapshot ${snapshot.slice(0, 10)}`);
			void reloadCurrent();
		} catch (e) {
			console.error('Undo failed:', e);
			checkpointStore.endAction('Undo failed');
			toasts.error('Undo failed', e instanceof Error ? e.message : undefined);
			return;
		}
		checkpointStore.endAction();
	}

	async function runRedo(): Promise<void> {
		const actor = undoActor.trim();
		if (!actor) {
			toasts.error('Enter an actor for redo');
			return;
		}
		checkpointStore.beginAction('redo');
		try {
			const snapshot = await redoEdit(actor);
			toasts.success('Redo recorded', `Snapshot ${snapshot.slice(0, 10)}`);
			void reloadCurrent();
		} catch (e) {
			console.error('Redo failed:', e);
			checkpointStore.endAction('Redo failed');
			toasts.error('Redo failed', e instanceof Error ? e.message : undefined);
			return;
		}
		checkpointStore.endAction();
	}

	async function runRebuildIndex(): Promise<void> {
		checkpointStore.beginAction('rebuild');
		try {
			const { indexedCommits } = await rebuildIndex();
			indexStatus = await getIndexStatus().catch(() => null);
			toasts.success(
				'Source index rebuilt',
				`${indexedCommits} commits indexed`,
			);
		} catch (e) {
			console.error('Index rebuild failed:', e);
			checkpointStore.endAction('Rebuild failed');
			toasts.error(
				'Index rebuild failed',
				e instanceof Error ? e.message : undefined,
			);
			return;
		}
		checkpointStore.endAction();
	}

	async function runCreateFileCheckpoint(): Promise<void> {
		const actor = createActor.trim();
		if (!actor) {
			toasts.error('Enter an actor for the checkpoint');
			return;
		}
		checkpointStore.beginAction('create');
		try {
			const created = await createFileCheckpoint(actor);
			createOpen = false;
			toasts.success(
				'File checkpoint created',
				`${created.fileCount} files · ${created.id.slice(0, 10)}`,
			);
			void reloadCurrent();
		} catch (e) {
			console.error('File checkpoint creation failed:', e);
			toasts.error(
				'Checkpoint creation failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runRestore(): Promise<void> {
		const target = restoreTarget;
		if (!target) return;
		checkpointStore.beginAction('restore');
		try {
			// Agent-loop checkpoints restore through the loop endpoint, all
			// other execution checkpoints through the executions endpoint.
			const loopId =
				target.entityType === 'agent_loop' ? target.executionId : undefined;
			await restoreFromCheckpoint(target.id, loopId);
			restoreOpen = false;
			restoreTarget = null;
			toasts.success(
				'Checkpoint restored',
				'Restore landed as a new commit; history was not rewritten.',
			);
			void reloadCurrent();
		} catch (e) {
			console.error('Restore failed:', e);
			toasts.error(
				'Restore failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runResume(checkpoint: Checkpoint): Promise<void> {
		checkpointStore.beginAction('resume');
		try {
			await resumeFromCheckpoint(checkpoint.id);
			toasts.success(
				'Resume requested',
				`Execution continues from ${checkpoint.id.slice(0, 10)}`,
			);
		} catch (e) {
			console.error('Resume failed:', e);
			toasts.error('Resume failed', e instanceof Error ? e.message : undefined);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runDelete(): Promise<void> {
		const target = deleteTarget;
		if (!target) return;
		checkpointStore.beginAction('delete');
		try {
			await deleteCheckpoint(target.id);
			deleteOpen = false;
			deleteTarget = null;
			toasts.success('Checkpoint deleted');
			void reloadCurrent();
		} catch (e) {
			console.error('Delete failed:', e);
			toasts.error('Delete failed', e instanceof Error ? e.message : undefined);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runBeginSession(): Promise<void> {
		checkpointStore.beginAction('begin-session');
		try {
			const id = await beginEditSession();
			toasts.success('Edit session started', id);
			sessions = await listEditSessions().catch(() => sessions);
		} catch (e) {
			console.error('Begin session failed:', e);
			toasts.error(
				'Begin session failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runRollbackSession(session: EditSession): Promise<void> {
		const actor = sessionActor.trim();
		if (!actor) {
			toasts.error('Enter an actor to roll back the session');
			return;
		}
		checkpointStore.beginAction(`rollback-${session.id}`);
		try {
			const snapshot = await rollbackEditSession(session.id, actor);
			toasts.success(
				'Session rolled back',
				`Snapshot ${snapshot.slice(0, 10)}`,
			);
			sessions = await listEditSessions().catch(() => sessions);
			void reloadCurrent();
		} catch (e) {
			console.error('Session rollback failed:', e);
			toasts.error(
				'Session rollback failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runResolveConflict(): Promise<void> {
		const target = resolveTarget;
		if (!target) return;
		const actor = resolveActor.trim();
		if (!actor) {
			toasts.error('Enter the resolving actor');
			return;
		}
		if (!resolveContent) {
			toasts.error('Paste the resolved file content');
			return;
		}
		checkpointStore.beginAction('resolve');
		try {
			const { remaining } = await resolveConflicts(actor, resolveFeature, {
				[target.path]: resolveContent,
			});
			resolveOpen = false;
			resolveTarget = null;
			resolveContent = '';
			toasts.success(
				remaining === 0 ? 'Conflict fully resolved' : 'Resolution recorded',
				remaining === 0 ? target.path : `${remaining} file(s) still conflicted`,
			);
			conflicts = await listConflicts().catch(() => conflicts);
		} catch (e) {
			console.error('Conflict resolution failed:', e);
			toasts.error(
				'Resolution failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	function openResolve(conflict: ConflictFile): void {
		resolveTarget = conflict;
		resolveOpen = true;
		if (!resolveActor && undoActor) resolveActor = undoActor;
		// Feature refs look like refs/wf/feat/<name>; main conflicts resolve
		// against an explicit feature name.
		resolveFeature = conflict.partition.startsWith('refs/wf/feat/')
			? conflict.partition.slice('refs/wf/feat/'.length)
			: '';
		resolveContent = '';
	}

	async function runSubmitReview(): Promise<void> {
		const actor = (reviewActor || undoActor).trim();
		if (!actor) {
			toasts.error('Enter an actor to submit for review');
			return;
		}
		checkpointStore.beginAction('submit');
		try {
			const { reviewCommit } = await submitForReview(actor);
			toasts.success('Submitted for review', reviewCommit.slice(0, 10));
			partitions = await listPartitions().catch(() => partitions);
		} catch (e) {
			console.error('Submit for review failed:', e);
			toasts.error(
				'Submit failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runMergeToFeature(): Promise<void> {
		const actor = (reviewActor || undoActor).trim();
		const feature = reviewFeature.trim();
		if (!actor) {
			toasts.error('Enter an actor to merge');
			return;
		}
		if (!feature) {
			toasts.error('Enter a feature name for refs/wf/feat/<name>');
			return;
		}
		checkpointStore.beginAction('merge-feature');
		try {
			const outcome = await mergeToFeature(actor, feature);
			if (outcome.conflictFiles.length > 0) {
				toasts.warning(
					'Merge has conflicts',
					outcome.conflictFiles.slice(0, 3).join(', '),
				);
			} else {
				toasts.success('Merged to feature', outcome.commitId.slice(0, 10));
			}
			conflicts = await listConflicts().catch(() => conflicts);
			partitions = await listPartitions().catch(() => partitions);
		} catch (e) {
			console.error('Merge to feature failed:', e);
			toasts.error(
				'Merge failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runMergeToMain(): Promise<void> {
		const actor = (reviewActor || undoActor).trim();
		const features = reviewFeaturesText
			.split(/[,\s]+/)
			.map((part) => part.trim())
			.filter((part) => part.length > 0);
		if (!actor) {
			toasts.error('Enter an actor to merge');
			return;
		}
		if (features.length === 0) {
			toasts.error('Enter at least one feature name');
			return;
		}
		checkpointStore.beginAction('merge-main');
		try {
			const outcome = await mergeToMain(actor, features);
			if (outcome.conflictFiles.length > 0) {
				toasts.warning(
					'Merge to main has conflicts',
					outcome.conflictFiles.slice(0, 3).join(', '),
				);
			} else {
				toasts.success('Merged to main', outcome.checkpointId.slice(0, 10));
			}
			conflicts = await listConflicts().catch(() => conflicts);
			partitions = await listPartitions().catch(() => partitions);
		} catch (e) {
			console.error('Merge to main failed:', e);
			toasts.error(
				'Merge to main failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	async function runRollbackMergedFeature(): Promise<void> {
		const actor = (reviewActor || undoActor).trim();
		const head = rollbackHead.trim();
		if (!actor) {
			toasts.error('Enter an actor for rollback');
			return;
		}
		if (!head) {
			toasts.error('Enter the merged feature head commit to revert');
			return;
		}
		checkpointStore.beginAction('rollback-feature');
		try {
			const { revertCommit } = await rollbackFeature(actor, head);
			toasts.success('Feature rolled back', revertCommit.slice(0, 10));
			partitions = await listPartitions().catch(() => partitions);
		} catch (e) {
			console.error('Feature rollback failed:', e);
			toasts.error(
				'Rollback failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			checkpointStore.endAction();
		}
	}

	function emptyCopyFor(current: string): {
		title: string;
		description: string;
	} {
		if (current === 'files') {
			return {
				title: 'No file changes',
				description:
					'File deltas appear here once a session writes to the checkpoint workspace.',
			};
		}
		if (current === 'approvals') {
			return {
				title: 'No approval requests',
				description:
					'Pending approvals appear here when a run asks for confirmation.',
			};
		}
		return {
			title: 'No checkpoints',
			description:
				'Checkpoints appear here once a run records a restorable state.',
		};
	}

	const segmentEmpty = $derived(
		tab === 'chain'
			? chain.length === 0
			: tab === 'files'
				? false
				: approvals.length === 0,
	);

	const segmentCopy = $derived(emptyCopyFor(tab));

	const selectedChange = $derived(
		fileChanges.find((change) => change.id === selectedChangeId) ?? null,
	);

	const timelineEntries = $derived(previewTimeline?.entries ?? []);
	const timelineVisible = $derived(timelineEntries.slice(0, timelineShown));

	function isMarkdownPath(path: string): boolean {
		return /\.(md|markdown)$/i.test(path);
	}

	function isJsonText(text: string): boolean {
		const trimmed = text.trim();
		if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) return false;
		try {
			JSON.parse(trimmed);
			return true;
		} catch {
			return false;
		}
	}

	function downloadPreview(): void {
		const content = previewContent;
		if (!content) return;
		const name = content.path.split('/').pop() || 'preview';
		const query = `actor=${encodeURIComponent(content.actor)}&path=${encodeURIComponent(content.path)}`;
		void downloadFile(`/api/v1/file-checkpoint/content?${query}`, name).catch(
			() => toasts.error('Download unavailable'),
		);
	}

	async function decideApproval(
		approval: Approval,
		granted: boolean,
	): Promise<void> {
		try {
			if (granted) {
				await approveApproval(approval.id);
				toasts.success('Approval granted');
			} else {
				rejectTarget = approval;
				rejectOpen = true;
				rejectReason = '';
				return;
			}
			seenApprovals = false;
			approvals = [];
			await loadTab(tab);
		} catch (e) {
			console.error('Failed to resolve approval:', e);
			toasts.error('Approval decision failed');
		}
	}

	async function runReject(): Promise<void> {
		const target = rejectTarget;
		if (!target) return;
		try {
			await rejectApproval(target.id, rejectReason || undefined);
			rejectOpen = false;
			rejectTarget = null;
			rejectReason = '';
			toasts.warning('Approval rejected');
			seenApprovals = false;
			approvals = [];
			await loadTab(tab);
		} catch (e) {
			console.error('Failed to reject approval:', e);
			toasts.error('Approval rejection failed');
		}
	}

	const liveLabel = $derived(
		checkpointStore.liveState === 'open'
			? 'Live'
			: checkpointStore.liveState === 'reconnecting'
				? 'Reconnecting'
				: 'Offline',
	);
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Checkpoints & files"
		description="Execution checkpoint chains, file workspace changes and pending approvals."
	>
		{#snippet actions()}
			<Badge
				variant={checkpointStore.liveState === 'open' ? 'success' : 'neutral'}
			>
				{liveLabel}
			</Badge>
			<IconButton
				icon="refresh"
				label="Refresh"
				onclick={() => void reload()}
			/>
			<Button
				size="sm"
				disabled={checkpointStore.busy}
				onclick={() => {
					createActor = 'manual';
					createOpen = true;
				}}
			>
				<Icon name="plus" size={13} />
				Create checkpoint
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented
		items={TABS}
		bind:value={tab}
		class="px-4"
		panelId="checkpoints-panel"
	/>

	<div
		id="checkpoints-panel"
		role="tabpanel"
		aria-label="Checkpoint sections"
		class="min-h-0 flex-1 overflow-y-auto px-4 py-3"
	>
		<PageState
			{loading}
			error={loadError}
			empty={segmentEmpty}
			emptyTitle={segmentCopy.title}
			emptyDescription={segmentCopy.description}
			onretry={() => void reload()}
		>
			{#if tab === 'chain'}
				<div class="space-y-2">
					{#each chain as checkpoint (checkpoint.id)}
						<Card title="{checkpoint.kind} checkpoint · #{checkpoint.sequence}">
							{#snippet actions()}
								<Badge variant={checkpoint.restorable ? 'success' : 'neutral'}>
									{checkpoint.restorable ? 'restorable' : 'locked'}
								</Badge>
							{/snippet}
							<div
								class="flex flex-wrap items-center gap-x-3 gap-y-1 text-caption text-muted-foreground"
							>
								<span class="font-mono">{checkpoint.id}</span>
								<span>{checkpoint.actor}</span>
								<span class="tabular-nums"
									>{formatBytes(checkpoint.sizeBytes)}</span
								>
								<span>{formatRelativeTime(checkpoint.createdAt)}</span>
							</div>
							<p class="mt-1.5 text-body">{checkpoint.note}</p>
							{#snippet footer()}
								<div class="flex flex-wrap items-center gap-2">
									<Button
										size="sm"
										disabled={!checkpoint.restorable || checkpointStore.busy}
										title={checkpoint.restorable
											? 'Restore this checkpoint'
											: 'This checkpoint is locked and cannot be restored'}
										onclick={() => {
											restoreTarget = checkpoint;
											restoreOpen = true;
										}}
									>
										Restore
									</Button>
									<Button
										variant="ghost"
										size="sm"
										disabled={checkpointStore.busy}
										onclick={() => void runResume(checkpoint)}
									>
										Resume from here
									</Button>
									<Button
										variant="ghost"
										size="sm"
										disabled={checkpointStore.busy}
										onclick={() => {
											deleteTarget = checkpoint;
											deleteOpen = true;
										}}
									>
										Delete
									</Button>
								</div>
							{/snippet}
						</Card>
					{/each}
				</div>
			{:else if tab === 'files'}
				<div class="grid gap-3 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
					<Card
						title="Changes"
						description="Recent file deltas grouped by session"
					>
						<ul class="divide-y divide-border">
							{#each fileChanges as change (change.id)}
								<li class="py-2 first:pt-0">
									<button
										type="button"
										onclick={() => void selectChange(change)}
										aria-pressed={selectedChangeId === change.id}
										class={cn(
											'flex w-full items-start justify-between gap-3 rounded-md px-2 py-1 text-left transition-colors hover:bg-accent/50',
											selectedChangeId === change.id && 'bg-accent/70',
										)}
									>
										<div class="min-w-0">
											<p class="truncate font-mono text-caption">
												{change.path}
											</p>
											<p class="mt-0.5 text-micro text-muted-foreground">
												{change.actor} · {change.session} · {formatRelativeTime(
													change.at,
												)}
											</p>
										</div>
										<div class="flex shrink-0 items-center gap-2">
											<span
												class={cn(
													'text-micro',
													toneText(CHANGE_TONE[change.changeType]),
												)}
											>
												{change.changeType}
											</span>
											<span class="text-micro tabular-nums text-success"
												>+{change.additions}</span
											>
											<span class="text-micro tabular-nums text-destructive"
												>-{change.deletions}</span
											>
										</div>
									</button>
								</li>
							{/each}
						</ul>
						{#snippet footer()}
							<CursorPager
								shown={fileChanges.length}
								hasMore={changesHasMore}
								loading={changesLoading}
								pageSize={CHANGES_PAGE}
								onloadmore={() => void loadMoreChanges()}
							/>
						{/snippet}
					</Card>

					<div class="space-y-3">
						{#if diffLoading}
							<Card title="Selected delta">
								<p class="text-caption text-muted-foreground">
									Loading diff against main…
								</p>
							</Card>
						{:else if mainDiff && !mainDiff.binary}
							<DiffView
								lines={mainDiff.lines}
								title={mainDiff.truncated
									? `${mainDiff.path} · truncated`
									: mainDiff.path}
							/>
						{:else if mainDiff?.binary}
							<Card title={mainDiff.path}>
								<p class="text-caption text-muted-foreground">
									Binary content has no text diff
								</p>
							</Card>
						{:else}
							<Card title="Selected delta">
								<p class="text-caption text-muted-foreground">
									{diffNote ??
										'Select a change to preview its diff against main'}
								</p>
							</Card>
						{/if}
						{#if previewContent && !previewContent.isBinary && previewContent.content !== null}
							<Card title={previewContent.path}>
								{#snippet actions()}
									<Button variant="ghost" size="sm" onclick={downloadPreview}>
										<Icon name="download" size={13} />
										Download
									</Button>
								{/snippet}
								{#if isMarkdownPath(previewContent.path)}
									<StreamMarkdown content={previewContent.content} done />
								{:else if isJsonText(previewContent.content)}
									<JsonViewer value={previewContent.content} />
								{:else}
									<pre
										class="max-h-64 overflow-auto rounded-md bg-muted px-2 py-1.5 font-mono text-micro break-words whitespace-pre-wrap text-foreground">{previewContent.content}</pre>
								{/if}
								<p class="mt-1.5 text-micro tabular-nums text-muted-foreground">
									{formatBytes(previewContent.size)}{previewContent.truncated
										? ' · truncated'
										: ''}
								</p>
							</Card>
						{:else if previewContent?.isBinary}
							<Card title={previewContent.path}>
								<p class="text-caption text-muted-foreground">
									Binary content has no text preview
								</p>
							</Card>
						{/if}
						{#if previewTimeline && timelineEntries.length > 0}
							<Card
								title="Version history"
								description="{previewTimeline.total} versions{previewTimeline.truncated
									? ' · truncated at 5000'
									: ''}{selectedChange?.path
									? ` · ${selectedChange.path}`
									: ''}"
							>
								<ul class="space-y-1.5">
									{#each timelineVisible as entry (entry.snapshotId)}
										<li
											class="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-caption"
										>
											<span class="font-mono text-micro text-muted-foreground"
												>{entry.snapshotId.slice(0, 10)}</span
											>
											<span class="min-w-0 flex-1 truncate">{entry.source}</span
											>
											{#if entry.movedFrom}
												<span class="text-micro text-muted-foreground"
													>renamed from {entry.movedFrom}</span
												>
											{/if}
											<span
												class="shrink-0 text-micro tabular-nums text-muted-foreground"
												>{formatRelativeTime(
													new Date(entry.timestamp).toISOString(),
												)}</span
											>
										</li>
									{/each}
								</ul>
								{#snippet footer()}
									{#if timelineShown < timelineEntries.length}
										<Button
											variant="ghost"
											size="sm"
											onclick={() => {
												timelineShown = Math.min(
													timelineShown + TIMELINE_PAGE,
													timelineEntries.length,
												);
											}}
										>
											Show more ({timelineEntries.length - timelineShown} remaining)
										</Button>
									{/if}
								{/snippet}
							</Card>
						{/if}
						<Card
							title="Workspace tree"
							description={previewTree
								? `${previewTree.total} files${previewTree.truncated ? ' · truncated at 2000' : ''}`
								: 'Browse an actor workspace by path prefix'}
						>
							<div class="mb-2 flex flex-wrap items-end gap-2">
								<div class="min-w-40 flex-1">
									<Input
										label="Path prefix"
										size="sm"
										bind:value={treePrefix}
										placeholder="src/lib (empty lists all)"
									/>
								</div>
								<Button
									variant="outline"
									size="sm"
									disabled={!workspaceActor.trim()}
									onclick={() => void reloadTree()}
								>
									Apply filter
								</Button>
							</div>
							{#if previewTree && previewTree.entries.length > 0}
								<ul class="space-y-1">
									{#each previewTree.entries as entry (entry.path)}
										<li
											class="flex items-center justify-between gap-2 text-caption"
										>
											<span class="min-w-0 flex-1 truncate font-mono"
												>{entry.path}</span
											>
											<span
												class="shrink-0 text-micro tabular-nums text-muted-foreground"
												>{formatBytes(entry.size)}</span
											>
										</li>
									{/each}
								</ul>
							{:else}
								<p class="text-caption text-muted-foreground">
									No tree loaded yet — select a change or apply a filter.
								</p>
							{/if}
						</Card>
					</div>
				</div>

				<div class="mt-3 grid gap-3 xl:grid-cols-2">
					<Card
						title="Partitions"
						description="{partitions.length} ref lines in the checkpoint store"
					>
						{#if partitions.length === 0}
							<p class="text-caption text-muted-foreground">
								No partitions reported.
							</p>
						{:else}
							<ul class="space-y-1.5">
								{#each partitions as partition (partition.partitionId)}
									<li
										class="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-caption"
									>
										<Badge variant="neutral">{partition.kind}</Badge>
										<span class="min-w-0 flex-1 truncate font-mono"
											>{partition.name}</span
										>
										{#if partition.actor}
											<span class="text-micro text-muted-foreground"
												>{partition.actor}</span
											>
										{/if}
										<span
											class="shrink-0 font-mono text-micro text-muted-foreground"
											>{partition.currentSnapshot.slice(0, 10)} · {partition.historyLen}</span
										>
									</li>
								{/each}
							</ul>
						{/if}
					</Card>

					<Card
						title="Conflicts"
						description={conflicts.length === 0
							? 'No unresolved merge conflicts'
							: `${conflicts.length} file(s) with unresolved merges`}
					>
						{#if conflicts.length === 0}
							<p class="text-caption text-muted-foreground">
								Conflicts are enumerated from unresolved merge commits.
							</p>
						{:else}
							<ul class="space-y-2">
								{#each conflicts as conflict (`${conflict.partition}:${conflict.path}`)}
									<li
										class="flex flex-wrap items-center gap-x-2 gap-y-1 text-caption"
									>
										<span class="min-w-0 flex-1 truncate font-mono"
											>{conflict.path}</span
										>
										<span class="text-micro text-muted-foreground"
											>{conflict.partition} · {conflict.conflictCount} regions</span
										>
										<Button
											variant="outline"
											size="sm"
											disabled={checkpointStore.busy}
											onclick={() => openResolve(conflict)}
										>
											Resolve
										</Button>
									</li>
								{/each}
							</ul>
						{/if}
					</Card>
				</div>

				<div class="mt-3 grid gap-3 xl:grid-cols-2">
					<Card
						title="Review & merge"
						description="Submit edit lines, merge features, roll back merged features"
					>
						<div class="grid gap-2 sm:grid-cols-2">
							<Input
								label="Actor"
								size="sm"
								bind:value={reviewActor}
								placeholder="Defaults to workspace actor"
							/>
							<Input
								label="Feature"
								size="sm"
								bind:value={reviewFeature}
								placeholder="Feature name for refs/wf/feat/<name>"
							/>
						</div>
						<div class="mt-2">
							<Input
								label="Features for main (comma or space separated)"
								size="sm"
								bind:value={reviewFeaturesText}
								placeholder="demo, next"
							/>
						</div>
						<div class="mt-2 flex flex-wrap gap-2">
							<Button
								variant="outline"
								size="sm"
								disabled={checkpointStore.busy}
								onclick={() => void runSubmitReview()}
							>
								Submit for review
							</Button>
							<Button
								variant="outline"
								size="sm"
								disabled={checkpointStore.busy}
								onclick={() => void runMergeToFeature()}
							>
								Merge to feature
							</Button>
							<Button
								variant="outline"
								size="sm"
								disabled={checkpointStore.busy}
								onclick={() => void runMergeToMain()}
							>
								Merge to main
							</Button>
						</div>
					</Card>

					<Card
						title="Feature rollback"
						description="Append a revert commit; other merged features stay untouched"
					>
						<Input
							label="Merged feature head commit"
							size="sm"
							bind:value={rollbackHead}
							placeholder="Merge commit id to revert"
						/>
						<div class="mt-2">
							<Button
								variant="outline"
								size="sm"
								disabled={checkpointStore.busy || !rollbackHead.trim()}
								onclick={() => void runRollbackMergedFeature()}
							>
								Roll back feature
							</Button>
						</div>
					</Card>
				</div>

				<div class="mt-3 grid gap-3 xl:grid-cols-2">
					<Card
						title="Edit sessions"
						description="Staged multi-file operations; rollback appends a revert commit"
					>
						{#snippet actions()}
							<Button
								variant="outline"
								size="sm"
								disabled={checkpointStore.busy}
								onclick={() => void runBeginSession()}
							>
								<Icon name="plus" size={13} />
								New session
							</Button>
						{/snippet}
						<div class="mb-2">
							<Input
								label="Rollback actor"
								size="sm"
								bind:value={sessionActor}
								placeholder="Actor owning the session partition"
							/>
						</div>
						{#if sessions.length === 0}
							<p class="text-caption text-muted-foreground">
								No staged edit sessions.
							</p>
						{:else}
							<ul class="space-y-2">
								{#each sessions as session (session.id)}
									<li
										class="flex flex-wrap items-center gap-x-2 gap-y-1 text-caption"
									>
										<span class="min-w-0 flex-1 truncate font-mono"
											>{session.id}</span
										>
										{#if session.label}
											<span class="text-micro text-muted-foreground"
												>{session.label}</span
											>
										{/if}
										<Button
											variant="outline"
											size="sm"
											disabled={checkpointStore.busy || !sessionActor.trim()}
											title="Roll back this session on the actor partition"
											onclick={() => void runRollbackSession(session)}
										>
											Rollback
										</Button>
									</li>
								{/each}
							</ul>
						{/if}
					</Card>

					<Card
						title="Workspace & index"
						description="Actor undo/redo, workspace inspection and source-index health"
					>
						<div class="mb-2 grid gap-2 sm:grid-cols-2">
							<Input
								label="Actor"
								size="sm"
								bind:value={undoActor}
								placeholder="Actor partition"
							/>
							<Input
								label="Workspace actor"
								size="sm"
								bind:value={workspaceActor}
								placeholder="Actor to inspect"
							/>
						</div>
						<div class="flex flex-wrap gap-2">
							<Button
								variant="outline"
								size="sm"
								disabled={checkpointStore.busy}
								onclick={() => void runUndo()}
							>
								<Icon name="arrow-left" size={13} />
								Undo
							</Button>
							<Button
								variant="outline"
								size="sm"
								disabled={checkpointStore.busy}
								onclick={() => void runRedo()}
							>
								Redo
							</Button>
							<Button
								variant="outline"
								size="sm"
								disabled={checkpointStore.busy}
								onclick={() => void loadWorkspace()}
							>
								Inspect workspace
							</Button>
							<Button
								variant="outline"
								size="sm"
								disabled={checkpointStore.busy}
								onclick={() => void runRebuildIndex()}
							>
								<Icon name="database" size={13} />
								Rebuild index
							</Button>
						</div>
						{#if checkpointStore.actionError}
							<p class="mt-2 text-caption text-destructive">
								{checkpointStore.actionError}
							</p>
						{/if}
						{#if indexStatus}
							<p class="mt-2 text-caption tabular-nums text-muted-foreground">
								Index: {indexStatus.indexedCommits}/{indexStatus.totalCommits} commits{indexStatus.needsRebuild
									? ' · rebuild recommended'
									: ' · healthy'}
							</p>
						{/if}
						{#if workspaceFiles.length > 0}
							<ul class="mt-2 space-y-1">
								{#each workspaceFiles.slice(0, 20) as file (file.path)}
									<li
										class="flex items-center justify-between gap-2 text-caption"
									>
										<span class="min-w-0 flex-1 truncate font-mono"
											>{file.path}</span
										>
										<span
											class="shrink-0 font-mono text-micro text-muted-foreground"
											>{file.hash.slice(0, 10)}</span
										>
									</li>
								{/each}
							</ul>
							{#if workspaceFiles.length > 20}
								<p class="mt-1 text-micro text-muted-foreground">
									Showing 20 of {workspaceFiles.length} files.
								</p>
							{/if}
						{/if}
					</Card>
				</div>
			{:else}
				<div class="space-y-2">
					{#each approvals as approval (approval.id)}
						<Card title={approval.title}>
							{#snippet actions()}
								<StatusBadge status={approval.status} size="sm" />
							{/snippet}
							<p class="text-caption">{approval.detail}</p>
							<div
								class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-micro text-muted-foreground"
							>
								<span>{approval.kind}</span>
								<span>{approval.requester}</span>
								<span>{formatDateTime(approval.requestedAt)}</span>
								<span class="font-mono">{approval.executionId}</span>
							</div>
							{#snippet footer()}
								{#if approval.status === 'pending'}
									<div class="flex items-center gap-2">
										<Button
											size="sm"
											onclick={() => void decideApproval(approval, true)}
										>
											<Icon name="check" size={13} />
											Approve
										</Button>
										<Button
											variant="outline"
											size="sm"
											onclick={() => void decideApproval(approval, false)}
										>
											<Icon name="x" size={13} />
											Reject
										</Button>
									</div>
								{:else}
									<span class="text-caption text-muted-foreground">
										Resolved · {approval.status}
									</span>
								{/if}
							{/snippet}
						</Card>
					{/each}
				</div>
			{/if}
		</PageState>
	</div>
</div>

<Dialog
	bind:open={createOpen}
	title="Create file checkpoint"
	description="Capture the full workspace for one actor."
>
	<Input label="Actor" bind:value={createActor} placeholder="manual" />
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (createOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy || !createActor.trim()}
			onclick={() => void runCreateFileCheckpoint()}
		>
			Create
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={restoreOpen}
	title="Restore checkpoint"
	description="Restore lands as a new commit on top of history. Pointers are never moved and other branches are untouched."
	onclose={() => (restoreTarget = null)}
>
	{#if restoreTarget}
		<p class="text-caption">
			Restore <span class="font-mono">{restoreTarget.id}</span>
			({restoreTarget.kind}
			· {restoreTarget.actor}). Files covered by this checkpoint return to the
			recorded state; everything else stays as-is.
		</p>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (restoreOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void runRestore()}
		>
			Restore as new commit
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={deleteOpen}
	title="Delete checkpoint"
	description="Delete removes the checkpoint record. File history in the object store is reclaimed by garbage collection."
	onclose={() => (deleteTarget = null)}
>
	{#if deleteTarget}
		<p class="text-caption">
			Delete <span class="font-mono">{deleteTarget.id}</span>? This cannot be
			undone from this page.
		</p>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (deleteOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy}
			onclick={() => void runDelete()}
		>
			Delete
		</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={rejectOpen}
	title="Reject approval"
	description="Rejecting rolls the approval partition back to its baseline. Only the pending reference is removed; history is untouched."
	onclose={() => (rejectTarget = null)}
>
	<Textarea
		label="Reason (optional)"
		bind:value={rejectReason}
		placeholder="Why is this change unacceptable? What should change next?"
		rows={3}
	/>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (rejectOpen = false)}>
			Cancel
		</Button>
		<Button size="sm" onclick={() => void runReject()}>Reject</Button>
	{/snippet}
</Dialog>

<Dialog
	bind:open={resolveOpen}
	title="Resolve conflict"
	description="Commit the resolved file content as a child of the conflicted head. Files still carrying markers stay conflicted."
	onclose={() => (resolveTarget = null)}
>
	{#if resolveTarget}
		<p class="mb-2 text-caption">
			<span class="font-mono">{resolveTarget.path}</span> on
			<span class="font-mono">{resolveTarget.partition}</span>
		</p>
	{/if}
	<div class="grid gap-2 sm:grid-cols-2">
		<Input
			label="Actor"
			bind:value={resolveActor}
			placeholder="Resolving actor"
		/>
		<Input
			label="Feature"
			bind:value={resolveFeature}
			placeholder="Feature name for refs/wf/feat/<name>"
		/>
	</div>
	<div class="mt-2">
		<Textarea
			label="Resolved content"
			bind:value={resolveContent}
			placeholder="Paste the full resolved file content"
			rows={10}
		/>
	</div>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (resolveOpen = false)}>
			Cancel
		</Button>
		<Button
			size="sm"
			disabled={checkpointStore.busy ||
				!resolveActor.trim() ||
				!resolveFeature.trim() ||
				!resolveContent}
			onclick={() => void runResolveConflict()}
		>
			Commit resolution
		</Button>
	{/snippet}
</Dialog>
