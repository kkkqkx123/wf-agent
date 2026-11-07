/**
 * Shared PTY session registry for the fixture-backed preview.
 *
 * The REST routes (`routes/pty.ts`) and the scripted shell WebSocket
 * (`pty-socket.ts`) both read this table, so a session created over REST
 * is the same session the mock socket serves — reconnects and the session
 * list render real data instead of an empty array.
 */

export type PreviewPtyTitleSource = 'default' | 'auto' | 'manual';

export interface PreviewPtySession {
	id: string;
	cwd: string;
	command?: string;
	env?: Record<string, string>;
	title: string;
	title_source: PreviewPtyTitleSource;
	status: 'running' | 'exited';
	exit_code: number | null;
	pid: number;
	rows: number;
	cols: number;
	created_at: number;
	last_activity: number;
}

const sessions = new Map<string, PreviewPtySession>();
let seq = 0;
// The mock server forgets everything on page reload while the session store
// restores stale ids from localStorage. A per-lifetime suffix keeps fresh ids
// from ever colliding with restored ones (which would duplicate an each key).
const lifetime = Date.now().toString(36);

export function createSession(input: {
	rows?: number;
	cols?: number;
	title?: string;
	title_source?: PreviewPtyTitleSource;
	cwd?: string;
	command?: string;
	env?: Record<string, string>;
}): PreviewPtySession {
	seq += 1;
	const now = Date.now();
	const session: PreviewPtySession = {
		id: `pty-preview-${lifetime}-${seq}`,
		cwd: input.cwd ?? '/preview/wf-agent',
		title: input.title ?? `Terminal ${seq}`,
		title_source: input.title_source ?? 'default',
		status: 'running',
		exit_code: null,
		pid: 42000 + seq,
		rows: input.rows ?? 24,
		cols: input.cols ?? 80,
		created_at: now,
		last_activity: now,
	};
	if (input.command) session.command = input.command;
	if (input.env && Object.keys(input.env).length > 0) session.env = { ...input.env };
	sessions.set(session.id, session);
	return session;
}

export function getSession(id: string): PreviewPtySession | undefined {
	return sessions.get(id);
}

export function listSessions(): PreviewPtySession[] {
	return [...sessions.values()];
}

export function resizeSession(
	id: string,
	rows?: number,
	cols?: number,
): PreviewPtySession | undefined {
	const session = sessions.get(id);
	if (!session) return undefined;
	if (rows != null) session.rows = rows;
	if (cols != null) session.cols = cols;
	session.last_activity = Date.now();
	return session;
}

export function renameSession(
	id: string,
	title?: string,
	title_source?: PreviewPtyTitleSource,
): PreviewPtySession | undefined {
	const session = sessions.get(id);
	if (!session) return undefined;
	if (title) {
		if (title_source === 'auto' && session.title_source === 'manual') return session;
		session.title = title;
		session.title_source = title_source ?? 'manual';
	}
	session.last_activity = Date.now();
	return session;
}

/** Mark a session as ended (shell `exit`); returns true when it existed. */
export function exitSession(id: string, exitCode = 0): boolean {
	const session = sessions.get(id);
	if (!session) return false;
	session.status = 'exited';
	session.exit_code = exitCode;
	return true;
}

export function removeSession(id: string): boolean {
	return sessions.delete(id);
}

/** Repository key holding the cross-store attach demo records. */
export const DEMO_FOREIGN_REPO = '/preview/other-repo';
const DEMO_STORAGE_PREFIX = 'wf.pty.v1';
let lastDemoIds: string[] = [];

/**
 * Preview-only seed for the attach dialog foreign section: preview mounts a
 * single repository, so no second store ever opens organically. Creates two
 * live mock sessions and writes their persisted records under a foreign
 * repository key; the dialog storage-key scan picks them up. Record shape
 * mirrors the session store validator (id plus title is all it requires).
 * Reseeding removes the previous demo sessions first so the mock table
 * does not accumulate orphans.
 */
export function seedAttachDemo(): { repo: string; ids: string[] } {
	for (const id of lastDemoIds) sessions.delete(id);
	const first = createSession({
		title: 'Demo service log',
		cwd: DEMO_FOREIGN_REPO,
		command: 'tail -f service.log',
	});
	const second = createSession({ title: 'Demo migration shell', cwd: DEMO_FOREIGN_REPO });
	lastDemoIds = [first.id, second.id];
	const firstBuffer = 'service started on :8080\r\nworker pool ready (4/4)\r\n';
	const records = [
		{
			v: 1,
			id: first.id,
			title: first.title,
			titleNumber: 1,
			rows: first.rows,
			cols: first.cols,
			status: 'running',
			buffer: firstBuffer,
			scrollY: 0,
			cursor: firstBuffer.length,
			origin: { cwd: DEMO_FOREIGN_REPO, command: 'tail -f service.log' },
		},
		{
			v: 1,
			id: second.id,
			title: second.title,
			titleNumber: 2,
			rows: second.rows,
			cols: second.cols,
			status: 'running',
		},
	];
	const payload = { v: 1, sessions: records, detachedIds: [second.id] };
	try {
		localStorage.setItem(`${DEMO_STORAGE_PREFIX}.${DEMO_FOREIGN_REPO}`, JSON.stringify(payload));
		localStorage.setItem(`${DEMO_STORAGE_PREFIX}.${DEMO_FOREIGN_REPO}.active`, first.id);
	} catch {
		// Storage unavailable; the sessions still exist server-side.
	}
	return { repo: DEMO_FOREIGN_REPO, ids: [...lastDemoIds] };
}
