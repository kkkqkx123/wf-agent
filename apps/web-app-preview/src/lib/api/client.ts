/**
 * Fixture-backed replacement for openapi-fetch's client.
 *
 * web-app-preview never talks to a real wf-server. Every call to
 * `client.GET` / `client.POST` below resolves instantly against the
 * hard-coded sample data in `$lib/fixtures`. The return shape matches
 * openapi-fetch ({ data, error, response }) so `envelope.ts` and the
 * services layer stay completely unchanged.
 *
 * This file is a "preview-only override": sync-frontend-preview.sh copies
 * it *once* from the source tree, then excludes it from future syncs so
 * edits here never get overwritten.
 */

import {
        agentLoops,
        loopMessages,
        loopVariables,
        loopDetail,
} from '$lib/fixtures/agentLoops';
import { checkpoints, fileChanges, approvals } from '$lib/fixtures/checkpoints';
import { executions, executionDetail, executionToolCalls } from '$lib/fixtures/executions';
import {
        overviewMetrics,
        templates,
        queryResult,
        auditReports,
        errorAnalyses,
        perfNodes,
        events,
        dependencies,
        diagnostics,
} from '$lib/fixtures/insights';
import {
        modelProfiles,
        providers,
        tools,
        scripts,
        skills,
} from '$lib/fixtures/resources';
import {
        triggerRecords,
        hooks,
        executionTimeline,
} from '$lib/fixtures/triggers';
import { workflows, workflowDetail } from '$lib/fixtures/workflows';
import { FIXTURE_EPOCH } from '$lib/fixtures/clock';

// ---------------------------------------------------------------------------
// openapi-fetch type — preview never runs typecheck against the schema
// (we only need the runtime shape), so importing paths is intentionally
// skipped. Using `any` keeps preview independent from schema.d.ts rebuilds.
// ---------------------------------------------------------------------------
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyClient = any;

// Re-export an empty baseUrl so shared code that reads it still compiles.
export const API_BASE_URL = '/mock';

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

/** Convert one camelCase key to snake_case without double-underscores. */
function camelToSnake(s: string): string {
        // Only uppercase ASCII needs handling — fixture keys never have
        // leading underscores or unicode identifiers.
        return s.replace(/[A-Z]/g, (m) => '_' + m.toLowerCase());
}

/** Recursively convert camelCase object keys to snake_case. Arrays are
 *  recursed into; non-plain objects (dates, classes) are returned as-is. */
function camelToSnakeDeep<T>(value: T): T {
        if (value === null || value === undefined) return value;
        if (Array.isArray(value)) {
                // eslint-disable-next-line @typescript-eslint/no-explicit-any
                return value.map((v: any) => camelToSnakeDeep(v)) as T;
        }
        if (typeof value === 'object') {
                // Date, Map, Set etc. — preserve identity
                if (value instanceof Date) return value;
                const out: Record<string, unknown> = {};
                for (const [k, v] of Object.entries(value)) {
                        out[camelToSnake(k)] = camelToSnakeDeep(v);
                }
                return out as T;
        }
        return value;
}

/** Wrap a list of DTOs in the backend's PageView shape. */
function pageView<T>(items: T[], limit = 50, offset = 0) {
        return {
                items: items.slice(offset, offset + limit),
                has_more: offset + limit < items.length,
                limit,
                offset,
        };
}

/** Best-effort extract of a query param from params.query. openapi-fetch
 *  passes `{ query: { limit, offset } }` for list calls. */
function queryOf(params?: Record<string, unknown>): Record<string, unknown> {
        const q = params?.query;
        return q && typeof q === 'object' ? (q as Record<string, unknown>) : {};
}

/** Wrap a handler result the way the real backend envelope does, so the
 *  shared `envelope.ts` unwraps an identical shape in both projects. */
function okEnvelope(data: unknown) {
        return { data: { success: true, data, error: null } };
}

/** Unrouted path: surface a 404 the services layer turns into an
 *  `ApiHttpError`, so missing fixture coverage renders as an explicit
 *  error state instead of a silent empty view. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function missingRoute(method: string, path: string): any {
        return {
                error: {
                        success: false,
                        data: null,
                        error: {
                                code: 'NOT_FOUND',
                                message: `No mock route for ${method} ${path}`,
                        },
                },
                response: { status: 404 },
        };
}

// ---------------------------------------------------------------------------
// Path dispatch
// ---------------------------------------------------------------------------

/**
 * Match `pathTemplate` (e.g. `/api/v1/workflows/{id}`) against a concrete
 * request path, returning captured params or null on miss.
 */
