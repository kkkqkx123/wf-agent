/** Definition-domain services shared by every editable definition kind.
 *
 * A draft is an unpublished definition under edit, a version is a snapshot
 * of a formal definition and a lock is the single-writer guard on editing
 * one. All three exist for every editable kind, and the kind decides which
 * endpoint answers. The kind is never guessed: an unknown kind is an error.
 */
import { client } from '#lib/api/client';
import { call, requireData } from '#lib/api/envelope';
import { diffTopology } from '#lib/graph/execution-projection';
import type { components } from '#lib/api/schema';
import type {
	DefinitionDraft,
	DefinitionIssue,
	DefinitionKind,
	DefinitionVersion,
	DependentImpact,
} from '#lib/types/models';

type LockView = components['schemas']['LockView'];
type PromoteReportDoc = components['schemas']['PromoteReportDoc'];

interface DiffNodeDto {
	id?: unknown;
}

interface DiffEdgeDto {
	id?: unknown;
	source_node_id?: unknown;
	target_node_id?: unknown;
}

/** What a promotion produced. */
export interface DefinitionPromotion {
	/** Issues the formal save raised without rejecting it. */
	warnings: DefinitionIssue[];
	/** The definitions referencing the promoted one, and the level each
	 * reached after revalidation. */
	dependents: DependentImpact[];
	errorCount: number;
	warningCount: number;
	passCount: number;
}

/** The editing lease on one definition. */
export interface DefinitionLock {
	ownerId: string;
	ownerName: string;
	expiresAt: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null;
}

function toIso(value: unknown): string {
	if (typeof value === 'number') return new Date(value).toISOString();
	if (typeof value === 'string' && value) return value;
	return '';
}

function toIssue(value: unknown): DefinitionIssue {
	if (!isRecord(value)) return { field: '', message: String(value ?? '') };
	return {
		field: typeof value.field === 'string' ? value.field : '',
		message: typeof value.message === 'string' ? value.message : '',
	};
}

function toDependent(value: unknown): DependentImpact {
	const row = isRecord(value) ? value : {};
	return {
		workflowId: String(row.workflow_id ?? ''),
		workflowName: String(row.workflow_name ?? ''),
		nodeId: String(row.node_id ?? ''),
		field: String(row.field ?? ''),
		level: String(row.level ?? ''),
		errors: Array.isArray(row.errors) ? row.errors.map(String) : [],
		warnings: Array.isArray(row.warnings) ? row.warnings.map(String) : [],
	};
}

function toLock(value: LockView | null): DefinitionLock | null {
	if (!value) return null;
	if (typeof value.owner_id !== 'string' || !value.owner_id) return null;
	return {
		ownerId: value.owner_id,
		ownerName: value.owner_name,
		expiresAt: value.expires_at,
	};
}

// ── drafts ───────────────────────────────────────────────────────

function toDraft(
	kind: DefinitionKind,
	value: unknown,
	issues: DefinitionIssue[],
): DefinitionDraft {
	const row = isRecord(value) ? value : {};
	const id = String(row.id ?? '');
	return {
		kind,
		id,
		name: String(row.name ?? id),
		updatedAt: toIso(row.updated_at ?? row.created_at),
		valid: issues.length === 0,
		issues,
		definition: row,
	};
}

/**
 * The drafts of one definition kind, each with its validation attached.
 * Drafts may be incomplete, so a draft that cannot be promoted is listed
 * with the issues that say why.
 *
 * A definition id narrows the list to the drafts of that definition: a
 * draft is a revision of one definition, so the drafts of another
 * definition are not part of this view.
 */
export async function listDefinitionDrafts(
	kind: DefinitionKind,
	definitionId?: string,
): Promise<DefinitionDraft[]> {
	const query =
		definitionId === undefined ? {} : { definition_id: definitionId };
	const data =
		kind === 'workflow'
			? await call<unknown>(
					client.GET('/api/v1/workflows/drafts', { params: { query } }),
				)
			: await call<unknown>(
					client.GET('/api/v1/agents/drafts', { params: { query } }),
				);
	const rows = Array.isArray(data) ? data : [];
	const validated = await Promise.allSettled(
		rows.map((row) => {
			const id = isRecord(row) ? String(row.id ?? '') : '';
			return validateDefinitionDraft(kind, id).then((issues): DefinitionDraft =>
				toDraft(kind, row, issues),
			);
		}),
	);
	return validated
		.filter(
			(result): result is PromiseFulfilledResult<DefinitionDraft> =>
				result.status === 'fulfilled',
		)
		.map((result) => result.value);
}

