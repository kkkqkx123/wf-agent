/** Display derivation for tool calls. Backend audit views carry name, arguments and result only, so kind, endpoint and exit code are derived here with a neutral fallback. */

const KIND_BY_NAME: Array<{ match: RegExp; kind: string }> = [
	{ match: /approv/i, kind: 'approval' },
	{ match: /mcp/i, kind: 'mcp' },
	{ match: /shell|bash|cli|command|exec|terminal/i, kind: 'bash' },
	{ match: /file|read|write|glob|path|tree|diff/i, kind: 'file' },
	{ match: /search|grep|web|fetch|http|rest|gateway|endpoint|url/i, kind: 'network' },
	{ match: /memory|remember|recall/i, kind: 'memory' },
	{ match: /knowledge|doc|retriev/i, kind: 'knowledge' },
	{ match: /agent|sub.?agent|spawn/i, kind: 'agent' },
	{ match: /ask|question|completion|interact/i, kind: 'interaction' },
	{ match: /workflow|plan|graph/i, kind: 'workflow' },
	{ match: /todo|utility|util/i, kind: 'utility' },
	{ match: /risk|guard|policy/i, kind: 'risk' },
	{ match: /script|run|eval/i, kind: 'script' },
];

/** Infer a display kind from the tool name. Unknown names yield an empty kind for neutral fallback. */
export function inferToolKind(name: string, hint = ''): string {
	const source = `${hint} ${name}`.trim();
	if (!source) return '';
	for (const entry of KIND_BY_NAME) {
		if (entry.match.test(source)) return entry.kind;
	}
	return hint;
}

function parseJson(value: string): unknown {
	const trimmed = value.trim();
	if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) return null;
	try {
		return JSON.parse(trimmed);
	} catch {
		return null;
	}
}

function pickString(record: Record<string, unknown>, keys: string[]): string {
	for (const key of keys) {
		const value = record[key];
		if (typeof value === 'string' && value) return value;
	}
	return '';
}

function pickInt(record: Record<string, unknown>, keys: string[]): number | null {
	for (const key of keys) {
		const value = record[key];
		if (typeof value === 'number' && Number.isFinite(value))
			return Math.trunc(value);
	}
	const details = record.details;
	if (details && typeof details === 'object') {
		const nested = pickInt(details as Record<string, unknown>, keys);
		if (nested !== null) return nested;
	}
	const result = record.result;
	if (result && typeof result === 'object') {
		const nested = pickInt(result as Record<string, unknown>, keys);
		if (nested !== null) return nested;
	}
	return null;
}

/** Gateway endpoint from tool arguments such as url, endpoint or uri. */
export function parseToolEndpoint(input: string): string {
	const parsed = parseJson(input);
	if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return '';
	const record = parsed as Record<string, unknown>;
	const direct = pickString(record, ['endpoint', 'url', 'uri']);
	if (direct) return direct;
	const params = record.parameters;
	if (params && typeof params === 'object' && !Array.isArray(params)) {
		return pickString(params as Record<string, unknown>, [
			'endpoint',
			'url',
			'uri',
		]);
	}
	return '';
}

/** Script exit code from tool results, including nested details. */
export function parseToolExitCode(output: string): number | null {
	const parsed = parseJson(output);
	if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed))
		return null;
	return pickInt(parsed as Record<string, unknown>, [
		'exit_code',
		'exitCode',
		'code',
	]);
}

/** Approval request id from tool arguments or results when present. */
export function parseApprovalId(input: string, output: string): string {
	for (const text of [input, output]) {
		const parsed = parseJson(text);
		if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed))
			continue;
		const found = pickString(parsed as Record<string, unknown>, [
			'approval_id',
			'approvalId',
			'approval',
		]);
		if (found) return found;
	}
	return '';
}