function matchPath(
        pathTemplate: string,
        actual: string,
): Record<string, string> | null {
        const regex = new RegExp(
                '^' + pathTemplate.replace(/\{[^}]+\}/g, '([^/]+)') + '$',
        );
        const names = [...pathTemplate.matchAll(/\{([^}]+)\}/g)].map((m) => m[1]);
        const m = regex.exec(actual);
        if (!m) return null;
        const out: Record<string, string> = {};
        names.forEach((n, i) => (out[n] = m[i + 1]));
        return out;
}

type Handler = (
        params: Record<string, string>,
        query: Record<string, unknown>,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        body: any,
) => unknown;

/** Build a method router for GET/POST/PUT/DELETE. */
function makeRouter() {
        // Method → [ [pattern, handler], ... ]
        const routes: Record<string, Array<[string, Handler]>> = {
                GET: [],
                POST: [],
                PUT: [],
                DELETE: [],
                PATCH: [],
        };

        function on(method: string, pattern: string, handler: Handler) {
                routes[method].push([pattern, handler]);
        }

        function dispatch(
                method: string,
                path: string,
                query: Record<string, unknown>,
                body: unknown,
        ): unknown {
                for (const [pattern, handler] of routes[method] ?? []) {
                        const params = matchPath(pattern, path);
                        if (params !== null) {
                                return handler(params, query, body);
                        }
                }
                return undefined;
        }

        return { on, dispatch };
}

// ---------------------------------------------------------------------------
// Wire up every endpoint
// ---------------------------------------------------------------------------

const router = makeRouter();

// ---------- workflows ----------
router.on('GET', '/api/v1/workflows', (_p, query) => {
        return pageView(camelToSnakeDeep(workflows), +(query.limit ?? 50), +(query.offset ?? 0));
});
router.on('GET', '/api/v1/workflows/{id}', (params) => {
        const found = workflowDetail.id === params.id
                ? workflowDetail
                : workflows.find((w) => w.id === params.id) ?? workflows[0];
        return camelToSnakeDeep(found);
});
router.on('GET', '/api/v1/workflows/{id}/graph', (params) => {
        const detail = workflowDetail.id === params.id
                ? workflowDetail
                : workflows.find((w) => w.id === params.id)
                        ? workflowDetail
                        : workflowDetail;
        return camelToSnakeDeep(detail.graph);
});
router.on('GET', '/api/v1/workflows/{id}/versions', () => {
        return camelToSnakeDeep([
                { version: 12, created_at: '', author: 'platform', note: 'Nightly build bump', current: true },
                { version: 11, created_at: '', author: 'platform', note: 'Reorganise test matrix', current: false },
                { version: 10, created_at: '', author: 'sre', note: 'Tighten sandbox defaults', current: false },
        ]);
});
router.on('GET', '/api/v1/workflows/{id}/graph/nodes', () => {
        return camelToSnakeDeep(
                workflowDetail.graph.nodes.map((node) => ({
                        id: node.id,
                        name: node.label,
                        nodeType: node.kind,
                })),
        );
});
router.on('GET', '/api/v1/workflows/{id}/graph/edges', () => {
        return camelToSnakeDeep(
                workflowDetail.graph.edges.map((edge) => ({
                        id: edge.id,
                        sourceNodeId: edge.from,
                        targetNodeId: edge.to,
                        edgeType: 'default',
                        condition: edge.label ?? null,
                })),
        );
});
router.on('GET', '/api/v1/workflows/{id}/graph/summary', () => {
        const nodes = workflowDetail.graph.nodes;
        const edges = workflowDetail.graph.edges;
        const targets = new Set(edges.map((edge) => edge.to));
        const sources = new Set(edges.map((edge) => edge.from));
        const nodeCountsByType: Record<string, number> = {};
        for (const node of nodes) {
                nodeCountsByType[node.kind] = (nodeCountsByType[node.kind] ?? 0) + 1;
        }
        return camelToSnakeDeep({
                workflowId: workflowDetail.id,
                nodeCount: nodes.length,
                edgeCount: edges.length,
                startNodeId: nodes.find((node) => !targets.has(node.id))?.id ?? null,
                endNodeIds: nodes.filter((node) => !sources.has(node.id)).map((node) => node.id),
                nodeCountsByType,
        });
});
router.on('GET', '/api/v1/workflows/{id}/graph/neighbors/{nodeId}', () => {
        // Path params never reach the mock (preview calls carry the raw
        // template, not interpolated values), so serve the hub node
        // neighborhood as representative sample data.
        const edges = workflowDetail.graph.edges;
        const nodeId = 'build-bundle';
        return camelToSnakeDeep({
                nodeId,
                predecessors: edges.filter((e) => e.to === nodeId).map((e) => e.from),
                successors: edges.filter((e) => e.from === nodeId).map((e) => e.to),
        });
});

