import { describe, expect, it } from 'vitest';
import { parsePendingInteractions } from './interactions';

function page(items: unknown[]): unknown {
	return { items, limit: items.length, offset: 0, has_more: false };
}

describe('parsePendingInteractions', () => {
	it('drops resolved records and keeps pending ones oldest first', () => {
		const result = parsePendingInteractions(
			page([
				{
					id: 'b',
					execution_id: 'loop-1',
					interaction_type: 'tool_approval',
					status: 'responded',
					request_data: { tool_name: 'bash' },
					created_at: 1,
				},
				{
					id: 'a',
					execution_id: 'loop-1',
					interaction_type: 'tool_approval',
					status: 'pending',
					request_data: { tool_name: 'bash' },
					created_at: 2,
				},
			]),
		);
		expect(result.map((item) => item.id)).toEqual(['a']);
	});

	it('reads the tool approval payload', () => {
		const [interaction] = parsePendingInteractions(
			page([
				{
					id: 'i1',
					execution_id: 'loop-1',
					interaction_type: 'tool_approval',
					status: 'pending',
					created_at: 5,
					request_data: {
						tool_call_id: 'tc1',
						tool_name: 'write_file',
						tool_description: 'Write a file',
						risk_level: 'high',
						parameters: { path: '/tmp/x' },
						batch_id: 'batch-1',
						tool_index: 1,
						total_tools: 3,
						pending_queue: [{ id: 'tc2', name: 'bash', risk_level: 'medium' }],
					},
				},
			]),
		);
		expect(interaction.approval?.toolName).toBe('write_file');
		expect(interaction.approval?.riskLevel).toBe('high');
		expect(interaction.approval?.pendingQueue).toEqual([
			{ id: 'tc2', name: 'bash', riskLevel: 'medium' },
		]);
		expect(interaction.questions).toEqual([]);
	});

	it('degrades a missing tool name to a placeholder', () => {
		const [interaction] = parsePendingInteractions(
			page([
				{
					id: 'i2',
					interaction_type: 'tool_approval',
					status: 'pending',
					request_data: {},
				},
			]),
		);
		expect(interaction.approval?.toolName).toBe('tool');
	});

	it('reads follow-up questions with both option shapes', () => {
		const [interaction] = parsePendingInteractions(
			page([
				{
					id: 'i3',
					interaction_type: 'followup_question',
					status: 'pending',
					request_data: {
						additional_info_label: 'Other',
						questions: [
							{
								index: 0,
								text: 'Which target?',
								options: [
									'local',
									{ label: 'Remote', value: 'remote', description: 'over ssh' },
								],
							},
						],
					},
				},
			]),
		);
		expect(interaction.approval).toBeNull();
		expect(interaction.questions[0].text).toBe('Which target?');
		expect(interaction.questions[0].options).toEqual([
			{ label: 'local', value: 'local', description: null },
			{ label: 'Remote', value: 'remote', description: 'over ssh' },
		]);
		expect(interaction.additionalInfoLabel).toBe('Other');
	});

	it('ignores records without an id or type', () => {
		expect(
			parsePendingInteractions(
				page([
					{ id: '', status: 'pending' },
					{ id: 'x', status: 'pending' },
				]),
			),
		).toEqual([]);
	});

	it('tolerates a missing payload', () => {
		expect(parsePendingInteractions(null)).toEqual([]);
		expect(parsePendingInteractions({})).toEqual([]);
	});
});
