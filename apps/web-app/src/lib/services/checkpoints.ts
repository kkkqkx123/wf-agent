import { client } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type { Checkpoint } from '$lib/types/models';

interface CheckpointDto {
    id?: string;
    execution_id?: string;
    sequence?: number;
    kind?: string;
    actor?: string;
    created_at?: string;
    size_bytes?: number;
    size?: number;
    note?: string;
    restorable?: boolean;
}

function toCheckpoint(d: CheckpointDto): Checkpoint {
    return {
        id: d.id ?? '',
        executionId: d.execution_id ?? '',
        sequence: d.sequence ?? 0,
        kind: d.kind ?? 'state',
        actor: d.actor ?? '',
        createdAt: d.created_at ?? '',
        sizeBytes: d.size_bytes ?? d.size ?? 0,
        note: d.note ?? '',
        restorable: d.restorable ?? true,
    };
}

export async function listCheckpoints(params?: {
    limit?: number;
    offset?: number;
}): Promise<PageResult<Checkpoint>> {
    const data = await call<unknown>(
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        client.GET('/api/v1/checkpoints' as any, {
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            params: { query: params ?? {} } as any,
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
        } as any),
    );
    const page = extractPage<CheckpointDto>(data);
    return { ...page, items: page.items.map(toCheckpoint) };
}

export async function listCheckpointsByEntity(entityId: string): Promise<Checkpoint[]> {
    const data = await call<unknown>(
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        client.GET('/api/v1/checkpoints/entity/{entityId}' as any, {
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            params: { path: { entityId } } as any,
	// eslint-disable-next-line @typescript-eslint/no-explicit-any
        } as any),
    );
    if (Array.isArray(data)) {
        return (data as CheckpointDto[]).map(toCheckpoint);
    }
    const page = extractPage<CheckpointDto>(data);
    return page.items.map(toCheckpoint);
}
