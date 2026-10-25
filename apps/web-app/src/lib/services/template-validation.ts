import { client } from '#lib/api/client';
import { call } from '#lib/api/envelope';
import { issueTargetsNode } from '#lib/graph/execution-projection';
import type { DisplayNode } from '#lib/graph/display-model';
import type { TemplateKind } from '#lib/types/models';

/**
 * Unified validation issue shared by the template drawer, the workflow
 * editor and the tool dialog. Local issues come from client field checks,
 * server issues from rule endpoints; both render in one list and locate
 * through the same field and node targeting.
 */
export interface TemplateIssue {
	source: 'local' | 'server';
	field: string | null;
	message: string;
	nodeId: string | null;
}

interface FieldCheck {
	field: string;
	message: string;
}

function missingFieldChecks(
	kind: TemplateKind,
	record: Record<string, unknown>,
): FieldCheck[] {
	const checks: FieldCheck[] = [];
	if (typeof record.name !== 'string' || !record.name.trim()) {
		checks.push({ field: 'name', message: 'name: required' });
	}
	if (
		kind === 'trigger' &&
		(typeof record.trigger_type !== 'string' || !record.trigger_type.trim())
	) {
		checks.push({ field: 'trigger_type', message: 'trigger_type: required' });
	}
	if (
		kind === 'node' &&
		(typeof record.node_type !== 'string' || !record.node_type.trim())
	) {
		checks.push({ field: 'node_type', message: 'node_type: required' });
	}
	return checks;
}

function asLocalIssues(checks: FieldCheck[]): TemplateIssue[] {
	return checks.map((check) => ({
		source: 'local' as const,
		field: check.field,
		message: check.message,
		nodeId: null,
	}));
}

/**
 * Synchronous local checks for live editor hints. Workflow and agent kinds
 * only validate through the server gate, so they report nothing here.
 */
export function localTemplateIssues(
	kind: TemplateKind,
	definition: unknown,
): TemplateIssue[] {
	if (kind === 'workflow' || kind === 'agent') return [];
	const record =
		definition && typeof definition === 'object'
			? (definition as Record<string, unknown>)
			: null;
	if (!record) {
		return [
			{
				source: 'local',
				field: null,
				message: 'Definition must be a JSON object',
				nodeId: null,
			},
		];
	}
	return asLocalIssues(missingFieldChecks(kind, record));
}

/**
 * Map server rule issues to the unified model, resolving the first canvas
 * node whose id appears as a field-path segment. Unmatched issues stay
 * global with a null node id.
 */
export function serverTemplateIssues(
	issues: Array<{ field: string; message: string }>,
	nodes: DisplayNode[],
): TemplateIssue[] {
	return issues.map((issue) => ({
		source: 'server' as const,
		field: issue.field,
		message: issue.message,
		nodeId:
			nodes.find((node) => issueTargetsNode(issue.field, node.id))?.id ?? null,
	}));
}

/**
 * Server-side validation for an edited definition. Returns unified issues;
 * empty means the definition validates. Every kind validates through a
 * server dry-run endpoint before save; the server owns the final check on
 * save.
 *
 * Validation layers, shared by every definition kind:
 * - instant hint: synchronous `localTemplateIssues` while typing;
 * - compile gate: a server endpoint before save (`requiresServerCompileGate`);
 * - save: the server owns the final check, failures surface without rollback.
 */
export async function validateTemplateDefinition(
	kind: TemplateKind,
	definition: unknown,
): Promise<TemplateIssue[]> {
	if (kind === 'workflow') {
		try {
			await call<unknown>(
				client.POST('/api/v1/workflows/validate', { body: definition }),
			);
			return [];
		} catch (e) {
			return [
				{
					source: 'server',
					field: null,
					message: e instanceof Error ? e.message : 'Workflow invalid',
					nodeId: null,
				},
			];
		}
	}
	if (kind === 'agent') {
		try {
			await call<unknown>(
				client.POST('/api/v1/agents/validate', { body: definition }),
			);
			return [];
		} catch (e) {
			return [
				{
					source: 'server',
					field: null,
					message: e instanceof Error ? e.message : 'Agent invalid',
					nodeId: null,
				},
			];
		}
	}
	if (kind === 'node') {
		try {
			await call<unknown>(
				client.POST('/api/v1/templates/node/validate', {
					body: definition,
				}),
			);
			return [];
		} catch (e) {
			return [
				{
					source: 'server',
					field: null,
					message: e instanceof Error ? e.message : 'Node template invalid',
					nodeId: null,
				},
			];
		}
	}
	try {
		await call<unknown>(
			client.POST('/api/v1/templates/trigger/validate', {
				body: definition,
			}),
		);
		return [];
	} catch (e) {
		return [
			{
				source: 'server',
				field: null,
				message: e instanceof Error ? e.message : 'Trigger template invalid',
				nodeId: null,
			},
		];
	}
}

/**
 * Unified definition editing model, shared by the template library and the
 * workflow detail page. A template is the reusable view of a definition; a
 * workflow is the runnable instance of the same definition shape. Every
 * kind goes through lock, draft, validate and promote against the same
 * endpoint shape; an unknown kind is an error and never guessed.
 */

/** Whether saving this kind must pass a server compile gate first. */
export function requiresServerCompileGate(kind: TemplateKind): boolean {
	void kind;
	return true;
}

/** Whether this kind has a single-writer editing lock on the backend. */
export function supportsDefinitionLock(kind: TemplateKind): boolean {
	void kind;
	return true;
}

/**
 * Where the server check for this kind happens: every kind validates
 * through a dry-run endpoint before save. Frontend field checks are
 * instant hints only.
 */
export function describeValidationGate(kind: TemplateKind): string {
	if (kind === 'workflow')
		return 'local hint + /workflows/validate + draft compile gate, final check on save';
	if (kind === 'agent')
		return 'local hint + /agents/validate compile gate, final check on save';
	if (kind === 'node')
		return 'local hint + /templates/node/validate compile gate, final check on save';
	return 'local hint + /templates/trigger/validate compile gate, final check on save';
}
