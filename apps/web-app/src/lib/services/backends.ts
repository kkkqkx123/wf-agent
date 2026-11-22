import { client } from '#lib/api/client';
import { ApiHttpError, call } from '#lib/api/envelope';
import type { components } from '#lib/api/schema';

type BackendListView = components['schemas']['BackendListView'];
type BackendEntryView = components['schemas']['BackendEntryView'];

/** One scoped backend row: the connected server's view of a remote. */
export interface BackendRow {
	name: string;
	url: string;
	reachable: boolean;
	latencyMs: number | null;
	version: string | null;
	apiVersion: string | null;
	tunnelRunning: boolean;
	interruptedReason: string | null;
}

/** Map one registry view row, tolerating absent optional fields. */
export function toBackendRow(row: BackendEntryView): BackendRow {
	return {
		name: row.name,
		url: row.url,
		reachable: row.status?.reachable ?? false,
		latencyMs: row.status?.latency_ms ?? null,
		version: row.status?.version ?? null,
		apiVersion: row.status?.api_version ?? null,
		tunnelRunning: row.tunnel?.running ?? false,
		interruptedReason: row.tunnel?.interrupted_reason ?? null,
	};
}

/** Map a list view to sorted rows. Nullish or malformed views yield no rows. */
export function toBackendRows(view: BackendListView | null | undefined): BackendRow[] {
	if (!view || !Array.isArray(view.backends)) return [];
	return view.backends.map(toBackendRow).sort((a, b) => a.name.localeCompare(b.name));
}

/**
 * Scoped read-only view of the connected server's remotes. Registration
 * stays a CLI operation; this only reads what the current connection
 * serves. Null when the backend domain is disabled or unreachable, so
 * callers render no backend section instead of failing the page. Other
 * failures (auth, corrupt payload) propagate.
 */
export async function listBackends(): Promise<BackendRow[] | null> {
	try {
		const view = await call<BackendListView>(client.GET('/api/v1/system/backends'));
		return toBackendRows(view);
	} catch (e) {
		if (e instanceof ApiHttpError && e.status === 503) return null;
		if (e instanceof TypeError) return null;
		throw e;
	}
}

// ---------------------------------------------------------------------------
// Remote deployment: the management service performs the SSH work. The web
// frontend only assembles the request and renders the plan/steps, so it holds
// no remote state and no credentials, exactly like the CLI.
// ---------------------------------------------------------------------------

type DeployPlanView = components['schemas']['DeployPlanView'];
type DeployOutcomeView = components['schemas']['DeployOutcomeView'];
type DeployProbeView = components['schemas']['DeployProbeView'];
type DeployStatusView = components['schemas']['DeployStatusView'];
type DeploySourceSchema = components['schemas']['DeploySourceView'];
type RemoteInstallationSchema = components['schemas']['RemoteInstallation'];
type DeployTargetSchema = components['schemas']['DeployTarget'];

export type DeployAction = 'probe' | 'install' | 'start' | 'stop' | 'upgrade' | 'uninstall';
export type DeployScope = 'none' | 'config' | 'full';

/** Wire shape shared with the CLI; the server validates every field. */
export interface DeployRequestPayload {
	action: DeployAction;
	scope?: DeployScope;
	/** Deploy source directory relative to the server's source root. */
	source?: string | null;
	artifact?: {
		kind: string;
		path?: string;
		version?: string;
		sha256?: string;
	} | null;
	force?: boolean;
}

/** A local deploy source directory on the management server. */
export interface DeploySource {
	name: string;
	path: string;
	configFiles: string[];
	scriptFiles: string[];
	problem: string | null;
}

export interface DeployStep {
	id: string;
	description: string;
	command: string;
}

export interface DeployTarget {
	os: string;
	arch: string;
	platform: string;
	home: string;
	curl: boolean;
	wfServerOnPath: string | null;
}

export interface DeployInstallation {
	root: string;
	current: string | null;
	version: string | null;
	versions: string[];
	pid: number | null;
	running: boolean;
	healthy: boolean;
	dirs: string[];
	configFiles: string[];
	scriptFiles: string[];
}

export interface DeployCheck {
	id: string;
	ok: boolean;
	detail: string;
}

export interface DeployPlan {
	backend: string;
	action: string;
	installRoot: string;
	scope: string;
	source: string;
	serverPort: number;
	target: DeployTarget | null;
	installation: DeployInstallation | null;
	artifact: { kind: string; source: string; bytes: number | null; status: string } | null;
	files: { path: string; bytes: number; mode: string }[];
	steps: DeployStep[];
	warnings: string[];
	blocked: string | null;
}

export interface DeployStepResult {
	id: string;
	ok: boolean;
	exitCode: number | null;
	stderr: string | null;
	stdout: string | null;
}