// ---------- executions ----------
router.on('GET', '/api/v1/executions', (_p, query) => {
        return pageView(camelToSnakeDeep(executions), +(query.limit ?? 50), +(query.offset ?? 0));
});
router.on('GET', '/api/v1/executions/{id}', (params) => {
        const found = executionDetail.id === params.id
                ? executionDetail
                : executions.find((e) => e.id === params.id) ?? executions[0];
        return camelToSnakeDeep(found);
});
router.on('GET', '/api/v1/executions/{id}/audit/tool-calls', () => {
        return camelToSnakeDeep(executionToolCalls);
});
router.on('GET', '/api/v1/executions/{id}/audit/timeline', () => {
        return camelToSnakeDeep(executionTimeline);
});
router.on('GET', '/api/v1/events/execution-timeline/{executionId}', () => {
        return camelToSnakeDeep(executionTimeline);
});

// ---------- agent-loops ----------

/** Backend SummaryDto shape the shared service maps via toLoop. */
function loopSummary(loop: (typeof agentLoops)[number]) {
        return {
                id: loop.id,
                status: loop.status,
                current_iteration: loop.iteration,
                tool_call_count: 0,
                start_time: Date.parse(loop.startedAt),
                end_time: Date.parse(loop.updatedAt),
                execution_time: null,
                profile_id: null,
        };
}
router.on('GET', '/api/v1/agent-loops/summaries', (_p, query) => {
        return pageView(
                agentLoops.map(loopSummary),
                +(query.limit ?? 50),
                +(query.offset ?? 0),
        );
});
router.on('GET', '/api/v1/agent-loops', (_p, query) => {
        return pageView(camelToSnakeDeep(agentLoops), +(query.limit ?? 50), +(query.offset ?? 0));
});
router.on('GET', '/api/v1/agent-loops/{id}', (params) => {
        const found = loopDetail.id === params.id
                ? loopDetail
                : agentLoops.find((a) => a.id === params.id) ?? agentLoops[0];
        return camelToSnakeDeep(found);
});
router.on('GET', '/api/v1/agent-loops/{id}/summary', (params) => {
        const found = loopDetail.id === params.id
                ? loopDetail
                : agentLoops.find((a) => a.id === params.id) ?? agentLoops[0];
        return loopSummary(found);
});
router.on('GET', '/api/v1/agent-loops/{id}/conversation', () => {
        return camelToSnakeDeep(loopMessages);
});
router.on('GET', '/api/v1/agent-loops/{id}/variables', () => {
        return {
                items: loopVariables.map((variable) => [variable.key, variable.value]),
                has_more: false,
                limit: 200,
                offset: 0,
        };
});
router.on('GET', '/api/v1/agent-loops/{id}/iteration-history', () => {
        return {
                items: loopDetail.iterations.map((iteration) => ({
                        iteration: iteration.index,
                        duration: iteration.durationMs ?? 0,
                        response_content: iteration.summary,
                        tool_calls: [],
                })),
                has_more: false,
                limit: 200,
                offset: 0,
        };
});
router.on('GET', '/api/v1/agent-loops/{id}/timeline', () => ({
        items: [],
        total: 0,
        truncated: false,
}));
router.on('GET', '/api/v1/agent-loops/{id}/checkpoints/chain', () => {
        return {
                items: camelToSnakeDeep(checkpoints),
                has_more: false,
                limit: 200,
                offset: 0,
        };
});
router.on('GET', '/api/v1/agent-loops/{id}/graph', () => {
        return camelToSnakeDeep(loopDetail.graph);
});
router.on('GET', '/api/v1/agent-loops/{id}/graph/nodes', () => {
        return loopDetail.graph.nodes.map((node, index) => ({
                node_id: node.id,
                description: node.label,
                type: node.kind,
                iteration: index + 1,
        }));
});
router.on('GET', '/api/v1/agent-loops/{id}/graph/edges', () => {
        return loopDetail.graph.edges.map((edge) => ({
                edge_id: edge.id,
                from_node_id: edge.from,
                to_node_id: edge.to,
                reason: edge.label ?? null,
        }));
});
router.on('GET', '/api/v1/agent-loops/{id}/graph/paths/steps', () => {
        const nodes = loopDetail.graph.nodes;
        return loopDetail.iterations.map((iteration, index) => ({
                step_no: iteration.index,
                node_id: nodes[index % nodes.length]?.id ?? '',
                node_type: nodes[index % nodes.length]?.kind ?? 'task',
                description: iteration.summary,
                iteration: iteration.index,
                timestamp: FIXTURE_EPOCH,
                duration: iteration.durationMs ?? 0,
        }));
});
router.on('GET', '/api/v1/agent-loops/{id}/graph/tool-frequency', () => {
        // Plain { tool: count } map, matching the backend contract the
        // shared service narrows structurally. Returned raw so tool-name
        // keys are never case-mangled.
        const counts: Record<string, number> = {};
        for (const entry of loopDetail.analysis.toolFrequency) {
                counts[entry.tool] = entry.count;
        }
        return counts;
});
router.on('GET', '/api/v1/agent-executions/{id}/errors/chain', () => {
        return camelToSnakeDeep(loopDetail.analysis.errorChain);
});
router.on('GET', '/api/v1/agent-executions/{id}/errors/root-cause', () => {
        return {
                root_cause_id: '',
                error: loopDetail.analysis.rootCause ?? '',
                chain_length: loopDetail.analysis.errorChain.length,
                suggested_action: loopDetail.analysis.recoveryHints[0] ?? null,
        };
});
router.on('POST', '/api/v1/agent-loops/{id}/pause', () => ({ ok: true }));
router.on('POST', '/api/v1/agent-loops/{id}/resume', () => ({ ok: true }));
router.on('POST', '/api/v1/agent-loops/{id}/cancel', () => ({ ok: true }));
router.on('POST', '/api/v1/agent-loops/{id}/checkpoints', () => ({ ok: true }));
router.on('POST', '/api/v1/agent-loops/{id}/checkpoints/{cid}/restore', () => ({ ok: true }));
router.on('POST', '/api/v1/agent-loops/{id}/run', () => ({
        agent_loop_id: 'loop-preview-run',
        result: 'Preview run accepted; connect a live backend to execute.',
        iterations: 1,
}));

