import type { Approval, Checkpoint, FileChange } from '$lib/types/models';
import { minutesAgo } from './clock';

export const checkpoints: Checkpoint[] = [
	{
		id: 'cp-3',
		executionId: 'exec-7f31c2a9',
		sequence: 3,
		kind: 'state',
		actor: 'agent',
		createdAt: minutesAgo(6),
		sizeBytes: 18 * 1024 * 1024,
		note: 'Before extending coverage with repeated-restore case',
		restorable: true,
	},
	{
		id: 'cp-2',
		executionId: 'exec-7f31c2a9',
		sequence: 2,
		kind: 'file',
		actor: 'agent',
		createdAt: minutesAgo(28),
		sizeBytes: 24 * 1024 * 1024,
		note: 'After patching restore coordinator',
		restorable: true,
	},
	{
		id: 'cp-1',
		executionId: 'exec-7f31c2a9',
		sequence: 1,
		kind: 'state',
		actor: 'system',
		createdAt: minutesAgo(38),
		sizeBytes: 12 * 1024 * 1024,
		note: 'Initial snapshot at loop start',
		restorable: true,
	},
	{
		id: 'cp-9d47-5',
		executionId: 'loop-9d47e015',
		sequence: 5,
		kind: 'state',
		actor: 'agent',
		createdAt: minutesAgo(22),
		sizeBytes: 31 * 1024 * 1024,
		note: 'Captured before retrying the flaky suite',
		restorable: true,
	},
	{
		id: 'cp-72be-1',
		executionId: 'loop-72be1f93',
		sequence: 1,
		kind: 'file',
		actor: 'human',
		createdAt: minutesAgo(880),
		sizeBytes: 8 * 1024 * 1024,
		note: 'Manual snapshot before permission audit',
		restorable: false,
	},
	{
		id: 'cp-4e09-4',
		executionId: 'loop-4e09dd51',
		sequence: 4,
		kind: 'state',
		actor: 'agent',
		createdAt: minutesAgo(1420),
		sizeBytes: 15 * 1024 * 1024,
		note: 'Final summary assembled',
		restorable: true,
	},
];

export const fileChanges: FileChange[] = [
	{
		id: 'fc-1',
		path: 'crates/checkpoint/src/restore_coordinator.rs',
		changeType: 'modified',
		actor: 'agent',
		at: minutesAgo(28),
		additions: 22,
		deletions: 9,
		session: 'sess-a1',
	},
	{
		id: 'fc-2',
		path: 'crates/checkpoint/src/branch_probe.rs',
		changeType: 'modified',
		actor: 'agent',
		at: minutesAgo(27),
		additions: 12,
		deletions: 2,
		session: 'sess-a1',
	},
	{
		id: 'fc-3',
		path: 'crates/checkpoint/src/idempotent_restore.rs',
		changeType: 'added',
		actor: 'agent',
		at: minutesAgo(24),
		additions: 96,
		deletions: 0,
		session: 'sess-a1',
	},
	{
		id: 'fc-4',
		path: 'crates/checkpoint/src/legacy_rollback.rs',
		changeType: 'deleted',
		actor: 'human',
		at: minutesAgo(20),
		additions: 0,
		deletions: 148,
		session: 'sess-h2',
	},
	{
		id: 'fc-5',
		path: 'docs/plan/checkpoint-restore.md',
		changeType: 'modified',
		actor: 'human',
		at: minutesAgo(18),
		additions: 34,
		deletions: 3,
		session: 'sess-h2',
	},
	{
		id: 'fc-6',
		path: 'crates/checkpoint/tests/restore_repeat.rs',
		changeType: 'added',
		actor: 'agent',
		at: minutesAgo(8),
		additions: 58,
		deletions: 0,
		session: 'sess-a3',
	},
	{
		id: 'fc-7',
		path: 'configs/checkpoint.toml',
		changeType: 'modified',
		actor: 'system',
		at: minutesAgo(6),
		additions: 4,
		deletions: 1,
		session: 'sess-s1',
	},
	{
		id: 'fc-8',
		path: 'crates/checkpoint/src/restore_coordinator.rs',
		changeType: 'renamed',
		actor: 'agent',
		at: minutesAgo(4),
		additions: 2,
		deletions: 2,
		session: 'sess-a3',
	},
	{
		id: 'fc-9',
		path: 'scripts/checkpoint-verify.sh',
		changeType: 'added',
		actor: 'system',
		at: minutesAgo(3),
		additions: 27,
		deletions: 0,
		session: 'sess-s1',
	},
	{
		id: 'fc-10',
		path: 'crates/checkpoint/src/branch_probe.rs',
		changeType: 'modified',
		actor: 'agent',
		at: minutesAgo(1),
		additions: 6,
		deletions: 0,
		session: 'sess-a3',
	},
];

