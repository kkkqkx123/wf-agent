/** Mock routes: LLM profiles, tools, scripts and skills. */

import { modelProfiles, providers, tools, scripts, skills } from '#lib/fixtures/resources';
import { camelToSnakeDeep, pageView, type MockRouter } from '../router';

const providerModels: Record<string, { id: string; name: string }[]> = {
	openai: [{ id: 'gpt-4o-mini', name: 'GPT 4o Mini' }],
	anthropic: [{ id: 'claude-sonnet', name: 'Claude Sonnet' }],
	'azure-openai': [{ id: 'gpt-4o', name: 'GPT 4o' }],
	ollama: [{ id: 'qwen2.5-coder', name: 'Qwen Coder' }],
};

export function registerResources(router: MockRouter): void {
	// ---------- resources ----------
	router.on('GET', '/api/v1/llm/profiles', () => pageView(camelToSnakeDeep(modelProfiles)));
	router.on('GET', '/api/v1/llm/providers', () => camelToSnakeDeep(providers));
	router.on('GET', '/api/v1/llm/models', () => camelToSnakeDeep(providerModels));
	router.on('GET', '/api/v1/llm/providers/{id}/models', (params) =>
		camelToSnakeDeep(providerModels[(params as { id: string }).id] ?? []),
	);
	router.on('GET', '/api/v1/tools', (_p, query) =>
		pageView(camelToSnakeDeep(tools), +(query.limit ?? 50), +(query.offset ?? 0)),
	);
	router.on('GET', '/api/v1/scripts', (_p, query) =>
		pageView(camelToSnakeDeep(scripts), +(query.limit ?? 50), +(query.offset ?? 0)),
	);
	router.on('GET', '/api/v1/skills', () => camelToSnakeDeep(skills));
	router.on('POST', '/api/v1/skills/{name}/enable', () => ({ ok: true }));
	router.on('POST', '/api/v1/skills/{name}/disable', () => ({ ok: true }));
	router.on('POST', '/api/v1/tools/{id}/enable', () => ({ ok: true }));
	router.on('POST', '/api/v1/tools/{id}/disable', () => ({ ok: true }));
}
