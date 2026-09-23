#!/usr/bin/env python3
"""Generate wf-server contract.json route inventory from .route() calls.

Usage: gen_contract.py <src/api dir> <ws.rs> <out contract.json>
Regenerate after adding routes; the contract probe test checks listed
routes resolve (contract to router). The reverse direction (router to
contract) is covered by regenerating and diffing the committed file.
"""
import json
import re
import sys
from pathlib import Path

METHODS = ("get", "post", "put", "patch", "delete")


def parse_routes(text):
    routes = []
    for match in re.finditer(r"\.route\(", text):
        i = match.end()
        depth = 1
        while i < len(text) and depth > 0:
            if text[i] == "(":
                depth += 1
            elif text[i] == ")":
                depth -= 1
            i += 1
        call = text[match.end() : i - 1]
        path_m = re.search(r'"([^"]+)"', call)
        if not path_m:
            continue
        methods = sorted({m for m in METHODS if re.search(rf"\b{m}\s*\(", call)})
        if methods:
            routes.append((path_m.group(1), methods))
    return routes


def main():
    api_dir, ws_file, out = sys.argv[1], sys.argv[2], sys.argv[3]
    entries = {}
    files = sorted(Path(api_dir).rglob("*.rs")) + [Path(ws_file)]
    # system/metrics.rs belongs to the metrics router, not the API router.
    files = [f for f in files if f.name != "metrics.rs"]
    for file in files:
        try:
            text = file.read_text()
        except OSError:
            continue
        for path, methods in parse_routes(text):
            if file.name != "health.rs" and not path.startswith("/api"):
                full = "/api/v1" + path
            else:
                full = path
            for method in methods:
                entries[(method.upper(), full)] = str(file)
    # The contract endpoint itself lives outside src/api; record it here so
    # the generator stays the single source of truth.
    entries[("GET", "/api/v1/contract")] = "src/contract.rs"
    routes = []
    for (method, path) in sorted(entries):
        stream = "stream" in path or path == "/api/v1/ws"
        routes.append(
            {
                "method": method,
                "path": path,
                "probe": not stream,
                "source": entries[(method, path)],
            }
        )
    doc = {
        "name": "wf-server HTTP contract",
        "version": "v1",
        "envelope": {"success": True, "data": "T|null", "error": None},
        "errorCodes": {
            "NOT_FOUND": 404,
            "INVALID_PARAMS": 400,
            "UNAUTHORIZED": 401,
            "FORBIDDEN": 403,
            "CONFLICT": 409,
            "ALREADY_EXISTS": 409,
            "RATE_LIMITED": 429,
            "TIMEOUT": 504,
            "SERVICE_UNAVAILABLE": 503,
            "STORAGE_ERROR": 500,
            "INTERNAL_ERROR": 500,
        },
        "pagination": {
            "query": ["limit", "offset"],
            "defaults": {"limit": 50, "maxLimit": 500, "offset": 0},
            "shape": "{items, limit, offset, has_more}",
        },
        "shapes": {
            "PageView": "{items: T[], limit: u64, offset: u64, has_more: bool}",
            "CappedView": "{items: T[], truncated: bool, total: usize} (chains cap 500, timelines cap 5000)",
            "BatchItemResult": "{id: string, ok: bool, error?: string}",
            "LoopVariableBatch": {
                "request": "{entries: [{name: string, value: unknown}] (1..100)}",
                "response": "BatchItemResult[]",
            },
            "BatchRespond": "{ids: string[] (1..100), response_data?, result_data?, agent_loop_id?}",
            "SearchResult": "{query, items, by_type, total, truncated, next_cursor?: string}",
            "SearchResultItem": "{id, type, label, score, matches, execution_id?, agent_loop_id?}",
            "SearchCursor": "opaque hex(JSON({source: offset})); pass back as ?cursor=",
            "CappedPaths": "{paths, truncated: bool, total: usize} (cap 1000, agent + workflow aligned)",
            "AuditReport": "{summary, iterations[<=500], node_executions[<=2000], truncated: bool, total_estimate?}",
            "AdvancedErrorAnalysis": "{..., error_hotspots[<=10], truncated: bool}",
            "Download": "alternate mode ?download=true returns attachment (Content-Type + Content-Disposition + Content-Length); query export also accepts body download flag",
            "FileContent": "{path, actor, hash, size, is_binary, content?: string, truncated: bool, timestamp}",
            "FileTree": "{entries: FileTreeEntry[], truncated: bool, total: usize} (cap 2000, sorted before truncation)",
            "WsSubscribe": "{type: subscribe, executionId?|agentLoopId?|workflowId?|global?|notifications?, since?}",
            "WsEvent": "{type, <id field>, eventType, data, timestamp, cursor}",
            "SseSince": "opaque hex token same as WS cursor; raw timestamps accepted for compat",
        },
        "notes": [
            "List endpoints return the paged shape above; retained bare arrays are bounded in-memory catalogs (providers, models, skills, static enums), single-object graph views, or single-parent bounded sets (scopes, node variables).",
            "Export endpoints support ?download=true attachment mode (query export plus workflow, template, variable, audit report); batch export-all and profile export-all stay envelope-only by design.",
            "Unified search covers workflow/execution/task/checkpoint/event/agent_loop/message with opaque cursor continuation.",
            "Large dumps are hard-capped with truncated markers (audit report, error analysis, path enumeration cap 1000, chains cap 500, timelines cap 5000).",
            "WS supports execution/agentLoop/workflow/global/notifications topics with since replay (bounded window); notifications are pure push with receipt-as-consumed semantics. SSE since accepts the same opaque token.",
            "File channel is read-only for content/tree/paged changes/timeline; rename and sessions are explicit workspace operations, approval channel covers human-in-the-loop tool approvals.",
            "POST stream endpoints speak SSE and need fetch streaming, not EventSource.",
        ],
        "routes": routes,
    }
    Path(out).write_text(json.dumps(doc, indent=2) + "\n")
    print(f"wrote {len(routes)} route entries to {out}")


main()
