/**
 * Template service facade.
 *
 * The template domain is split by responsibility; this module only
 * re-exports the public surface so existing routes and panels keep one
 * import path:
 * - `template-coordinate`: unified definition coordinate (browse/detail
 *   lifecycle entry, split-first contract).
 * - `template-library`: browse, detail lifecycle, publish, derive, clone,
 *   import/export.
 * - `template-validation`: local hints, server dry-run gates, lock gates.
 * - `template-forms`: form/JSON dual editing.
 * - `template-topology`: topology parse/merge, backend definition shape.
 * - `template-session`: non-reactive session document (single source of
 *   truth for the template editor).
 */

export {
	describeCoordinate,
	isLibraryCoordinateKind,
	templateRouteFor,
	type DefinitionCoordinate,
} from '#lib/services/template-coordinate';
export {
	DEFAULT_AGENT_ID,
	cloneTemplate,
	deleteTemplate,
	exportTemplate,
	getTemplateDetail,
	importTemplate,
	isHttpError,
	isRunnableTemplate,
	listFeaturedTemplates,
	listTemplates,
	publishWorkflowTemplate,
	recordTemplateUsage,
	saveTemplate,
	unpublishWorkflowTemplate,
	type TemplateDetail,
} from '#lib/services/template-library';
export {
	describeValidationGate,
	localTemplateIssues,
	requiresServerCompileGate,
	serverTemplateIssues,
	supportsDefinitionLock,
	validateTemplateDefinition,
	type TemplateIssue,
} from '#lib/services/template-validation';
export {
	formFromDefinition,
	formToDefinition,
	jsonErrorLine,
	jsonFieldLine,
	templateFormFields,
	type TemplateFormField,
} from '#lib/services/template-forms';
export {
	allocateGraphNodeId,
	mergeTemplateGraph,
	parseTemplateTopology,
	skeletonNodeHints,
	summarizeTemplate,
	templateBackendDefinition,
	templateEditTarget,
	type TemplateTopology,
} from '#lib/services/template-topology';
export { TemplateEditSession } from '#lib/services/template-session';
