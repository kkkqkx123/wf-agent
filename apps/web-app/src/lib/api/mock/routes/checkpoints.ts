/** Mock routes: the execution checkpoint chain and its stats / metadata. */

import { checkpoints } from '#lib/fixtures/checkpoints';
import {
	addMockCheckpoint,
	emitMockEvent,
	removeMockCheckpoint,
	removeMockCheckpointsBefore,
	removeMockCheckpointsByEntity,
	setMockCheckpointTags,
} from '#lib/fixtures/checkpoints';
import { pageView, type MockRouter } from '../router';
import { toCheckpointWire } from '../wire';

export function registerCheckpoints(router: MockRouter): void {
	// The fixtures hold view-model rows; the services layer parses wire
	// `CheckpointDoc` fields (entity_id / checkpoint_type / timestamp / …), so
	// the mock serves the wire shape and keeps the mapping in one place.
	const checkpointsWire = () => checkpoints.map(toCheckpointWire);

	router.on('GET', '/api/v1/checkpoints', (_p, query) => {
		return pageView(checkpointsWire(), +(query.limit ?? 50), +(query.offset ?? 0));
	});
	router.on('GET', '/api/v1/checkpoints/entity/{entityId}', (params) => {
		const found = checkpoints.filter((c) => c.executionId === params.entityId);
		return (found.length > 0 ? found : checkpoints).map(toCheckpointWire);
	});
	router.on('GET', '/api/v1/checkpoints/time-range', (params, query) => {
		const from = query.from != null ? Number(query.from) : null;
		const to = query.to != null ? Number(query.to) : null;
		const filtered = checkpoints.filter((c) => {
			const ts = Date.parse(c.createdAt);
			return (from === null || ts >= from) && (to === null || ts <= to);
		});
		return pageView(filtered.map(toCheckpointWire), +(query.limit ?? 50), +(query.offset ?? 0));
	});
	router.on('GET', '/api/v1/checkpoints/entities', (params, query) => {
		const ids = String(query.entity_ids ?? '')
			.split(',')
			.map((id) => id.trim())
			.filter((id) => id.length > 0);
		const filtered = ids.length === 0 ? checkpoints : checkpoints.filter((c) => ids.includes(c.executionId));
		return pageView(filtered.map(toCheckpointWire), +(query.limit ?? 50), +(query.offset ?? 0));
	});
	router.on('GET', '/api/v1/checkpoints/entity/{entityId}/metadata', () => ({
		metadata: { preview: true, notes: 'Fixture metadata for the preview store' },
	}));
	router.on('PUT', '/api/v1/checkpoints/entity/{entityId}/metadata', () => ({ ok: true }));
	router.on('GET', '/api/v1/checkpoints/stats', () => {
		return { total: checkpoints.length, restorable: checkpoints.filter((c) => c.restorable).length };
	});
	router.on('GET', '/api/v1/agent-checkpoints/stats', () => {
		const byType: Record<string, number> = {};
		for (const c of checkpoints) byType[c.kind] = (byType[c.kind] ?? 0) + 1;
		return {
			total: checkpoints.length,
			restorable: checkpoints.filter((c) => c.restorable).length,
			by_type: byType,
		};
	});
	router.on('GET', '/api/v1/executions/checkpoints/{cid}/preview', (params) => ({
		checkpoint_id: params.cid,
		execution_id: checkpoints.find((c) => c.id === params.cid)?.executionId ?? 'exec-7f31c2a9',
		status: 'running',
		current_node_id: 'build-bundle',
		variable_keys: ['inputs', 'artifact'],
		node_result_keys: ['build', 'tests'],
	}));
	router.on('POST', '/api/v1/executions/checkpoints/{cid}/restore', (params) => {
		emitMockEvent('CHECKPOINT_RESTORED', { checkpoint_id: params.cid });
		return { ok: true };
	});
	router.on('POST', '/api/v1/executions/checkpoints/{cid}/resume', () => ({ ok: true }));
	router.on('DELETE', '/api/v1/checkpoints/{id}', (params) => {
		removeMockCheckpoint(params.id);
		return { ok: true };
	});
	router.on('PUT', '/api/v1/checkpoints/{id}/tags', (params, _q, body) => {
		setMockCheckpointTags(params.id, Array.isArray(body) ? body.map(String) : []);
		return { ok: true };
	});
	router.on('DELETE', '/api/v1/checkpoints/entity/{entityId}', (params) => {
		return { data: removeMockCheckpointsByEntity(params.entityId) };
	});
	router.on('POST', '/api/v1/executions/{id}/checkpoints', (params) => {
		const row = addMockCheckpoint({
			executionId: params.id,
			kind: 'state',
			actor: 'system',
			note: 'Manual execution checkpoint (preview)',
		});
		return row.id;
	});
	router.on('POST', '/api/v1/executions/{id}/checkpoints/gc', (params, query) => {
		const before = query.before != null ? Number(query.before) : Date.now();
		void params;
		return removeMockCheckpointsBefore(before);
	});
}
