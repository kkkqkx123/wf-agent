/**
 * Interaction channel: the pending tool approvals and follow-up questions an
 * agent loop is blocked on. The engine persists each request as a user
 * interaction record scoped to the owning agent loop and waits for a response,
 * so this module wraps the three transport calls the chat session needs:
 * list by execution, respond to an approval, respond to a question.
 *
 * The record payload is untyped on the wire, so every field is read through a
 * guard and degraded to a safe default rather than trusted.
 */

import { client } from '#lib/api/client';
import { call, extractPage, requireData } from '#lib/api/envelope';

/** One offered answer of a follow-up question. */
export interface InteractionOption {
	label: string;
	value: string;
	description: string | null;
}

/** One follow-up question with its optional multiple-choice options. */
export interface InteractionQuestion {
	index: number;
	text: string;
	options: InteractionOption[];
}

/** One tool call waiting behind the current approval in the same batch. */
export interface PendingToolRef {
	id: string;
	name: string;
	riskLevel: string | null;
}

/** Tool approval payload of a pending interaction. */
export interface InteractionApproval {
	toolCallId: string;
	toolName: string;
	toolDescription: string | null;
	parameters: unknown;
	riskLevel: string | null;
	pendingQueue: PendingToolRef[];
	batchId: string | null;
	toolIndex: number | null;
	totalTools: number | null;
}

/** One pending interaction as the session surface consumes it. */
export interface PendingInteraction {
	id: string;
	executionId: string;
	type: string;
	createdAt: number;
	/** Tool approval payload; null for follow-up questions. */
	approval: InteractionApproval | null;
	/** Follow-up questions; empty for approvals. */
	questions: InteractionQuestion[];
	additionalInfoLabel: string | null;
}

/** Answer of one follow-up question. */
export interface InteractionAnswer {
	questionIndex: number;
	selectedValue: string | null;
	customInput: string | null;
}

interface InteractionDto {
	id?: unknown;
	execution_id?: unknown;
	interaction_type?: unknown;
	status?: unknown;
	request_data?: unknown;
	created_at?: unknown;
}

function asString(value: unknown): string {
	return typeof value === 'string' ? value : '';
}

function asNumber(value: unknown): number | null {
	return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function asRecord(value: unknown): Record<string, unknown> | null {
	return value !== null && typeof value === 'object' ? (value as Record<string, unknown>) : null;
}

function asArray(value: unknown): unknown[] {
	return Array.isArray(value) ? value : [];
}

/** Read one option, accepting the label/value/description object shape. */
function toOption(raw: unknown): InteractionOption | null {
	if (typeof raw === 'string') {
		return { label: raw, value: raw, description: null };
	}
	const record = asRecord(raw);
	if (!record) return null;
	const label = asString(record.label) || asString(record.text);
	const value = asString(record.value) || label;
	if (!label && !value) return null;
	return {
		label: label || value,
		value,
		description: asString(record.description) || null,
	};
}

/** Read one question; the engine uses a 0-based index. */
function toQuestion(raw: unknown, fallbackIndex: number): InteractionQuestion | null {
	const record = asRecord(raw);
	if (!record) return null;
	const text = asString(record.text) || asString(record.prompt) || asString(record.question);
	if (!text) return null;
	const options = asArray(record.options)
		.map(toOption)
		.filter((option): option is InteractionOption => option !== null);
	return {
		index: asNumber(record.index) ?? fallbackIndex,
		text,
		options,
	};
}

function toApproval(request: Record<string, unknown>): InteractionApproval {
	const queue = asArray(request.pending_queue)
		.map((raw) => {
			const record = asRecord(raw);
			if (!record) return null;
			const name = asString(record.name);
			if (!name) return null;
			return {
				id: asString(record.id),
				name,
				riskLevel: asString(record.risk_level) || null,
			};
		})
		.filter((ref): ref is PendingToolRef => ref !== null);
	return {
		toolCallId: asString(request.tool_call_id),
		toolName: asString(request.tool_name) || 'tool',
		toolDescription: asString(request.tool_description) || null,
		parameters: request.parameters ?? null,
		riskLevel: asString(request.risk_level) || null,
		pendingQueue: queue,
		batchId: asString(request.batch_id) || null,
		toolIndex: asNumber(request.tool_index),
		totalTools: asNumber(request.total_tools),
	};
}

function toPendingInteraction(dto: InteractionDto): PendingInteraction | null {
	const id = asString(dto.id);
	const type = asString(dto.interaction_type);
	if (!id || !type) return null;
	const request = asRecord(dto.request_data) ?? {};
	const questions = asArray(request.questions)
		.map((raw, index) => toQuestion(raw, index))
		.filter((question): question is InteractionQuestion => question !== null);
	return {
		id,
		executionId: asString(dto.execution_id),
		type,
		createdAt: asNumber(dto.created_at) ?? 0,
		approval: type === 'tool_approval' ? toApproval(request) : null,
		questions,
		additionalInfoLabel: asString(request.additional_info_label) || null,
	};
}

/** Project a page payload into pending interactions, oldest first. Resolved
 * records are dropped so callers only ever see what still blocks the run. */
export function parsePendingInteractions(data: unknown): PendingInteraction[] {
	return extractPage<InteractionDto>(data)
		.items.filter((dto) => asString(dto.status) === 'pending')
		.map(toPendingInteraction)
		.filter((item): item is PendingInteraction => item !== null)
		.sort((a, b) => a.createdAt - b.createdAt);
}

/** Pending interactions of one execution, oldest first. */
export async function listPendingInteractions(executionId: string): Promise<PendingInteraction[]> {
	const data = await call<unknown>(
		client.GET('/api/v1/interactions/by-execution/{executionId}', {
			params: { path: { executionId }, query: { limit: 100 } },
		}),
	);
	requireData(data, `Interactions missing for execution ${executionId}`);
	return parsePendingInteractions(data);
}

/** Answer a tool approval. Denial carries an optional reason. */
export async function respondToolApproval(
	interactionId: string,
	approved: boolean,
	reason?: string,
): Promise<void> {
	const response_data: Record<string, unknown> = { approved };
	if (!approved && reason) response_data.rejection_reason = reason;
	await call<unknown>(
		client.POST('/api/v1/interactions/{id}/respond', {
			params: { path: { id: interactionId } },
			body: { response_data },
		}),
	);
}

/** Answer a follow-up question set, with optional free-form extra info. */
export async function respondToQuestions(
	interactionId: string,
	answers: InteractionAnswer[],
	additionalInfo?: string,
): Promise<void> {
	const response_data: Record<string, unknown> = { answers };
	if (additionalInfo) response_data.additional_info = additionalInfo;
	await call<unknown>(
		client.POST('/api/v1/interactions/{id}/respond', {
			params: { path: { id: interactionId } },
			body: { response_data },
		}),
	);
}

/** Decline a follow-up question without an answer (CLI cancel semantics: a
 * null response payload). */
export async function cancelInteraction(interactionId: string): Promise<void> {
	await call<unknown>(
		client.POST('/api/v1/interactions/{id}/respond', {
			params: { path: { id: interactionId } },
			body: { response_data: null },
		}),
	);
}
