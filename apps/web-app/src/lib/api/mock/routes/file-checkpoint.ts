/** Mock routes: the file-checkpoint workspace (changes, approvals, merges). */

import {
	fileChanges,
	approvals,
	partitions,
	conflicts,
	editSessions,
} from '#lib/fixtures/checkpoints';
import {
	addMockCheckpoint,
	decideMockApproval,
	emitMockEvent,
	resolveMockConflicts,
} from '#lib/fixtures/checkpoints';
import { FIXTURE_EPOCH } from '#lib/fixtures/clock';
import { pageView, type MockRouter } from '../router';

export function registerFileCheckpoint(router: MockRouter): void {
	// ---------- file checkpoint ----------
	router.on('GET', '/api/v1/file-checkpoint/changes', (_p, query) => {
		return pageView(
			fileChanges.map((change) => ({
				file: change.path,
				source: change.actor,
				timestamp: change.at,
				snapshot_id: change.id,
				hash: 'preview',
				kind: change.changeType ?? 'modified',
				additions: change.additions ?? 0,
				deletions: change.deletions ?? 0,
				message: change.session || null,
			})),
			+(query.limit ?? 100),
			+(query.offset ?? 0),
		);
	});
	router.on('GET', '/api/v1/file-checkpoint/approvals/pending', () => {
		return pageView(
			approvals
				.filter((approval) => approval.status === 'pending')
				.map((approval) => ({
					actor: approval.requester,
					snapshot_id: approval.id,
					submitted_at: approval.requestedAt,
					changes: [
						{
							file: `preview/${approval.id}.rs`,
							source: approval.requester,
							timestamp: approval.requestedAt,
							snapshot_id: approval.id,
							hash: 'preview',
							kind: 'modified',
							additions: 1,
							deletions: 0,
						},
					],
				})),
		);
	});
	router.on('GET', '/api/v1/file-checkpoint/content', (_p, query) => {
		const path = String(query.path ?? fileChanges[0]?.path ?? 'preview.txt');
		const actor = String(query.actor ?? fileChanges[0]?.actor ?? 'agent');
		return {
			path,
			actor,
			hash: 'preview',
			size: 240,
			is_binary: false,
			content: `# Preview sample\n\nRead-only content for ${path} in workspace ${actor}.\n`,
			truncated: false,
			timestamp: FIXTURE_EPOCH,
		};
	});
	router.on('GET', '/api/v1/file-checkpoint/diff/actors/{a}/{b}', () => [
		{
			path: 'crates/checkpoint/src/restore_coordinator.rs',
			kind: 'Modified',
			diff: [
				'--- a/crates/checkpoint/src/restore_coordinator.rs',
				'+++ b/crates/checkpoint/src/restore_coordinator.rs',
				'@@ -12,7 +12,7 @@',
				' pub fn restore(&self, target: &Branch) -> Result<Snapshot> {',
				'-    if self.branch_exists(target)? {',
				'-        return Err(Error::BranchConflict(target.clone()));',
				'-    }',
				'+    if let Some(existing) = self.find_snapshot(target)? {',
				'+        return Ok(existing);',
				'+    }',
				'     self.write_snapshot(target)',
				' }',
			].join('\n'),
			additions: 3,
			deletions: 3,
		},
	]);
	router.on('GET', '/api/v1/file-checkpoint/diff/staged/{id}', () => [
		{
			path: 'crates/checkpoint/src/restore_coordinator.rs',
			kind: 'Modified',
			diff: [
				'--- a/crates/checkpoint/src/restore_coordinator.rs',
				'+++ b/crates/checkpoint/src/restore_coordinator.rs',
				'@@ -12,7 +12,7 @@',
				' pub fn restore(&self, target: &Branch) -> Result<Snapshot> {',
				'-    if self.branch_exists(target)? {',
				'-        return Err(Error::BranchConflict(target.clone()));',
				'-    }',
				'+    if let Some(existing) = self.find_snapshot(target)? {',
				'+        return Ok(existing);',
				'+    }',
				'     self.write_snapshot(target)',
				' }',
			].join('\n'),
			additions: 3,
			deletions: 3,
		},
		{
			path: 'crates/checkpoint/src/branch_probe.rs',
			kind: 'Modified',
			diff: [
				'--- a/crates/checkpoint/src/branch_probe.rs',
				'+++ b/crates/checkpoint/src/branch_probe.rs',
				'@@ -4,6 +4,7 @@',
				' pub fn probe(branch: &Branch) -> Probe {',
				'     let mut probe = Probe::new(branch);',
				'+    probe.with_cache(true);',
				'     probe.run()',
				' }',
			].join('\n'),
			additions: 1,
			deletions: 0,
		},
	]);
	router.on('GET', '/api/v1/file-checkpoint/tree/{id}', () => ({
		entries: fileChanges.map((change) => ({
			path: change.path,
			hash: 'preview',
			size: 1024,
			timestamp: FIXTURE_EPOCH,
		})),
		truncated: false,
		total: fileChanges.length,
	}));
	router.on('GET', '/api/v1/file-checkpoint/timeline/{id}', (params) => ({
		original_path: params.id,
		entries: [
			{
				path: params.id,
				snapshot_id: 'preview-snap-2',
				content_hash: 'preview',
				timestamp: FIXTURE_EPOCH,
				source: 'agent:preview',
			},
			{
				path: params.id,
				snapshot_id: 'preview-snap-1',
				content_hash: 'preview',
				timestamp: FIXTURE_EPOCH - 3_600_000,
				source: 'manual',
			},
		],
		truncated: false,
		total: 2,
	}));
	router.on('POST', '/api/v1/file-checkpoint/approvals/{id}/approve', (params) => {
		decideMockApproval(params.id, true);
		return { ok: true };
	});
	router.on('POST', '/api/v1/file-checkpoint/approvals/{id}/reject', (params) => {
		decideMockApproval(params.id, false);
		return { baseline_snapshot_id: 'preview-baseline' };
	});
	router.on('GET', '/api/v1/file-checkpoint/partitions', () => partitions);
	router.on('GET', '/api/v1/file-checkpoint/conflicts', () => conflicts);
	router.on('POST', '/api/v1/file-checkpoint/conflicts/resolve', (_p, _q, body) => {
		const remaining = resolveMockConflicts(Object.keys(body?.resolutions ?? {}));
		return { remaining };
	});
	router.on('GET', '/api/v1/file-checkpoint/sessions', () => editSessions);
	router.on('POST', '/api/v1/file-checkpoint/sessions', () => 'sess-preview-1');
	router.on(
		'POST',
		'/api/v1/file-checkpoint/sessions/{id}/rollback/{actor}',
		() => 'preview-rollback-snap',
	);
	router.on('POST', '/api/v1/file-checkpoint/undo/{id}', () => 'preview-undo-snap');
	router.on('POST', '/api/v1/file-checkpoint/redo/{id}', () => 'preview-redo-snap');
	router.on('POST', '/api/v1/file-checkpoint/undo/preview', (_p, _q, body) => ({
		execution_id: String(body?.execution_id ?? 'exec-preview'),
		start_checkpoint_id: 'cp-preview-start',
		end_checkpoint_id: 'cp-preview-end',
		start_file_commit: 'preview-file-start',
		end_file_commit: 'preview-file-end',
		files: fileChanges.map((change) => change.path),
		preview: [],
	}));
	router.on('POST', '/api/v1/file-checkpoint/undo/apply', () => ({ restored: 1 }));
	router.on('GET', '/api/v1/file-checkpoint/workspace/{id}', () =>
		fileChanges.map((change) => ({
			path: change.path,
			content: '',
			hash: 'preview',
			timestamp: FIXTURE_EPOCH,
		})),
	);
	router.on('GET', '/api/v1/file-checkpoint/index/status', () => ({
		indexed_commits: 42,
		total_commits: 42,
		needs_rebuild: false,
	}));
	router.on('POST', '/api/v1/file-checkpoint/index/rebuild', () => ({ indexed_commits: 42 }));
	router.on('POST', '/api/v1/file-checkpoint/checkpoints', (_p, _q, body) => {
		const row = addMockCheckpoint({
			executionId: 'exec-7f31c2a9',
			kind: 'file',
			actor: String(body?.actor ?? 'manual'),
			note: 'Manual workspace checkpoint (preview)',
		});
		return { id: row.id, timestamp: Date.parse(row.createdAt), file_count: fileChanges.length };
	});
	router.on('POST', '/api/v1/file-checkpoint/restore', () => ({ restored: 3 }));
	router.on('POST', '/api/v1/file-checkpoint/submit', () => ({
		review_commit: 'preview-review-commit',
	}));
	router.on('GET', '/api/v1/file-checkpoint/diff/main/{id}', () => [
		{
			path: 'crates/checkpoint/src/restore_coordinator.rs',
			kind: 'Modified',
			diff: [
				'--- a/crates/checkpoint/src/restore_coordinator.rs',
				'+++ b/crates/checkpoint/src/restore_coordinator.rs',
				'@@ -12,7 +12,7 @@',
				' pub fn restore(&self, target: &Branch) -> Result<Snapshot> {',
				'-    if self.branch_exists(target)? {',
				'-        return Err(Error::BranchConflict(target.clone()));',
				'-    }',
				'+    if let Some(existing) = self.find_snapshot(target)? {',
				'+        return Ok(existing);',
				'+    }',
				'     self.write_snapshot(target)',
				' }',
			].join('\n'),
			additions: 3,
			deletions: 3,
		},
		{
			path: 'crates/checkpoint/src/idempotent_restore.rs',
			kind: 'Added',
			diff: [
				'--- /dev/null',
				'+++ b/crates/checkpoint/src/idempotent_restore.rs',
				'@@ -0,0 +1,3 @@',
				'+pub fn restore_once(target: &Branch) -> Result<Snapshot> {',
				'+    self.find_snapshot(target)?.ok_or_else(|| Error::Missing(target.clone()))',
				'+}',
			].join('\n'),
			additions: 3,
			deletions: 0,
		},
	]);
	router.on('POST', '/api/v1/file-checkpoint/merge-to-feature', (params, _q, body) => {
		// Mirror the real merge driver: unresolved conflict rows keep the merge
		// conflicted until they are resolved through the conflicts endpoint.
		const details = conflicts.map((c) => ({
			binary: false,
			file: c.path,
			regions: c.conflicts.map((r) => ({
				start_line: r.start_line,
				end_line: r.end_line,
				base_lines: [],
				ours_lines: [],
				theirs_lines: [],
			})),
		}));
		const commitId =
			details.length > 0
				? `preview-feature-conflict-${Date.now().toString(36)}`
				: `preview-feature-merge-${Date.now().toString(36)}`;
		void params;
		return {
			commit_id: commitId,
			conflict_files: conflicts.map((c) => c.path),
			details,
			parents: [String(body?.feature ?? 'refs/wf/feat/preview-feature'), 'preview-agent-head'],
		};
	});
	router.on('POST', '/api/v1/file-checkpoint/merge-to-main', () => ({
		checkpoint_id: 'preview-merge-ckpt',
		merge_result: {
			commit_id: `preview-main-merge-${Date.now().toString(36)}`,
			conflict_files: [],
			details: [],
			parents: ['preview-main-head', 'preview-review-commit'],
		},
	}));
	router.on('POST', '/api/v1/file-checkpoint/rollback-feature', () => ({
		revert_commit: `preview-revert-${Date.now().toString(36)}`,
	}));
	router.on('GET', '/api/v1/file-checkpoint/gc/stats', () => ({
		commits: 42,
		refs: partitions.length,
		loose_objects: 7,
		store_bytes: 96 * 1024 * 1024,
		last_gc_ms: FIXTURE_EPOCH - 2 * 3_600_000,
	}));
	router.on('POST', '/api/v1/file-checkpoint/gc', () => {
		emitMockEvent('CHECKPOINT_GC_COMPLETED', { reclaimed_snapshots: 3 });
		return { reclaimed_snapshots: 3, removed_checkpoints: 2, removed_snapshots: 5 };
	});
	router.on('GET', '/api/v1/file-checkpoint/state-links', (_p, query) => {
		const ids = String(query.ids ?? '')
			.split(',')
			.map((id) => id.trim())
			.filter((id) => id.length > 0);
		return ids.map((id) => ({
			checkpoint_id: id,
			state: 'linked',
			file_commit_id: `preview-file-${id.slice(0, 8)}`,
		}));
	});
	router.on(
		'POST',
		'/api/v1/file-checkpoint/rename',
		() => `preview-rename-${Date.now().toString(36)}`,
	);
	router.on('GET', '/api/v1/file-checkpoint/partitions/{id}/history', (params, query) => {
		const partition = partitions.find((p) => p.partition_id === params.id) ?? partitions[0];
		const limit = Math.min(+(query.limit ?? 50), partition.history_len);
		return Array.from({ length: limit }, (_unused, i) => ({
			snapshot_id: `${partition.current_snapshot}-${partition.history_len - i}`,
			timestamp: partition.updated_at - i * 3_600_000,
			parents: i === 0 ? [] : [`${partition.current_snapshot}-${partition.history_len - i + 1}`],
			is_merge: false,
			has_conflicts: partition.partition_id === 'approval' && i === 0,
			message: i === 0 ? 'Head commit (preview)' : `Preview commit ${partition.history_len - i}`,
			actor: partition.actor,
			session: null,
			tool: null,
		}));
	});
}