/** One draft of one definition kind, with its validation attached. */
export async function getDefinitionDraft(
	kind: DefinitionKind,
	id: string,
): Promise<DefinitionDraft> {
	const data =
		kind === 'workflow'
			? await call<unknown>(
					client.GET('/api/v1/workflows/drafts/{id}', {
						params: { path: { id } },
					}),
				)
			: await call<unknown>(
					client.GET('/api/v1/agents/drafts/{id}', {
						params: { path: { id } },
					}),
				);
	const issues = await validateDefinitionDraft(kind, id);
	return toDraft(kind, requireData(data, `Draft ${id} missing`), issues);
}

/** Store a draft. The payload is the definition as its kind stores it. */
export async function saveDefinitionDraft(
	kind: DefinitionKind,
	definition: Record<string, unknown>,
): Promise<string> {
	const id = String(definition.id ?? '');
	const data =
		kind === 'workflow'
			? await call<string>(
					client.POST('/api/v1/workflows/drafts', { body: definition }),
				)
			: await call<string>(
					client.POST('/api/v1/agents/drafts', { body: definition }),
				);
	return data || id;
}

/** Delete a draft. */
export async function deleteDefinitionDraft(
	kind: DefinitionKind,
	id: string,
): Promise<void> {
	if (kind === 'workflow') {
		await call<boolean>(
			client.DELETE('/api/v1/workflows/drafts/{id}', {
				params: { path: { id } },
			}),
		);
		return;
	}
	await call<boolean>(
		client.DELETE('/api/v1/agents/drafts/{id}', {
			params: { path: { id } },
		}),
	);
}

/** The issues that do not block the promotion of a draft. */
export async function validateDefinitionDraft(
	kind: DefinitionKind,
	id: string,
): Promise<DefinitionIssue[]> {
	const data =
		kind === 'workflow'
			? await call<unknown>(
					client.GET('/api/v1/workflows/drafts/{id}/validate', {
						params: { path: { id } },
					}),
				)
			: await call<unknown>(
					client.GET('/api/v1/agents/drafts/{id}/validate', {
						params: { path: { id } },
					}),
				);
	const rows = Array.isArray(data) ? data : [];
	return rows.map(toIssue);
}

/** Promote a draft to its formal definition. */
export async function promoteDefinitionDraft(
	kind: DefinitionKind,
	id: string,
): Promise<DefinitionPromotion> {
	if (kind === 'workflow') {
		const data = requireData(
			await call<PromoteReportDoc | null>(
				client.POST('/api/v1/workflows/drafts/{id}/promote', {
					params: { path: { id } },
				}),
			),
			`Promotion report missing for draft ${id}`,
		);
		return {
			warnings: [],
			dependents: (data.dependents ?? []).map(toDependent),
			errorCount: data.error_count ?? 0,
			warningCount: data.warning_count ?? 0,
			passCount: data.pass_count ?? 0,
		};
	}
	const data = await call<unknown>(
		client.POST('/api/v1/agents/drafts/{id}/promote', {
			params: { path: { id } },
		}),
	);
	const rows = Array.isArray(data) ? data : [];
	return {
		warnings: rows.map(toIssue),
		dependents: [],
		errorCount: 0,
		warningCount: rows.length,
		passCount: 0,
	};
}

// ── versions ─────────────────────────────────────────────────────

