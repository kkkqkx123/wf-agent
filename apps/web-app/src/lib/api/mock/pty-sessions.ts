/**
 * Shared PTY session registry for the fixture-backed preview.
 *
 * The REST routes (`routes/pty.ts`) and the scripted shell WebSocket
 * (`pty-socket.ts`) both read this table, so a session created over REST
 * is the same session the mock socket serves — reconnects and the session
 * list render real data instead of an empty array.
 */

export interface PreviewPtySession {
	id: string;
	cwd: string;
	title: string;
	status: 'running' | 'exited';
	exit_code: number | null;
	pid: number;
	rows: number;
	cols: number;
}

const sessions = new Map<string, PreviewPtySession>();
let seq = 0;

export function createSession(input: { rows?: number; cols?: number; title?: string }): PreviewPtySession {
	seq += 1;
	const session: PreviewPtySession = {
		id: `pty-preview-${seq}`,
		cwd: '/preview/wf-agent',
		title: input.title ?? `Terminal ${seq}`,
		status: 'running',
		exit_code: null,
		pid: 42000 + seq,
		rows: input.rows ?? 24,
		cols: input.cols ?? 80,
	};
	sessions.set(session.id, session);
	return session;
}

export function getSession(id: string): PreviewPtySession | undefined {
	return sessions.get(id);
}

export function listSessions(): PreviewPtySession[] {
	return [...sessions.values()];
}

export function resizeSession(id: string, rows?: number, cols?: number): PreviewPtySession | undefined {
	const session = sessions.get(id);
	if (!session) return undefined;
	if (rows != null) session.rows = rows;
	if (cols != null) session.cols = cols;
	return session;
}

export function renameSession(id: string, title?: string): PreviewPtySession | undefined {
	const session = sessions.get(id);
	if (!session) return undefined;
	if (title) session.title = title;
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
