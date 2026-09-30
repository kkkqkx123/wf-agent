import type { DisplayEdge } from './display-model';
import { connectByPort } from './display-model';
import { isGroupTitleId } from './group-view';

export interface ConnectCheck {
	editMode: boolean;
	source: string;
	target: string;
	edges: DisplayEdge[];
	hiddenIds: Set<string>;
	/** Backend kind of the source node; enables port-type validation. */
	sourceKind?: string;
	/** Backend kind of the target node; enables port-type validation. */
	targetKind?: string;
}

/** Human-readable reason a connect attempt is rejected, or null when valid. */
export function describeConnectRejection(check: ConnectCheck): string | null {
	const { editMode, source, target, edges, hiddenIds } = check;
	if (!editMode) return 'Read-only canvas. Enter edit mode to connect.';
	if (!source || !target) return 'Choose another node to connect.';
	if (source === target) return 'Cannot self-connect.';
	if (
		target.startsWith('groupbox:') ||
		source.startsWith('groupbox:') ||
		isGroupTitleId(source) ||
		isGroupTitleId(target)
	) {
		return 'Groups cannot connect directly. Expand the group first.';
	}
	if (hiddenIds.has(source) || hiddenIds.has(target)) {
		return 'Hidden group members cannot connect. Expand the group first.';
	}
	if (edges.some((edge) => edge.source === source && edge.target === target)) {
		return 'Edge already exists.';
	}
	const portReason = connectByPort({
		sourceKind: check.sourceKind ?? '',
		targetKind: check.targetKind ?? '',
	});
	if (portReason) return portReason;
	return null;
}

export function isConnectValid(check: ConnectCheck): boolean {
	return describeConnectRejection(check) === null;
}
