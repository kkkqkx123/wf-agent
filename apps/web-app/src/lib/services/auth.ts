import { client } from '#lib/api/client';
import { call, requireData } from '#lib/api/envelope';

export interface AuthProviderStatus {
	id: string;
	display_name: string;
	method: string;
	login_label: string | null;
	is_subscription: boolean;
	has_credential: boolean;
}

export interface AuthLoginStart {
	session_id: string;
	state: unknown;
}

function toAuthProviderStatus(value: unknown): AuthProviderStatus | null {
	if (!value || typeof value !== 'object') return null;
	const record = value as Record<string, unknown>;
	const id = typeof record.id === 'string' ? record.id : '';
	if (!id) return null;
	return {
		id,
		display_name:
			typeof record.display_name === 'string' ? record.display_name : '',
		method: typeof record.method === 'string' ? record.method : 'api_key',
		login_label:
			typeof record.login_label === 'string' ? record.login_label : null,
		is_subscription: record.is_subscription === true,
		has_credential: record.has_credential === true,
	};
}

export async function listAuthProviders(): Promise<AuthProviderStatus[]> {
	const data = await call<unknown>(client.GET('/api/v1/llm/auth'));
	const payload = requireData(data, 'Auth providers');
	const items = Array.isArray(payload) ? payload : [];
	return items
		.map(toAuthProviderStatus)
		.filter((item): item is AuthProviderStatus => item !== null);
}

export async function startAuthLogin(
	provider_id: string,
): Promise<AuthLoginStart> {
	const data = await call<unknown>(
		client.POST('/api/v1/llm/auth/login', {
			body: { provider_id },
		}),
	);
	const payload = requireData(
		data as AuthLoginStart | null | undefined,
		`Login start missing for ${provider_id}`,
	);
	if (!payload.session_id) {
		throw new Error(`Login start missing session_id for ${provider_id}`);
	}
	return payload;
}

export async function pollAuthLogin(session_id: string): Promise<unknown> {
	const data = await call<unknown>(
		client.GET('/api/v1/llm/auth/login/{id}', {
			params: { path: { id: session_id } },
		}),
	);
	return requireData(data, `Login state missing for ${session_id}`);
}

export async function submitAuthLogin(
	session_id: string,
	input: string,
): Promise<unknown> {
	const data = await call<unknown>(
		client.POST('/api/v1/llm/auth/login/{id}/submit', {
			params: { path: { id: session_id } },
			body: { input },
		}),
	);
	return requireData(data, `Login submit missing for ${session_id}`);
}

export async function cancelAuthLogin(session_id: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/llm/auth/login/{id}/cancel', {
			params: { path: { id: session_id } },
		}),
	);
}

export async function logoutAuth(provider_id: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/llm/auth/logout', {
			body: { provider_id },
		}),
	);
}