function toVersion(
	kind: DefinitionKind,
	value: unknown,
	index: number,
): DefinitionVersion {
	const row = isRecord(value) ? value : {};
	// A workflow version is a definition; an agent version is a template
	// wrapping one.
	const definition = kind === 'agent' ? row.definition : row;
	const payload = isRecord(definition) ? definition : {};
	const version =
		typeof payload.version === 'string' && payload.version
			? payload.version
			: `v${index + 1}`;
	const metadata = isRecord(payload.metadata) ? payload.metadata : {};
	return {
		kind,
		version,
		createdAt: toIso(payload.created_at ?? payload.updated_at),
		author: typeof metadata.author === 'string' ? metadata.author : '',
		note: String(payload.name ?? row.name ?? ''),
		current: index === 0,
		definition: payload,
	};
}

/** The saved versions of one formal definition. */
export async function listDefinitionVersions(
	kind: DefinitionKind,
	id: string,
): Promise<DefinitionVersion[]> {
	const data =
		kind === 'workflow'
			? await call<unknown>(
					client.GET('/api/v1/workflows/{id}/versions', {
						params: { path: { id } },
					}),
				)
			: await call<unknown>(
					client.GET('/api/v1/templates/library/agents/{id}/versions', {
						params: { path: { id } },
					}),
				);
	const rows = Array.isArray(data) ? data : [];
	return rows.map((row, index) => toVersion(kind, row, index));
}

/** One saved version of one formal definition. */
export async function getDefinitionVersion(
	kind: DefinitionKind,
	id: string,
	version: string,
): Promise<Record<string, unknown>> {
	const data =
		kind === 'workflow'
			? await call<unknown>(
					client.GET('/api/v1/workflows/{id}/versions/{version}', {
						params: { path: { id, version } },
					}),
				)
			: await call<unknown>(
					client.GET(
						'/api/v1/templates/library/agents/{id}/versions/{version}',
						{
							params: { path: { id, version } },
						},
					),
				);
	const row = requireData(data, `Version ${version} missing for ${id}`);
	return isRecord(row) ? row : {};
}

/** Roll a formal definition back to one of its saved versions. */
export async function rollbackDefinition(
	kind: DefinitionKind,
	id: string,
	version: string,
): Promise<void> {
	if (kind === 'workflow') {
		await call<unknown>(
			client.POST('/api/v1/workflows/{id}/rollback', {
				params: { path: { id } },
				body: { version },
			}),
		);
		return;
	}
	await call<unknown>(
		client.POST('/api/v1/templates/library/agents/{id}/rollback', {
			params: { path: { id } },
			body: { version },
		}),
	);
}

/** What changed between two saved versions. */
export interface DefinitionDiff {
	/** Nodes only the newer version has. */
	addedNodes: string[];
	/** Nodes only the older version has. */
	removedNodes: string[];
	/** Edges only the newer version has. */
	addedEdges: Array<{ source: string; target: string }>;
	/** Edges only the older version has. */
	removedEdges: Array<{ source: string; target: string }>;
	/** Definition fields whose value differs between the two versions. */
	changedFields: string[];
}

function versionDefinition(
	kind: DefinitionKind,
	value: Record<string, unknown>,
): Record<string, unknown> {
	if (kind === 'agent') {
		const inner = value.definition;
		return isRecord(inner) ? inner : value;
	}
	return value;
}

function changedTopLevelFields(
	before: Record<string, unknown>,
	after: Record<string, unknown>,
): string[] {
	const changed: string[] = [];
	const keys = new Set([...Object.keys(before), ...Object.keys(after)]);
	for (const key of keys) {
		if (key === 'version' || key === 'updated_at' || key === 'created_at') {
			continue;
		}
		const a = JSON.stringify(before[key] ?? null);
		const b = JSON.stringify(after[key] ?? null);
		if (a !== b) changed.push(key);
	}
	return changed.sort();
}

/**
 * The difference between two saved versions, computed client-side. A
 * workflow is compared by its graph, a definition without a graph by the
 * fields that changed.
 */
