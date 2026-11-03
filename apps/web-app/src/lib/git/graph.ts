export interface GraphInput {
	id: string;
	parents: string[];
}

export interface GraphRow {
	id: string;
	lane: number;
	laneCount: number;
	active: boolean[];
}

/**
 * Greedy lane assignment for the commit graph column.
 *
 * The server list order is kept as-is; clock skew is treated as an
 * approximate topology and no strict sort is applied here. Pagination
 * windows are assigned independently, so lanes at a window edge may not
 * join up with the next page. Callers render the result as window-local.
 */
export function assignGraphLanes(commits: GraphInput[]): GraphRow[] {
	const active: Array<string | null> = [];
	const rows: GraphRow[] = [];
	for (const commit of commits) {
		const parents = commit.parents ?? [];
		let lane = active.indexOf(commit.id);
		if (lane < 0) {
			const free = active.indexOf(null);
			if (free >= 0) {
				lane = free;
				active[lane] = commit.id;
			} else {
				lane = active.length;
				active.push(commit.id);
			}
		}
		for (let i = 0; i < active.length; i += 1) {
			if (i !== lane && active[i] === commit.id) active[i] = null;
		}
		if (parents.length === 0) {
			active[lane] = null;
		} else {
			active[lane] = parents[0];
			for (const extra of parents.slice(1)) {
				if (active.includes(extra)) continue;
				const free = active.indexOf(null);
				if (free >= 0) active[free] = extra;
				else active.push(extra);
			}
		}
		while (active.length > 0 && active[active.length - 1] == null) active.pop();
		const laneCount = Math.max(active.length, lane + 1);
		const flags: boolean[] = Array.from({ length: laneCount }, (_, i) => active[i] != null);
		flags[lane] = true;
		rows.push({ id: commit.id, lane, laneCount, active: flags });
	}
	return rows;
}