// ---------- checkpoints ----------
router.on('GET', '/api/v1/checkpoints', (_p, query) => {
        return pageView(camelToSnakeDeep(checkpoints), +(query.limit ?? 50), +(query.offset ?? 0));
});
router.on('GET', '/api/v1/checkpoints/entity/{entityId}', () => {
        return camelToSnakeDeep(checkpoints);
});
router.on('GET', '/api/v1/checkpoints/stats', () => {
        return { total: checkpoints.length, restorable: checkpoints.filter((c) => c.restorable).length };
});
router.on('GET', '/api/v1/agent-checkpoints/stats', () => {
        return { total: checkpoints.length, restorable: checkpoints.filter((c) => c.restorable).length };
});
router.on('POST', '/api/v1/executions/checkpoints/{cid}/restore', () => ({ ok: true }));
router.on('POST', '/api/v1/executions/checkpoints/{cid}/resume', () => ({ ok: true }));

// ---------- events ----------
router.on('GET', '/api/v1/events', (_p, query) => {
        return pageView(camelToSnakeDeep(events), +(query.limit ?? 50), +(query.offset ?? 0));
});
router.on('GET', '/api/v1/events/search', (_p, query) => {
        return pageView(camelToSnakeDeep(events), +(query.limit ?? 50), +(query.offset ?? 0));
});
router.on('GET', '/api/v1/events/size', () => {
        return events.length;
});