export async function diffDefinitionVersions(
	kind: DefinitionKind,
	id: string,
	from: string,
	to: string,
): Promise<DefinitionDiff> {
	const [a, b] = await Promise.all([
		getDefinitionVersion(kind, id, from),
		getDefinitionVersion(kind, id, to),
	]);
	const before = versionDefinition(kind, a);
	const after = versionDefinition(kind, b);
	const nodes = (value: Record<string, unknown>) =>
		(Array.isArray(value.nodes) ? (value.nodes as DiffNodeDto[]) : [])
			.map((node) => String(node.id ?? ''))
			.filter(Boolean);
	const edges = (value: Record<string, unknown>) =>
		(Array.isArray(value.edges) ? (value.edges as DiffEdgeDto[]) : []).map(
			(edge) => ({
				source: String(edge.source_node_id ?? ''),
				target: String(edge.target_node_id ?? ''),
			}),
		);
	const topology = diffTopology(
		nodes(before).map((nodeId) => ({
			id: nodeId,
			label: nodeId,
			kind: 'unknown',
		})),
		edges(before).map((edge) => ({
			id: `${edge.source}->${edge.target}`,
			source: edge.source,
			target: edge.target,
		})),
		nodes(after).map((nodeId) => ({
			id: nodeId,
			label: nodeId,
			kind: 'unknown',
		})),
		edges(after).map((edge) => ({
			id: `${edge.source}->${edge.target}`,
			source: edge.source,
			target: edge.target,
		})),
	);
	return {
		addedNodes: topology.addedNodes,
		removedNodes: topology.removedNodes,
		addedEdges: topology.addedEdges,
		removedEdges: topology.removedEdges,
		changedFields: changedTopLevelFields(before, after),
	};
}

// ── locks ────────────────────────────────────────────────────────

/** The current lease holder; null means unlocked. */
export async function getDefinitionLock(
	kind: DefinitionKind,
	id: string,
): Promise<DefinitionLock | null> {
	const data =
		kind === 'workflow'
			? await call<LockView | null>(
					client.GET('/api/v1/workflows/{id}/lock', {
						params: { path: { id } },
					}),
				)
			: await call<LockView | null>(
					client.GET('/api/v1/agents/{id}/lock', {
						params: { path: { id } },
					}),
				);
	return toLock(data);
}

/** Claim the lease for an owner; returns the active holder. */
export async function acquireDefinitionLock(
	kind: DefinitionKind,
	id: string,
	owner: { ownerId: string; ownerName: string },
): Promise<DefinitionLock> {
	const data =
		kind === 'workflow'
			? await call<LockView | null>(
					client.POST('/api/v1/workflows/{id}/lock/acquire', {
						params: { path: { id } },
						body: { owner_id: owner.ownerId, owner_name: owner.ownerName },
					}),
				)
			: await call<LockView | null>(
					client.POST('/api/v1/agents/{id}/lock/acquire', {
						params: { path: { id } },
						body: { owner_id: owner.ownerId, owner_name: owner.ownerName },
					}),
				);
	const lock = toLock(data);
	if (!lock) throw new Error('Lock acquisition returned no holder');
	return lock;
}

/** Renew the lease while editing. */
export async function heartbeatDefinitionLock(
	kind: DefinitionKind,
	id: string,
	ownerId: string,
): Promise<DefinitionLock> {
	const data =
		kind === 'workflow'
			? await call<LockView | null>(
					client.POST('/api/v1/workflows/{id}/lock/heartbeat', {
						params: { path: { id } },
						body: { owner_id: ownerId },
					}),
				)
			: await call<LockView | null>(
					client.POST('/api/v1/agents/{id}/lock/heartbeat', {
						params: { path: { id } },
						body: { owner_id: ownerId },
					}),
				);
	const lock = toLock(data);
	if (!lock) throw new Error('Lock heartbeat returned no holder');
	return lock;
}

/** Release the lease; no-op when held by someone else. */
export async function releaseDefinitionLock(
	kind: DefinitionKind,
	id: string,
	ownerId: string,
): Promise<void> {
	if (kind === 'workflow') {
		await call<LockView | null>(
			client.POST('/api/v1/workflows/{id}/lock/release', {
				params: { path: { id } },
				body: { owner_id: ownerId },
			}),
		);
		return;
	}
	await call<LockView | null>(
		client.POST('/api/v1/agents/{id}/lock/release', {
			params: { path: { id } },
			body: { owner_id: ownerId },
		}),
	);
}
