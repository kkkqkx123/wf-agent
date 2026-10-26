import type { TemplateKind } from '#lib/types/models';

/**
 * Unified definition coordinate shared by every template service module.
 *
 * All four kinds are managed in the template library; there is no separate
 * workflow page. A workflow is one definition kind among node, trigger,
 * agent and workflow.
 */
export interface DefinitionCoordinate {
	kind: TemplateKind;
	id: string;
}

/** Library kinds live under the template library; node/trigger have own routes. */
export function isLibraryCoordinateKind(kind: TemplateKind): boolean {
	return kind === 'agent' || kind === 'workflow';
}

/** Route to the definition view for a coordinate. */
export function templateRouteFor(coordinate: DefinitionCoordinate): string {
	return `/templates/${coordinate.kind}/${encodeURIComponent(coordinate.id)}`;
}

/** Human label for a coordinate, for toasts and diagnostics. */
export function describeCoordinate(coordinate: DefinitionCoordinate): string {
	return `${coordinate.kind}:${coordinate.id}`;
}
