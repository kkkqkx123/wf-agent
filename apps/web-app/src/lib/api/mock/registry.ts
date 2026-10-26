/**
 * Route registry — wires every domain module onto one router.
 *
 * Registration order is significant: the router dispatches to the first
 * pattern that matches, so overlapping templates (e.g. `/agent-loops/
 * summaries` before `/agent-loops/{id}`) must keep their relative order.
 * The sequence below mirrors the original single-file layout.
 */

import type { MockRouter } from './router';
import { registerDefinitions } from './routes/definitions';
import { registerWorkflows } from './routes/workflows';
import { registerExecutions } from './routes/executions';
import { registerAgentLoops } from './routes/agent-loops';
import { registerCheckpoints } from './routes/checkpoints';
import { registerInsights } from './routes/insights';
import { registerResources } from './routes/resources';
import { registerTemplates } from './routes/templates';
import { registerTriggers } from './routes/triggers';
import { registerFileCheckpoint } from './routes/file-checkpoint';
import { registerGit } from './routes/git';
import { registerPty } from './routes/pty';
import { registerFs } from './routes/fs';

export function registerAllRoutes(router: MockRouter): void {
	// Definition routes come first: their literal paths (`/workflows/drafts`,
	// `/agents/drafts`) would otherwise be captured by the `{id}` patterns.
	registerDefinitions(router);
	registerWorkflows(router);
	registerExecutions(router);
	registerAgentLoops(router);
	registerCheckpoints(router);
	registerInsights(router);
	registerResources(router);
	registerTemplates(router);
	registerTriggers(router);
	registerFileCheckpoint(router);
	registerGit(router);
	registerPty(router);
	registerFs(router);
}
