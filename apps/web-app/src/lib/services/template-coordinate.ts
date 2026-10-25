import type { TemplateKind } from '#lib/types/models';

/**
 * Unified definition coordinate shared by every template service module.
 *
 * A template is the reusable view of a definition; a workflow is the runnable
 * instance of the same definition shape. Both address a definition by the
 * same coordinate, so browse, lifecycle, validation, forms, topology and
 * session modules never branch on kind-specific paths on their own.
 */
export interface DefinitionCoordinate {
	kind: TemplateKind;
	id: string;
}

/** Library kinds live under the template library; node/trigger have own routes. */
export function isLibraryCoordinateKind(kind: TemplateKind): boolean {
	return kind === 'agent' || kind === 'workflow';
}

/** Route to the reusable template view for a coordinate. */
export function templateRouteFor(coordinate: DefinitionCoordinate): string {
	return `/templates/${coordinate.kind}/${encodeURIComponent(coordinate.id)}`;
}

/** Route to the runnable workflow instance derived from a template. */
export function workflowRouteFor(workflowId: string): string {
	return `/workflows/${encodeURIComponent(workflowId)}`;
}

/** Human label for a coordinate, for toasts and diagnostics. */
export function describeCoordinate(coordinate: DefinitionCoordinate): string {
	return `${coordinate.kind}:${coordinate.id}`;
}