// ---------- dependencies + health ----------
router.on('GET', '/api/v1/dependencies/audit', () => camelToSnakeDeep(dependencies));
router.on('GET', '/health', () => ({
        ready: true,
        storage: 'sqlite',
        persistence: { mode: 'single', db: 'wf.db' },
}));
router.on('GET', '/api/v1/storage/diagnose', () => camelToSnakeDeep(diagnostics));
router.on('GET', '/api/v1/storage/stats', () => ({
        files: 128,
        total_bytes: 4_120_000,
        oldest: new Date(FIXTURE_EPOCH - 86_400_000).toISOString(),
        newest: new Date(FIXTURE_EPOCH).toISOString(),
}));

// ---------- insights / analysis ----------
router.on('POST', '/api/v1/query', () => camelToSnakeDeep(queryResult));
router.on('GET', '/api/v1/analysis/stats/top-node-types', () => camelToSnakeDeep(perfNodes));
router.on('GET', '/api/v1/analysis/stats', () => ({
        executions_24h: overviewMetrics.find((m) => m.label === 'Completed 24h')?.value ?? 0,
        running: overviewMetrics.find((m) => m.label === 'Running')?.value ?? 0,
        failed_24h: overviewMetrics.find((m) => m.label === 'Failed 24h')?.value ?? 0,
}));

// ---------- resources ----------
router.on('GET', '/api/v1/llm/profiles', () => pageView(camelToSnakeDeep(modelProfiles)));
router.on('GET', '/api/v1/llm/providers', () => camelToSnakeDeep(providers));
router.on('GET', '/api/v1/tools', (_p, query) => pageView(camelToSnakeDeep(tools), +(query.limit ?? 50), +(query.offset ?? 0)));
router.on('GET', '/api/v1/scripts', (_p, query) => pageView(camelToSnakeDeep(scripts), +(query.limit ?? 50), +(query.offset ?? 0)));
router.on('GET', '/api/v1/skills', () => camelToSnakeDeep(skills));
router.on('POST', '/api/v1/skills/{name}/enable', () => ({ ok: true }));
router.on('POST', '/api/v1/skills/{name}/disable', () => ({ ok: true }));
router.on('POST', '/api/v1/tools/{id}/enable', () => ({ ok: true }));
router.on('POST', '/api/v1/tools/{id}/disable', () => ({ ok: true }));

// ---------- templates ----------
router.on('GET', '/api/v1/templates/library', (_p, query) => {
        void query;
        // The real endpoint answers with a bare array; usage counts use the
        // backend field name so the shared service maps them directly.
        return camelToSnakeDeep(
                templates.map((t) => ({ ...t, usage_count: t.usage })),
        );
});
router.on('GET', '/api/v1/templates/library/featured', () => {
        return camelToSnakeDeep(templates.filter((t) => t.featured));
});
router.on('GET', '/api/v1/templates/library/popular', () => {
        return camelToSnakeDeep([...templates].sort((a, b) => b.usage - a.usage).slice(0, 6));
});
router.on('GET', '/api/v1/templates/node', (_p, query) => {
        const nodeTemplates = templates.filter((t) => t.kind === 'node');
        return pageView(camelToSnakeDeep(nodeTemplates), +(query.limit ?? 50), +(query.offset ?? 0));
});
router.on('GET', '/api/v1/templates/trigger', (_p, query) => {
        const triggerTemplates = templates.filter((t) => t.kind === 'trigger');
        return pageView(camelToSnakeDeep(triggerTemplates), +(query.limit ?? 50), +(query.offset ?? 0));
});
// Detail routes: path params never reach the mock, so serve the first
// fixture of the requested registry as sample data.
router.on('GET', '/api/v1/templates/node/{id}', () => {
        return camelToSnakeDeep(templates.find((t) => t.kind === 'node') ?? undefined);
});
router.on('GET', '/api/v1/templates/trigger/{id}', () => {
        return camelToSnakeDeep(templates.find((t) => t.kind === 'trigger') ?? undefined);
});
router.on('GET', '/api/v1/templates/library/workflows/{id}', () => {
        return camelToSnakeDeep(templates.find((t) => t.kind === 'workflow') ?? undefined);
});
router.on('GET', '/api/v1/templates/library/agents/{id}', () => {
        return camelToSnakeDeep(templates.find((t) => t.kind === 'agent') ?? undefined);
});

