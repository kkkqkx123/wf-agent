/**
 * View-model → wire adapters for fixture data the services layer parses
 * as backend `Doc` shapes. Keeping the mapping here means each route
 * module serves the wire contract without re-deriving it.
 */

import type { Checkpoint } from '#lib/types/models';

/**
 * Adapt a fixture checkpoint (view model) onto the wire `CheckpointDoc`
 * the services layer parses (entity_id / checkpoint_type / timestamp / …).
 */
export function toCheckpointWire(c: Checkpoint) {
	return {
		id: c.id,
		entity_id: c.executionId,
		entity_type: c.executionId.startsWith('loop-') ? 'agent_loop' : 'execution',
		checkpoint_type: c.kind,
		timestamp: Date.parse(c.createdAt),
		blob_size: c.sizeBytes,
		chain_position: c.sequence,
		status: c.restorable ? 'active' : 'corrupted',
		tags: c.tags ?? [],
	};
}
