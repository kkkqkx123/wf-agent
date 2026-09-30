import type cytoscape from 'cytoscape';

/**
 * Canvas stylesheet. Data mappers (`data(field)`) are core cytoscape
 * behavior but postdate the shipped stylesheet types, hence the cast.
 */
export const CANVAS_STYLESHEET = [
	{
		selector: 'node',
		style: {
			shape: 'data(shape)',
			width: 120,
			height: 40,
			'background-color': '#18181b',
			'background-opacity': 0.9,
			'border-width': 1.5,
			'border-color': 'data(color)',
			label: 'data(label)',
			color: '#e4e4e7',
			'font-size': 10,
			'text-valign': 'center',
			'text-halign': 'center',
			'text-wrap': 'ellipsis',
			'text-max-width': 104,
		},
	},
	{
		selector: 'node.selected',
		style: {
			'border-width': 3,
			'border-color': '#fafafa',
		},
	},
	{
		selector: 'node.problem',
		style: {
			'border-width': 2.5,
			'border-color': '#dc2626',
			'border-style': 'dashed',
		},
	},
	{
		selector: 'node.failed',
		style: {
			'border-width': 3,
			'border-color': '#ef4444',
		},
	},
	{
		selector: 'node.running',
		style: {
			'border-width': 3,
			'border-color': '#2563eb',
		},
	},
	{
		selector: 'node.critical',
		style: {
			'border-width': 2.5,
			'border-color': '#f59e0b',
		},
	},
	{
		selector: 'node.decision',
		style: {
			'border-width': 2.5,
			'border-color': '#7c3aed',
			'border-style': 'dashed',
		},
	},
	{
		selector: 'node.heat-1',
		style: {
			'border-width': 2,
			'border-color': '#fbbf24',
		},
	},
	{
		selector: 'node.heat-2',
		style: {
			'border-width': 2.5,
			'border-color': '#f97316',
		},
	},
	{
		selector: 'node.heat-3',
		style: {
			'border-width': 3,
			'border-color': '#ea580c',
		},
	},
	{
		selector: 'node.highlighted',
		style: {
			'border-width': 3,
			'border-color': '#f59e0b',
		},
	},
	{
		selector: 'node.dimmed',
		style: { opacity: 0.3 },
	},
	{
		selector: 'edge',
		style: {
			width: 1.5,
			'line-color': 'data(color)',
			'line-style': 'data(lineStyle)',
			'target-arrow-shape': 'triangle',
			'target-arrow-color': 'data(color)',
			'curve-style': 'bezier',
			label: 'data(label)',
			color: '#a1a1aa',
			'font-size': 9,
			'edge-text-rotation': 'autorotate',
			'text-background-color': '#18181b',
			'text-background-opacity': 0.7,
			'text-background-padding': 2,
		},
	},
	{
		selector: 'node.group-title',
		style: {
			width: 160,
			'border-width': 2,
			'border-style': 'dashed',
			'border-color': '#71717a',
		},
	},
	{
		selector: 'node.group-title.failed',
		style: {
			'border-width': 3,
			'border-style': 'solid',
			'border-color': '#ef4444',
		},
	},
	{
		selector: 'node.group-title.running',
		style: {
			'border-width': 3,
			'border-style': 'solid',
			'border-color': '#2563eb',
		},
	},
	{
		selector: 'node.group-title.problem',
		style: {
			'border-width': 2.5,
			'border-style': 'dashed',
			'border-color': '#dc2626',
		},
	},
	{
		selector: 'node.group-title.critical',
		style: {
			'border-width': 2.5,
			'border-style': 'dashed',
			'border-color': '#f59e0b',
		},
	},
	{
		selector: 'node.group-box',
		style: {
			'background-opacity': 0.15,
			'background-color': '#52525b',
			'border-width': 1,
			'border-style': 'dashed',
			'border-color': '#a1a1aa',
			label: 'data(label)',
			color: '#d4d4d8',
			'font-size': 10,
			'text-valign': 'top',
			'text-halign': 'center',
			padding: '14px',
		},
	},
	{
		selector: 'node.connect-ok',
		style: {
			'border-width': 3,
			'border-color': '#16a34a',
		},
	},
	{
		selector: 'node.connect-bad',
		style: { opacity: 0.45 },
	},
	{
		selector: 'edge.dimmed',
		style: { opacity: 0.25 },
	},
] as unknown as cytoscape.StylesheetJson;
