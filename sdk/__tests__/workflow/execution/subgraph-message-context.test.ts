/**
 * Subgraph Message Context Passing Tests
 * 
 * Tests for the message context passing mechanism between parent and subgraph workflows.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import type { Node, StartNodeConfig, SubgraphNodeConfig, NamedMessageContext } from '@wf-agent/types';
import { validateAndMapMessageContexts, hasMessageContextConfig } from '../../../workflow/validation/utils/message-context-validator.js';
import { InMemoryMessageContextRegistry } from '../../../core/messaging/message-context-registry.js';

describe('Subgraph Message Context Validation', () => {
  describe('validateAndMapMessageContexts', () => {
    it('should validate and map inputs correctly', () => {
      const subgraphNode: Node = {
        id: 'call-research',
        type: 'SUBGRAPH',
        config: {
          subgraphId: 'research-agent',
          async: false,
          messagePassing: {
            inputs: {
              research_query: 'my-query',
              background_knowledge: 'kb-context',
            },
          },
        },
      } as any;

      const startNode: Node = {
        id: 'start',
        type: 'START',
        config: {
          messageInputs: [
            { externalName: 'research_query', internalName: 'query', required: true },
            { externalName: 'background_knowledge', internalName: 'knowledge', required: false },
          ],
        } as StartNodeConfig,
      } as any;

      const result = validateAndMapMessageContexts(subgraphNode, startNode);

      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value.inputMapping.get('my-query')).toBe('query');
        expect(result.value.inputMapping.get('kb-context')).toBe('knowledge');
      }
    });

    it('should validate and map outputs correctly', () => {
      const subgraphNode: Node = {
        id: 'call-research',
        type: 'SUBGRAPH',
        config: {
          subgraphId: 'research-agent',
          async: false,
          messagePassing: {
            outputs: {
              result: 'research-result',
            },
          },
        },
      } as any;

      const startNode: Node = {
        id: 'start',
        type: 'START',
        config: {
          messageOutputs: [
            { internalName: 'analysis_result', externalName: 'result' },
          ],
        } as StartNodeConfig,
      } as any;

      const result = validateAndMapMessageContexts(subgraphNode, startNode);

      expect(result.isOk()).toBe(true);
      if (result.isOk()) {
        expect(result.value.outputMapping.get('analysis_result')).toBe('research-result');
      }
    });

    it('should return error for invalid input reference', () => {
      const subgraphNode: Node = {
        id: 'call-research',
        type: 'SUBGRAPH',
        config: {
          subgraphId: 'research-agent',
          async: false,
          messagePassing: {
            inputs: {
              invalid_input: 'some-context',
            },
          },
        },
      } as any;

      const startNode: Node = {
        id: 'start',
        type: 'START',
        config: {
          messageInputs: [
            { externalName: 'research_query', internalName: 'query', required: true },
          ],
        } as StartNodeConfig,
      } as any;

      const result = validateAndMapMessageContexts(subgraphNode, startNode);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.length).toBe(1);
        expect(result.error[0].message).toContain('does not accept input');
      }
    });

    it('should return error for invalid output reference', () => {
      const subgraphNode: Node = {
        id: 'call-research',
        type: 'SUBGRAPH',
        config: {
          subgraphId: 'research-agent',
          async: false,
          messagePassing: {
            outputs: {
              invalid_output: 'some-context',
            },
          },
        },
      } as any;

      const startNode: Node = {
        id: 'start',
        type: 'START',
        config: {
          messageOutputs: [
            { internalName: 'analysis_result', externalName: 'result' },
          ],
        } as StartNodeConfig,
      } as any;

      const result = validateAndMapMessageContexts(subgraphNode, startNode);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.length).toBe(1);
        expect(result.error[0].message).toContain('does not produce output');
      }
    });

    it('should return error when messagePassing is not configured', () => {
      const subgraphNode: Node = {
        id: 'call-research',
        type: 'SUBGRAPH',
        config: {
          subgraphId: 'research-agent',
          async: false,
          // messagePassing is now required
        },
      } as any;

      const startNode: Node = {
        id: 'start',
        type: 'START',
        config: {},
      } as any;

      const result = validateAndMapMessageContexts(subgraphNode, startNode);

      expect(result.isErr()).toBe(true);
      if (result.isErr()) {
        expect(result.error.length).toBe(1);
        expect(result.error[0].message).toContain('must configure messagePassing');
        expect(result.error[0].context?.code).toBe('MISSING_MESSAGE_PASSING_CONFIG');
      }
    });
  });

  describe('hasMessageContextConfig', () => {
    it('should return true when messagePassing is configured', () => {
      const subgraphNode: Node = {
        id: 'call-research',
        type: 'SUBGRAPH',
        config: {
          subgraphId: 'research-agent',
          async: false,
          messagePassing: {
            inputs: { query: 'current' },
          },
        },
      } as any;

      expect(hasMessageContextConfig(subgraphNode)).toBe(true);
    });

    it('should return false when messagePassing is not configured (deprecated usage)', () => {
      const subgraphNode: Node = {
        id: 'call-research',
        type: 'SUBGRAPH',
        config: {
          subgraphId: 'research-agent',
          async: false,
          // messagePassing is required in new implementation
        },
      } as any;

      expect(hasMessageContextConfig(subgraphNode)).toBe(false);
    });
  });
});

describe('Message Context Registry Integration', () => {
  let registry: InMemoryMessageContextRegistry;

  beforeEach(() => {
    registry = new InMemoryMessageContextRegistry();
  });

  it('should copy context from parent to subgraph', () => {
    // Register parent context
    const parentContext: NamedMessageContext = {
      id: 'my-query',
      messages: [
        { role: 'user', content: 'What is TypeScript?' },
        { role: 'assistant', content: 'TypeScript is a typed superset of JavaScript.' },
      ],
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };
    registry.register(parentContext);

    // Simulate entering subgraph - copy context with new name
    const copiedContext: NamedMessageContext = {
      id: 'query',
      messages: [...parentContext.messages],
      createdAt: Date.now(),
      updatedAt: Date.now(),
      metadata: {
        ...(parentContext.metadata || {}),
        sourceContext: 'my-query',
        passedFromParent: true,
      } as any,
    };
    registry.register(copiedContext);

    // Verify both contexts exist
    expect(registry.has('my-query')).toBe(true);
    expect(registry.has('query')).toBe(true);

    // Verify messages are copied (shallow copy)
    const queryContext = registry.get('query');
    expect(queryContext).toBeDefined();
    expect(queryContext!.messages.length).toBe(2);
    expect(queryContext!.metadata?.sourceContext).toBe('my-query');
  });

  it('should update parent context with subgraph output', () => {
    // Register initial contexts
    const subgraphOutput: NamedMessageContext = {
      id: 'analysis_result',
      messages: [
        { role: 'assistant', content: 'Analysis complete. Found 5 relevant sources.' },
      ],
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };
    registry.register(subgraphOutput);

    // Simulate exiting subgraph - update parent context
    registry.register({
      id: 'research-result',
      messages: [...subgraphOutput.messages],
      createdAt: Date.now(),
      updatedAt: Date.now(),
      metadata: {
        description: 'Output from subgraph research-agent',
        sourceSubgraph: 'research-agent',
      } as any,
    });

    // Verify output context exists
    expect(registry.has('research-result')).toBe(true);
    const resultContext = registry.get('research-result');
    expect(resultContext).toBeDefined();
    expect(resultContext!.messages.length).toBe(1);
    expect(resultContext!.metadata?.sourceSubgraph).toBe('research-agent');
  });

  it('should handle context isolation (no conflicts)', () => {
    // Parent has context A
    registry.register({
      id: 'context-a',
      messages: [{ role: 'user', content: 'Parent context' }],
      createdAt: Date.now(),
      updatedAt: Date.now(),
    });

    // Subgraph creates its own context A (different data)
    registry.register({
      id: 'context-a-subgraph',
      messages: [{ role: 'user', content: 'Subgraph context' }],
      createdAt: Date.now(),
      updatedAt: Date.now(),
    });

    // Both should coexist without conflict
    const parentA = registry.get('context-a');
    const subgraphA = registry.get('context-a-subgraph');

    expect(parentA).toBeDefined();
    expect(subgraphA).toBeDefined();
    expect(parentA!.messages[0].content).toBe('Parent context');
    expect(subgraphA!.messages[0].content).toBe('Subgraph context');
  });
});