// ---------- triggers ----------
router.on('GET', '/api/v1/triggers/history', (_p, query) => {
        return pageView(camelToSnakeDeep(triggerRecords), +(query.limit ?? 50), +(query.offset ?? 0));
});
router.on('GET', '/api/v1/trigger-executions', (_p, query) => {
        return pageView(camelToSnakeDeep(triggerRecords), +(query.limit ?? 50), +(query.offset ?? 0));
});
router.on('POST', '/api/v1/trigger-executions/cleanup', () => {
        // The backend answers with a bare removed-count number; the
        // fixture sandbox reports the fixture size as removed.
        return triggerRecords.length;
});
router.on('POST', '/api/v1/hooks/{name}', () => ({
        status: 'fired',
        detail: 'Preview dispatch accepted; no live hook endpoint behind the mock.',
}));

// ---------- file checkpoint ----------
router.on('GET', '/api/v1/file-checkpoint/changes', (_p, query) => {
        return pageView(camelToSnakeDeep(fileChanges), +(query.limit ?? 100), +(query.offset ?? 0));
});
router.on('GET', '/api/v1/file-checkpoint/approvals/pending', () => {
        return pageView(camelToSnakeDeep(approvals));
});
router.on('GET', '/api/v1/file-checkpoint/content', (_p, query) => {
        const path = String(query.path ?? fileChanges[0]?.path ?? 'preview.txt');
        const actor = String(query.actor ?? fileChanges[0]?.actor ?? 'agent');
        return {
                path,
                actor,
                hash: 'preview',
                size: 240,
                is_binary: false,
                content: `# Preview sample\n\nRead-only content for ${path} in workspace ${actor}.\n`,
                truncated: false,
                timestamp: FIXTURE_EPOCH,
        };
});
router.on('GET', '/api/v1/file-checkpoint/diff/actors/{a}/{b}', () => ([
        {
                path: 'crates/checkpoint/src/restore_coordinator.rs',
                kind: 'Modified',
                diff: [
                        '--- a/crates/checkpoint/src/restore_coordinator.rs',
                        '+++ b/crates/checkpoint/src/restore_coordinator.rs',
                        '@@ -12,7 +12,7 @@',
                        ' pub fn restore(&self, target: &Branch) -> Result<Snapshot> {',
                        '-    if self.branch_exists(target)? {',
                        '-        return Err(Error::BranchConflict(target.clone()));',
                        '-    }',
                        '+    if let Some(existing) = self.find_snapshot(target)? {',
                        '+        return Ok(existing);',
                        '+    }',
                        '     self.write_snapshot(target)',
                        ' }',
                ].join('\n'),
                additions: 3,
                deletions: 3,
        },
]));
router.on('GET', '/api/v1/file-checkpoint/tree/{id}', () => ({
        entries: fileChanges.map((change) => ({
                path: change.path,
                hash: 'preview',
                size: 1024,
                timestamp: FIXTURE_EPOCH,
        })),
        truncated: false,
        total: fileChanges.length,
}));
router.on('GET', '/api/v1/file-checkpoint/timeline/{id}', (params) => ({
        original_path: params.id,
        entries: [
                {
                        path: params.id,
                        snapshot_id: 'preview-snap-2',
                        content_hash: 'preview',
                        timestamp: FIXTURE_EPOCH,
                        source: 'agent:preview',
                },
                {
                        path: params.id,
                        snapshot_id: 'preview-snap-1',
                        content_hash: 'preview',
                        timestamp: FIXTURE_EPOCH - 3_600_000,
                        source: 'manual',
                },
        ],
        truncated: false,
        total: 2,
}));
router.on('POST', '/api/v1/file-checkpoint/approvals/{id}/approve', () => ({ ok: true }));
router.on('POST', '/api/v1/file-checkpoint/approvals/{id}/reject', () => ({ ok: true }));
router.on('GET', '/api/v1/file-checkpoint/diff/staged/{id}', () => ([
        {
                path: 'crates/checkpoint/src/restore_coordinator.rs',
                kind: 'Modified',
                diff: [
                        '--- a/crates/checkpoint/src/restore_coordinator.rs',
                        '+++ b/crates/checkpoint/src/restore_coordinator.rs',
                        '@@ -12,7 +12,7 @@',
                        ' pub fn restore(&self, target: &Branch) -> Result<Snapshot> {',
                        '-    if self.branch_exists(target)? {',
                        '-        return Err(Error::BranchConflict(target.clone()));',
                        '-    }',
                        '+    if let Some(existing) = self.find_snapshot(target)? {',
                        '+        return Ok(existing);',
                        '+    }',
                        '     self.write_snapshot(target)',
                        ' }',
                ].join('\n'),
                additions: 3,
                deletions: 3,
        },
        {
                path: 'crates/checkpoint/src/branch_probe.rs',
                kind: 'Modified',
                diff: [
                        '--- a/crates/checkpoint/src/branch_probe.rs',
                        '+++ b/crates/checkpoint/src/branch_probe.rs',
                        '@@ -4,6 +4,7 @@',
                        ' pub fn probe(branch: &Branch) -> Probe {',
                        '     let mut probe = Probe::new(branch);',
                        '+    probe.with_cache(true);',
                        '     probe.run()',
                        ' }',
                ].join('\n'),
                additions: 1,
                deletions: 0,
        },
]));

