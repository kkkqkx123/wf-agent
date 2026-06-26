import type { CheckpointDependencyGraph, CheckpointStorageMetadata } from "@wf-agent/types";

export interface CheckpointInfoInput {
  id: string;
  metadata: CheckpointStorageMetadata;
}

export function buildDependencyGraph(checkpoints: CheckpointInfoInput[]): CheckpointDependencyGraph {
  const referencedBy = new Map<string, string[]>();
  const chainRootMap = new Map<string, string>();
  const chainGroups = new Map<string, string[]>();

  for (const cp of checkpoints) {
    const prevId = cp.metadata.previousCheckpointId;
    if (prevId && prevId !== cp.id) {
      const refs = referencedBy.get(prevId) || [];
      refs.push(cp.id);
      referencedBy.set(prevId, refs);
    }

    const chainRootId = cp.metadata.chainRootId || cp.id;
    chainRootMap.set(cp.id, chainRootId);

    const group = chainGroups.get(chainRootId) || [];
    group.push(cp.id);
    chainGroups.set(chainRootId, group);
  }

  return { referencedBy, chainRootMap, chainGroups };
}

export function computeProtectedCheckpoints(
  candidateIds: Set<string>,
  graph: CheckpointDependencyGraph,
  allCheckpointIds: Set<string>,
): Set<string> {
  const protectedSet = new Set<string>();

  const previousMap = new Map<string, string>();
  for (const [prevId, refs] of graph.referencedBy) {
    for (const ref of refs) {
      previousMap.set(ref, prevId);
    }
  }

  const survivingIds = [...allCheckpointIds].filter(id => !candidateIds.has(id));

  for (const survivingId of survivingIds) {
    let currentId: string | undefined = survivingId;
    const chainRoot = graph.chainRootMap.get(survivingId);

    while (currentId && currentId !== chainRoot) {
      if (candidateIds.has(currentId)) {
        protectedSet.add(currentId);
      }
      currentId = previousMap.get(currentId);
    }

    if (chainRoot && candidateIds.has(chainRoot)) {
      protectedSet.add(chainRoot);
    }
  }

  return protectedSet;
}
