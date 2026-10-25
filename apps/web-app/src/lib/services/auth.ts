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
		display_name: typeof record.display_name === 'string' ? record.display_name : '',
		method: typeof record.method === 'string' ? record.method : 'api_key',
		login_label: typeof record.login_label === 'string' ? record.login_label : null,
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

export async function startAuthLogin(provider_id: string): Promise<AuthLoginStart> {
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

export interface AuthPromptOption {
	id: string;
	label: string;
	description?: string | null;
}

export type AuthPrompt =
	| { kind: 'Text'; message: string; placeholder?: string | null }
	| { kind: 'Secret'; message: string; placeholder?: string | null }
	| {
			kind: 'Select';
			message: string;
			options: AuthPromptOption[];
	  }
	| { kind: 'ManualCode'; message: string; placeholder?: string | null };

export interface AuthInfoLink {
	url: string;
	label?: string | null;
}

export type AuthEvent =
	| { kind: 'Info'; message: string; links: AuthInfoLink[] }
	| { kind: 'AuthUrl'; url: string; instructions?: string | null }
	| {
			kind: 'DeviceCode';
			user_code: string;
			verification_uri: string;
			interval_seconds?: number | null;
	  }
	| { kind: 'Progress'; message: string };

export type LoginStateKind = 'Challenge' | 'Pending' | 'AwaitingInput' | 'Completed' | 'Failed';

export interface ParsedLoginState {
	kind: LoginStateKind;
	events: AuthEvent[];
	prompt: AuthPrompt | null;
	completedProvider?: string;
	error?: string;
}

function asRecord(value: unknown): Record<string, unknown> | null {
	if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
	return value as Record<string, unknown>;
}

function asString(value: unknown): string | null {
	return typeof value === 'string' ? value : null;
}

function parsePrompt(value: unknown): AuthPrompt | null {
	const record = asRecord(value);
	if (!record) return null;
	const keys = Object.keys(record);
	if (keys.length === 0) return null;
	const kind = keys[0];
	const body = asRecord(record[kind]) ?? {};
	const message = asString(body.message) ?? 'Input required';
	if (kind === 'Select') {
		const rawOptions = Array.isArray(body.options) ? body.options : [];
		const options: AuthPromptOption[] = [];
		for (const item of rawOptions) {
			const option = asRecord(item);
			if (!option) continue;
			const id = asString(option.id);
			const label = asString(option.label);
			if (!id || !label) continue;
			options.push({
				id,
				label,
				description: asString(option.description),
			});
		}
		return { kind: 'Select', message, options };
	}
	if (kind === 'Secret') {
		return {
			kind: 'Secret',
			message,
			placeholder: asString(body.placeholder),
		};
	}
	if (kind === 'ManualCode') {
		return {
			kind: 'ManualCode',
			message,
			placeholder: asString(body.placeholder),
		};
	}
	return {
		kind: 'Text',
		message,
		placeholder: asString(body.placeholder),
	};
}

function parseEvent(value: unknown): AuthEvent | null {
	const record = asRecord(value);
	if (!record) return null;
	const keys = Object.keys(record);
	if (keys.length === 0) return null;
	const kind = keys[0];
	const body = asRecord(record[kind]) ?? {};
	if (kind === 'AuthUrl') {
		const url = asString(body.url);
		if (!url) return null;
		return { kind: 'AuthUrl', url, instructions: asString(body.instructions) };
	}
	if (kind === 'DeviceCode') {
		const user_code = asString(body.user_code);
		const verification_uri = asString(body.verification_uri);
		if (!user_code || !verification_uri) return null;
		const interval = body.interval_seconds;
		return {
			kind: 'DeviceCode',
			user_code,
			verification_uri,
			interval_seconds: typeof interval === 'number' ? interval : null,
		};
	}
	if (kind === 'Info') {
		const links: AuthInfoLink[] = [];
		const rawLinks = body.links;
		if (Array.isArray(rawLinks)) {
			for (const item of rawLinks) {
				const link = asRecord(item);
				const url = link ? asString(link.url) : null;
				if (url) links.push({ url, label: link ? asString(link.label) : null });
			}
		}
		return {
			kind: 'Info',
			message: asString(body.message) ?? '',
			links,
		};
	}
	if (kind === 'Progress') {
		return { kind: 'Progress', message: asString(body.message) ?? '' };
	}
	return null;
}

function parseEvents(value: unknown): AuthEvent[] {
	if (!Array.isArray(value)) return [];
	const events: AuthEvent[] = [];
	for (const item of value) {
		const event = parseEvent(item);
		if (event) events.push(event);
	}
	return events;
}

export function parseLoginState(state: unknown): ParsedLoginState {
	const record = asRecord(state);
	const fallback: ParsedLoginState = {
		kind: 'Pending',
		events: [],
		prompt: null,
	};
	if (!record) return fallback;
	const keys = Object.keys(record);
	if (keys.length === 0) return fallback;
	const kind = keys[0] as LoginStateKind;
	const body = asRecord(record[kind]) ?? {};
	if (kind === 'AwaitingInput') {
		return {
			kind,
			events: parseEvents(body.events),
			prompt: parsePrompt(body.prompt),
		};
	}
	if (kind === 'Challenge' || kind === 'Pending') {
		return { kind, events: parseEvents(body.events), prompt: null };
	}
	if (kind === 'Completed') {
		return {
			kind,
			events: parseEvents(body.events),
			prompt: null,
			completedProvider: asString(body.provider_id) ?? undefined,
		};
	}
	if (kind === 'Failed') {
		return {
			kind,
			events: parseEvents(body.events),
			prompt: null,
			error: asString(body.error) ?? 'Login failed',
		};
	}
	return fallback;
}

export function loginPollInterval(events: AuthEvent[]): number {
	for (let index = events.length - 1; index >= 0; index -= 1) {
		const event = events[index];
		if (
			event.kind === 'DeviceCode' &&
			typeof event.interval_seconds === 'number' &&
			event.interval_seconds > 0
		) {
			return event.interval_seconds;
		}
	}
	return 5;
}

export async function pollAuthLogin(session_id: string): Promise<unknown> {
	const data = await call<unknown>(
		client.GET('/api/v1/llm/auth/login/{id}', {
			params: { path: { id: session_id } },
		}),
	);
	return requireData(data, `Login state missing for ${session_id}`);
}

export async function submitAuthLogin(session_id: string, input: string): Promise<unknown> {
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