// ---------------------------------------------------------------------------
// Public client object — mirrors openapi-fetch surface used by services
// ---------------------------------------------------------------------------

const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

export const client: AnyClient = {
        // openapi-fetch middleware hooks — no-ops in preview
        use() {},

        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        async GET(path: string, opts?: any): Promise<any> {
                await delay(30); // simulate a tiny network latency
                const query = queryOf(opts?.params);
                const paramsObj = opts?.params?.path ?? {};
                const data = router.dispatch('GET', path, query, undefined);
                if (data === undefined) return missingRoute('GET', path);
                return okEnvelope(data);
        },

        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        async POST(path: string, opts?: any): Promise<any> {
                await delay(30);
                const query = queryOf(opts?.params);
                const paramsObj = opts?.params?.path ?? {};
                const body = opts?.body;
                const data = router.dispatch('POST', path, query, body);
                if (data === undefined) return missingRoute('POST', path);
                return okEnvelope(data);
        },

        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        async PUT(path: string, opts?: any): Promise<any> {
                await delay(30);
                const query = queryOf(opts?.params);
                const paramsObj = opts?.params?.path ?? {};
                const body = opts?.body;
                const data = router.dispatch('PUT', path, query, body);
                if (data === undefined) return missingRoute('PUT', path);
                return okEnvelope(data);
        },

        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        async DELETE(path: string, opts?: any): Promise<any> {
                await delay(30);
                const query = queryOf(opts?.params);
                const paramsObj = opts?.params?.path ?? {};
                const data = router.dispatch('DELETE', path, query, undefined);
                if (data === undefined) return missingRoute('DELETE', path);
                return okEnvelope(data);
        },

        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        async PATCH(path: string, opts?: any): Promise<any> {
                await delay(30);
                const query = queryOf(opts?.params);
                const body = opts?.body;
                const data = router.dispatch('PATCH', path, query, body);
                if (data === undefined) return missingRoute('PATCH', path);
                return okEnvelope(data);
        },
};

// ---------------------------------------------------------------------------
// request / downloadFile — same call shape as the formal client
// ---------------------------------------------------------------------------

export async function request(
        method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE',
        path: string,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        opts?: any,
): Promise<{ data?: unknown; error?: unknown }> {
        await delay(30);
        const query = queryOf(opts?.params);
        const body = opts?.body;
        const data = router.dispatch(method, path, query, body);
        if (data === undefined) return missingRoute(method, path);
        return okEnvelope(data);
}

export async function downloadFile(
        path: string,
        fallbackFilename: string,
        init?: { method?: 'GET' | 'POST'; body?: unknown },
): Promise<void> {
        const method = init?.method ?? 'GET';
        const [urlPath, queryString] = path.split('?');
        const query: Record<string, unknown> = {};
        if (queryString) {
                for (const [key, value] of new URLSearchParams(queryString)) {
                        query[key] = value;
                }
        }
        const data = router.dispatch(method, urlPath, query, init?.body);
        const blob = new Blob([JSON.stringify(data ?? null, null, 2)], {
                type: 'application/json',
        });
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement('a');
        anchor.href = url;
        anchor.download = fallbackFilename;
        anchor.click();
        URL.revokeObjectURL(url);
}

// ---------------------------------------------------------------------------
// resolveApiKey — no-op in preview (no backend to authenticate against)
// ---------------------------------------------------------------------------
export function resolveApiKey(): string | undefined {
        return undefined;
}