export interface DeployOutcome {
	backend: string;
	action: string;
	ok: boolean;
	installRoot: string;
	version: string | null;
	steps: DeployStepResult[];
	error: string | null;
}

function toTarget(view: DeployTargetSchema | null | undefined): DeployTarget | null {
	if (!view) return null;
	return {
		os: view.os,
		arch: view.arch,
		platform: view.platform,
		home: view.home,
		curl: view.curl,
		wfServerOnPath: view.wf_server_on_path ?? null,
	};
}

function toInstallation(
	view: RemoteInstallationSchema | null | undefined,
): DeployInstallation | null {
	if (!view) return null;
	return {
		root: view.root,
		current: view.current ?? null,
		version: view.version ?? null,
		versions: view.versions ?? [],
		pid: view.pid ?? null,
		running: view.running,
		healthy: view.healthy,
		dirs: view.dirs ?? [],
		configFiles: view.config_files ?? [],
		scriptFiles: view.script_files ?? [],
	};
}

function toPlan(view: DeployPlanView): DeployPlan {
	return {
		backend: view.backend,
		action: view.action,
		installRoot: view.install_root,
		serverPort: view.server_port,
		scope: view.scope ?? 'none',
		source: view.source ?? '',
		target: toTarget(view.target),
		installation: toInstallation(view.installation),
		artifact: view.artifact
			? {
					kind: view.artifact.kind,
					source: view.artifact.source,
					bytes: view.artifact.bytes ?? null,
					status: view.artifact.status,
				}
			: null,
		files: (view.files ?? []).map((file) => ({
			path: file.path,
			bytes: file.bytes,
			mode: file.mode,
		})),
		steps: (view.steps ?? []).map((step) => ({
			id: step.id,
			description: step.description,
			command: step.command,
		})),
		warnings: view.warnings ?? [],
		blocked: view.blocked ?? null,
	};
}

function toOutcome(view: DeployOutcomeView): DeployOutcome {
	return {
		backend: view.backend,
		action: view.action,
		ok: view.ok,
		installRoot: view.install_root,
		version: view.version ?? null,
		steps: (view.steps ?? []).map((step) => ({
			id: step.id,
			ok: step.ok,
			exitCode: step.exit_code ?? null,
			stderr: step.stderr ?? null,
			stdout: step.stdout ?? null,
		})),
		error: view.error ?? null,
	};
}

/**
 * Dry run: probe the host and return the exact command sequence plus the
 * blocking reasons. Nothing is written.
 */
export async function planBackendDeploy(
	name: string,
	request: DeployRequestPayload,
): Promise<DeployPlan> {
	const view = await call<DeployPlanView>(
		client.POST('/api/v1/system/backends/{name}/deploy/plan', {
			params: { path: { name } },
			body: request as never,
		}),
	);
	return toPlan(view);
}

/** Execute the same plan; the response carries per-step results. */
export async function applyBackendDeploy(
	name: string,
	request: DeployRequestPayload,
): Promise<DeployOutcome> {
	const view = await call<DeployOutcomeView>(
		client.POST('/api/v1/system/backends/{name}/deploy', {
			params: { path: { name } },
			body: request as never,
		}),
	);
	return toOutcome(view);
}

/** Probe result: host facts, current install, per-check verdict and errors. */
export interface DeployProbe {
	target: DeployTarget | null;
	installation: DeployInstallation | null;
	verdict: string;
	checks: DeployCheck[];
	error: string | null;
}

/** Ask the server to inspect one host now. Costs one SSH round trip. */
export async function probeBackendDeploy(name: string): Promise<DeployProbe> {
	const view = await call<DeployProbeView>(
		client.POST('/api/v1/system/backends/{name}/deploy/probe', {
			params: { path: { name } },
		}),
	);
	return {
		target: toTarget(view.target),
		installation: toInstallation(view.installation),
		verdict: view.verdict ?? '',
		checks: (view.checks ?? []).map((check) => ({
			id: check.id,
			ok: check.ok,
			detail: check.detail,
		})),
		error: view.error ?? null,
	};
}

/**
 * Local deploy source directories exposed by the management server. This is
 * read-only and costs no SSH round trip, so the panel can render it on open.
 */
export async function fetchDeploySources(name: string): Promise<DeploySource[]> {
	const view = await call<DeployStatusView>(
		client.GET('/api/v1/system/backends/{name}/deploy', {
			params: { path: { name } },
		}),
	);
	return (view.sources ?? []).map((source: DeploySourceSchema) => ({
		name: source.name ?? '',
		path: source.path,
		configFiles: source.config_files ?? [],
		scriptFiles: source.script_files ?? [],
		problem: source.problem ?? null,
	}));
}
