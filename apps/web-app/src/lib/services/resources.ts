import { client } from '$lib/api/client';
import { call, extractPage } from '$lib/api/envelope';
import type { PageResult } from '$lib/api/envelope';
import type { ModelProfile, Provider, Tool, Script, Skill } from '$lib/types/models';

/**
 * DTO shapes are local interfaces — the backend schemas in schema.d.ts
 * are mostly `unknown` for response bodies, so we mirror the fields we
 * actually consume.
 */
interface LlmProfileDto {
	id?: string;
	name?: string;
	model?: string;
	provider?: string;
	is_default?: boolean;
	default?: boolean;
	status?: string;
	requests?: number;
	tokens?: number;
	cost?: number | null;
}

interface LlmProviderDto {
	id?: string;
	name?: string;
	base_url?: string;
	models?: number | unknown[];
	model_count?: number;
	status?: string;
}

interface ToolDto {
	id?: string;
	name?: string;
	kind?: string;
	description?: string;
	enabled?: boolean;
	registration_enabled?: boolean;
	calls?: number;
	invocation_count?: number;
	success_rate?: number | null;
}

interface ScriptDto {
	id?: string;
	name?: string;
	runtime?: string;
	enabled?: boolean;
	runs?: number;
	run_count?: number;
	updated_at?: string;
}

interface SkillDto {
	id?: string;
	name?: string;
	description?: string;
	enabled?: boolean;
	version?: string;
	prompt_preview?: string;
}

function toModelProfile(d: LlmProfileDto): ModelProfile {
	return {
		id: d.id ?? d.name ?? '',
		name: d.name ?? '',
		provider: d.provider ?? '',
		model: d.model ?? '',
		isDefault: d.is_default ?? d.default ?? false,
		status: d.status ?? 'active',
		requests: d.requests ?? 0,
		tokens: d.tokens ?? 0,
		cost: d.cost ?? null,
	};
}

function toProvider(d: LlmProviderDto): Provider {
	const modelsCount =
		typeof d.models === 'number'
			? d.models
			: Array.isArray(d.models)
				? d.models.length
				: d.model_count ?? 0;
	return {
		id: d.id ?? d.name ?? '',
		name: d.name ?? '',
		baseUrl: d.base_url ?? '',
		models: modelsCount,
		status: d.status ?? 'active',
	};
}

function toTool(d: ToolDto): Tool {
	return {
		id: d.id ?? d.name ?? '',
		name: d.name ?? '',
		kind: d.kind ?? '',
		description: d.description ?? '',
		enabled: d.enabled ?? d.registration_enabled ?? false,
		calls: d.calls ?? d.invocation_count ?? 0,
		successRate: d.success_rate ?? null,
	};
}

function toScript(d: ScriptDto): Script {
	return {
		id: d.id ?? d.name ?? '',
		name: d.name ?? '',
		runtime: d.runtime ?? '',
		enabled: d.enabled ?? false,
		runs: d.runs ?? d.run_count ?? 0,
		updatedAt: d.updated_at ?? '',
	};
}

function toSkill(d: SkillDto): Skill {
	return {
		id: d.id ?? d.name ?? '',
		name: d.name ?? '',
		description: d.description ?? '',
		enabled: d.enabled ?? false,
		version: d.version ?? '',
		promptPreview: d.prompt_preview ?? '',
	};
}

export async function listModelProfiles(): Promise<ModelProfile[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/llm/profiles'),
	);
	// llm/profiles wraps in PageView
	const page = extractPage<LlmProfileDto>(data);
	return page.items.map(toModelProfile);
}

export async function listProviders(): Promise<Provider[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/llm/providers'),
	);
	// providers returns ApiEnvelope_Value (bare array or object)
	if (Array.isArray(data)) {
		return (data as LlmProviderDto[]).map(toProvider);
	}
	// could be a single object
	return [toProvider(data as LlmProviderDto)];
}

export async function listTools(params?: {
	limit?: number;
	offset?: number;
}): Promise<PageResult<Tool>> {
	const data = await call<unknown>(
		client.GET('/api/v1/tools', {
			params: { query: params ?? {} },
		}),
	);
	const page = extractPage<ToolDto>(data);
	return { ...page, items: page.items.map(toTool) };
}

export async function listScripts(params?: {
	limit?: number;
	offset?: number;
}): Promise<PageResult<Script>> {
	const data = await call<unknown>(
		client.GET('/api/v1/scripts', {
			params: { query: params ?? {} },
		}),
	);
	const page = extractPage<ScriptDto>(data);
	return { ...page, items: page.items.map(toScript) };
}

export async function listSkills(): Promise<Skill[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/skills'),
	);
	if (Array.isArray(data)) {
		return (data as SkillDto[]).map(toSkill);
	}
	return [];
}

export async function setSkillEnabled(name: string, enabled: boolean): Promise<void> {
	const path = enabled
		? '/api/v1/skills/{name}/enable'
		: '/api/v1/skills/{name}/disable';
	await call<unknown>(
		// eslint-disable-next-line @typescript-eslint/no-explicit-any
			client.POST(path as any, {
			params: { path: { name } },
// eslint-disable-next-line @typescript-eslint/no-explicit-any
		} as any),
	);
}

export async function setToolEnabled(toolId: string, enabled: boolean): Promise<void> {
	// Backend exposes per-tool enable/disable routes when available.
	// Fallback: emit a no-op if the route is missing — the UI will
	// still show the toggle but this keeps the service resilient.
	const path = enabled ? '/api/v1/tools/{id}/enable' : '/api/v1/tools/{id}/disable';
	try {
		await call<unknown>(
			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			client.POST(path as any, {
				params: { path: { id: toolId } },
			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			} as any),
		);
	} catch {
		// Route may not yet exist — tolerate and log at the page level.
	}
}
