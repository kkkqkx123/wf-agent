/** Mock routes: the definition capabilities shared by every editable kind.
 *
 * Drafts, versions and edit locks exist for workflows and agents alike.
 * The mock keeps the endpoint surface identical to the backend so the
 * shared service finds the same routes in preview and in production.
 */

import { FIXTURE_EPOCH } from '#lib/fixtures/clock';
import { camelToSnakeDeep, type MockRouter } from '../router';

const WORKFLOW_DRAFT = {
	id: 'wf-release-train',
	name: 'Release pipeline (draft)',
	description: 'Builds, signs and publishes a release bundle.',
	version: '3.1.0',
	nodes: [
		{ id: 'build', name: 'Build', node_type: 'script' },
		{ id: 'sign', name: 'Sign', node_type: 'script' },
	],
	edges: [{ source_node_id: 'build', target_node_id: 'sign', edge_type: 'default' }],
	updated_at: FIXTURE_EPOCH,
};

const AGENT_DRAFT = {
	id: 'tpl-6',
	name: 'Triage agent (draft)',
	description: 'Reads an incident and proposes the next action.',
	version: '0.4.0',
	config: { system_prompt_template_id: 'incident-triage' },
	updated_at: FIXTURE_EPOCH,
};

const AGENT_VERSIONS = [
	{
		id: 'triage-agent',
		name: 'Triage agent',
		description: 'Reads an incident and proposes the next action.',
		definition: {
			id: 'triage-agent',
			name: 'Triage agent',
			description: 'Reads an incident and proposes the next action.',
			version: '0.4.0',
			config: { system_prompt_template_id: 'incident-triage' },
			created_at: FIXTURE_EPOCH,
			updated_at: FIXTURE_EPOCH,
		},
	},
	{
		id: 'triage-agent',
		name: 'Triage agent',
		description: 'Reads an incident and proposes the next action.',
		definition: {
			id: 'triage-agent',
			name: 'Triage agent',
			description: 'Reads an incident and proposes the next action.',
			version: '0.3.0',
			config: { system_prompt_template_id: 'incident-triage' },
			created_at: FIXTURE_EPOCH,
			updated_at: FIXTURE_EPOCH,
		},
	},
];

function lockHolder(body: unknown): {
	owner_id: string;
	owner_name: string;
	expires_at: number;
} {
	const row = (body ?? {}) as { owner_id?: unknown; owner_name?: unknown };
	return {
		owner_id:
			typeof row.owner_id === 'string' && row.owner_id ? row.owner_id : 'preview-owner',
		owner_name:
			typeof row.owner_name === 'string' && row.owner_name
				? row.owner_name
				: 'Preview Owner',
		expires_at: FIXTURE_EPOCH,
	};
}

export function registerDefinitions(router: MockRouter): void {
	// ---------- workflow drafts ----------
	// A definition id narrows the list to the drafts of that definition: the
	// draft filed under the definition itself and the one the canvas editor
	// writes for it.
	router.on('GET', '/api/v1/workflows/drafts', (_p, query) => {
		const definitionId = query.definition_id;
		if (typeof definitionId !== 'string' || definitionId === '') {
			return [WORKFLOW_DRAFT];
		}
		return [WORKFLOW_DRAFT].filter(
			(draft) =>
				draft.id === definitionId ||
				draft.id === `${definitionId}-canvas`,
		);
	});
	router.on('GET', '/api/v1/workflows/drafts/{id}', () => WORKFLOW_DRAFT);
	router.on('GET', '/api/v1/workflows/drafts/{id}/validate', () => []);
	router.on('POST', '/api/v1/workflows/drafts', (_p, _q, body) => {
		const row = (body ?? {}) as { id?: unknown };
		return typeof row.id === 'string' ? row.id : WORKFLOW_DRAFT.id;
	});
	router.on('POST', '/api/v1/workflows/drafts/{id}/promote', () => ({
		resource_kind: 'sub_workflow',
		resource_id: WORKFLOW_DRAFT.id,
		dependents: [],
		error_count: 0,
		warning_count: 0,
		pass_count: 0,
	}));
	router.on('DELETE', '/api/v1/workflows/drafts/{id}', () => true);
	router.on('POST', '/api/v1/workflows/{id}/rollback', () => ({}));

	// ---------- agent drafts ----------
	router.on('GET', '/api/v1/agents/drafts', (_p, query) => {
		const definitionId = query.definition_id;
		if (typeof definitionId !== 'string' || definitionId === '') {
			return [AGENT_DRAFT];
		}
		return [AGENT_DRAFT].filter((draft) => draft.id === definitionId);
	});
	router.on('GET', '/api/v1/agents/drafts/{id}', () => AGENT_DRAFT);
	router.on('GET', '/api/v1/agents/drafts/{id}/validate', () => []);
	router.on('POST', '/api/v1/agents/drafts', (_p, _q, body) => {
		const row = (body ?? {}) as { id?: unknown };
		return typeof row.id === 'string' ? row.id : AGENT_DRAFT.id;
	});
	router.on('POST', '/api/v1/agents/drafts/{id}/promote', () => []);
	router.on('DELETE', '/api/v1/agents/drafts/{id}', () => true);

	// ---------- agent versions ----------
	router.on('GET', '/api/v1/templates/library/agents/{id}/versions', () =>
		camelToSnakeDeep(AGENT_VERSIONS),
	);
	router.on('GET', '/api/v1/templates/library/agents/{id}/versions/{version}', () =>
		camelToSnakeDeep(AGENT_VERSIONS[0].definition),
	);
	router.on('POST', '/api/v1/templates/library/agents/{id}/rollback', () => ({}));

	// ---------- agent edit lock ----------
	// Preview has no concurrent editors, so the lease is always free until
	// this session takes it.
	router.on('GET', '/api/v1/agents/{id}/lock', () => null);
	router.on('POST', '/api/v1/agents/{id}/lock/acquire', (_p, _q, body) => lockHolder(body));
	router.on('POST', '/api/v1/agents/{id}/lock/heartbeat', (_p, _q, body) => lockHolder(body));
	router.on('POST', '/api/v1/agents/{id}/lock/release', () => null);
}