export const approvals: Approval[] = [
	{
		id: 'apr-1',
		title: 'Publish artifact to staging channel',
		kind: 'publish',
		requester: 'wf-release-train',
		requestedAt: minutesAgo(2),
		status: 'pending',
		detail:
			'Artifact 2026.09.3 will become the canary build for the staging fleet.',
		executionId: 'exec-7f31c2a9',
	},
	{
		id: 'apr-2',
		title: 'Delete legacy snapshot branch',
		kind: 'destructive',
		requester: 'agent',
		requestedAt: minutesAgo(11),
		status: 'pending',
		detail: 'Branch legacy/restore-2026.02 has no remaining references.',
		executionId: 'exec-7f31c2a9',
	},
	{
		id: 'apr-3',
		title: 'Escalate shell command privileges',
		kind: 'permission',
		requester: 'loop-3c1f8ab2',
		requestedAt: minutesAgo(26),
		status: 'approved',
		detail:
			'Requested write access under scripts/ for the verification helper.',
		executionId: 'loop-3c1f8ab2',
	},
	{
		id: 'apr-4',
		title: 'Retry failed publish step',
		kind: 'retry',
		requester: 'loop-3c1f8ab2',
		requestedAt: minutesAgo(300),
		status: 'rejected',
		detail:
			'Operator declined a retry until the registry incident is resolved.',
		executionId: 'exec-2f8b1170',
	},
];

export interface PreviewPartition {
	partition_id: string;
	name: string;
	kind: string;
	actor: string | null;
	current_snapshot: string;
	history_len: number;
	created_at: number;
	updated_at: number;
}

export const partitions: PreviewPartition[] = [
	{
		partition_id: 'main',
		name: 'refs/wf/main',
		kind: 'main',
		actor: null,
		current_snapshot: 'preview-main-head',
		history_len: 42,
		created_at: Date.parse(minutesAgo(880)),
		updated_at: Date.parse(minutesAgo(4)),
	},
	{
		partition_id: 'agent',
		name: 'refs/wf/edit/agent',
		kind: 'agent',
		actor: 'agent',
		current_snapshot: 'preview-agent-head',
		history_len: 7,
		created_at: Date.parse(minutesAgo(28)),
		updated_at: Date.parse(minutesAgo(1)),
	},
	{
		partition_id: 'approval',
		name: 'refs/wf/review/agent',
		kind: 'approval',
		actor: 'agent',
		current_snapshot: 'preview-review-head',
		history_len: 2,
		created_at: Date.parse(minutesAgo(11)),
		updated_at: Date.parse(minutesAgo(2)),
	},
];

export interface PreviewConflict {
	path: string;
	snapshot_id: string;
	partition: string;
	conflicts: Array<{ start_line: number; end_line: number }>;
}

export const conflicts: PreviewConflict[] = [
	{
		path: 'crates/checkpoint/src/restore_coordinator.rs',
		snapshot_id: 'preview-conflict-snap',
		partition: 'refs/wf/feat/preview-feature',
		conflicts: [{ start_line: 12, end_line: 19 }],
	},
];

export interface PreviewEditSession {
	id: string;
	label: string | null;
	created_at: number;
}

export const editSessions: PreviewEditSession[] = [
	{
		id: 'sess-a3',
		label: 'retry coverage batch',
		created_at: Date.parse(minutesAgo(8)),
	},
	{
		id: 'sess-h2',
		label: 'manual permission audit',
		created_at: Date.parse(minutesAgo(20)),
	},
];
