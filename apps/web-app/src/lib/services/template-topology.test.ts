import { describe, expect, it } from 'vitest';
import { mergeTemplateGraph, parseTemplateTopology } from './template-topology';

describe('template topology node config', () => {
	it('reads and preserves node config through a graph merge', () => {
		const value = {
			definition: {
				nodes: [
					{
						id: 'n1',
						node_type: 'LLM',
						name: 'Original',
						config: { profile_id: 'p1' },
					},
				],
				edges: [],
			},
		};
		const topology = parseTemplateTopology(value);
		expect(topology.nodes[0]?.config).toEqual({ profile_id: 'p1' });

		const merged = mergeTemplateGraph(
			value,
			[
				{
					...topology.nodes[0]!,
					label: 'Renamed',
					config: { template_id: 'nt-llm' },
				},
			],
			[],
		) as { definition: { nodes: Array<{ name: string; config: Record<string, unknown> }> } };

		expect(merged.definition.nodes[0]?.name).toBe('Renamed');
		expect(merged.definition.nodes[0]?.config).toEqual({ template_id: 'nt-llm' });
	});

	it('keeps stored config when the display node has none', () => {
		const value = {
			nodes: [
				{
					id: 'n1',
					node_type: 'SCRIPT',
					name: 'Script',
					config: { template_id: 'nt-script' },
				},
			],
			edges: [],
		};
		const topology = parseTemplateTopology(value);
		const merged = mergeTemplateGraph(value, [{ ...topology.nodes[0]!, label: 'Renamed' }], []) as {
			nodes: Array<{ config: Record<string, unknown> }>;
		};

		expect(merged.nodes[0]?.config).toEqual({ template_id: 'nt-script' });
	});
});
