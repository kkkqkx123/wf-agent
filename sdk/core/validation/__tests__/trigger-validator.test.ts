import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  validateTriggerCondition,
  validateExecuteTriggeredSubworkflowActionConfig,
  validateExecuteScriptActionConfig,
  validateTriggerAction,
  validateWorkflowTrigger,
  validateTriggerReference,
  validateTriggers,
} from '../trigger-validator.js';
import { ConfigurationValidationError, ExpressionSecurityError } from '@wf-agent/types';
import { validateExpression } from '../../../workflow/evaluation/index.js';

vi.mock('../../../workflow/evaluation/index.js', () => ({
  validateExpression: vi.fn(),
}));

function createValidCondition(overrides: Record<string, unknown> = {}) {
  return {
    eventType: 'WORKFLOW_EXECUTION_STARTED',
    ...overrides,
  };
}

function createValidTrigger(overrides: Record<string, unknown> = {}) {
  return {
    id: 'trigger-1',
    name: 'Test Trigger',
    condition: createValidCondition(),
    action: {
      type: 'stop_workflow_execution',
      parameters: { executionId: 'exec-1' },
    },
    ...overrides,
  };
}

describe('validateTriggerCondition', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should validate a valid trigger condition', () => {
    const condition = createValidCondition();
    const result = validateTriggerCondition(condition as any);

    expect(result.isOk()).toBe(true);
  });

  it('should reject condition with invalid eventType', () => {
    const condition = createValidCondition({ eventType: 'INVALID_EVENT' });
    const result = validateTriggerCondition(condition as any);

    expect(result.isErr()).toBe(true);
    if (result.isErr()) {
      expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
    }
  });

  it('should reject condition with missing eventType', () => {
    const condition = createValidCondition({ eventType: undefined });
    const result = validateTriggerCondition(condition as any);

    expect(result.isErr()).toBe(true);
  });

  it('should validate condition with eventName', () => {
    const condition = createValidCondition({
      eventType: 'NODE_CUSTOM_EVENT',
      eventName: 'custom_event',
    });
    const result = validateTriggerCondition(condition as any);

    expect(result.isOk()).toBe(true);
  });

  it('should validate condition with metadata', () => {
    const condition = createValidCondition({ metadata: { key: 'value' } });
    const result = validateTriggerCondition(condition as any);

    expect(result.isOk()).toBe(true);
  });

  it('should validate condition with expression', () => {
    const condition = createValidCondition({
      condition: { expression: 'data.status == "completed"' },
    });
    vi.mocked(validateExpression).mockReturnValue(undefined);
    const result = validateTriggerCondition(condition as any);

    expect(result.isOk()).toBe(true);
  });

  it('should reject condition when expression fails security check', () => {
    const condition = createValidCondition({
      condition: { expression: 'unsafe' },
    });
    vi.mocked(validateExpression).mockImplementation(() => {
      throw new ExpressionSecurityError('Expression too long', {
        operation: 'validateExpression',
        field: 'expression',
      });
    });
    const result = validateTriggerCondition(condition as any);

    expect(result.isErr()).toBe(true);
    if (result.isErr()) {
      expect(result.error[0]).toBeInstanceOf(ConfigurationValidationError);
    }
  });

  it('should propagate non-security expression errors', () => {
    const condition = createValidCondition({
      condition: { expression: 'data.status == "completed"' },
    });
    vi.mocked(validateExpression).mockImplementation(() => {
      throw new Error('Unexpected error');
    });
    expect(() => validateTriggerCondition(condition as any)).toThrow('Unexpected error');
  });

  it('should handle condition with empty expression string', () => {
    const condition = createValidCondition({
      condition: { expression: '' },
    });
    const result = validateTriggerCondition(condition as any);

    expect(result.isErr()).toBe(true);
  });

  it('should accept condition without condition field', () => {
    const condition = createValidCondition();
    delete (condition as any).condition;
    const result = validateTriggerCondition(condition as any);

    expect(result.isOk()).toBe(true);
  });
});

describe('validateExecuteTriggeredSubworkflowActionConfig', () => {
  it('should validate a valid subworkflow config', () => {
    const config = { triggeredWorkflowId: 'wf-1' };
    const result = validateExecuteTriggeredSubworkflowActionConfig(config as any);

    expect(result.isOk()).toBe(true);
  });

  it('should reject config with missing triggeredWorkflowId', () => {
    const config = {};
    const result = validateExecuteTriggeredSubworkflowActionConfig(config as any);

    expect(result.isErr()).toBe(true);
  });

  it('should validate config with all optional fields', () => {
    const config = {
      triggeredWorkflowId: 'wf-1',
      waitForCompletion: true,
      timeout: 30000,
      recordHistory: false,
      inputMapping: {
        variables: { key: 'value' },
        messageContexts: { ctx: 'ctx1' },
        additionalParams: { extra: 'data' },
      },
      outputMapping: {
        variables: {
          include: ['var1'],
          includeAll: false,
          rename: { old: 'new' },
        },
        messageContexts: {
          include: ['msg1'],
          includeAll: false,
        },
      },
    };
    const result = validateExecuteTriggeredSubworkflowActionConfig(config as any);

    expect(result.isOk()).toBe(true);
  });

  it('should reject config with empty triggeredWorkflowId', () => {
    const config = { triggeredWorkflowId: '' };
    const result = validateExecuteTriggeredSubworkflowActionConfig(config as any);

    expect(result.isErr()).toBe(true);
  });
});

describe('validateExecuteScriptActionConfig', () => {
  it('should validate a valid script action config', () => {
    const config = { scriptName: 'my_script' };
    const result = validateExecuteScriptActionConfig(config as any);

    expect(result.isOk()).toBe(true);
  });

  it('should reject config with missing scriptName', () => {
    const config = {};
    const result = validateExecuteScriptActionConfig(config as any);

    expect(result.isErr()).toBe(true);
  });

  it('should validate config with all optional fields', () => {
    const config = {
      scriptName: 'my_script',
      parameters: { key: 'value' },
      timeout: 5000,
      ignoreError: true,
      validateExistence: true,
    };
    const result = validateExecuteScriptActionConfig(config as any);

    expect(result.isOk()).toBe(true);
  });

  it('should reject negative timeout', () => {
    const config = {
      scriptName: 'my_script',
      timeout: -1,
    };
    const result = validateExecuteScriptActionConfig(config as any);

    expect(result.isErr()).toBe(true);
  });
});

describe('validateTriggerAction', () => {
  it('should validate stop_workflow_execution action', () => {
    const action = {
      type: 'stop_workflow_execution',
      parameters: { executionId: 'exec-1' },
    };
    const result = validateTriggerAction(action as any);

    expect(result.isOk()).toBe(true);
  });

  it('should validate pause_workflow_execution action', () => {
    const action = {
      type: 'pause_workflow_execution',
      parameters: { executionId: 'exec-1', reason: 'test' },
    };
    const result = validateTriggerAction(action as any);

    expect(result.isOk()).toBe(true);
  });

  it('should validate resume_workflow_execution action', () => {
    const action = {
      type: 'resume_workflow_execution',
      parameters: { executionId: 'exec-1' },
    };
    const result = validateTriggerAction(action as any);

    expect(result.isOk()).toBe(true);
  });

  it('should validate skip_node action', () => {
    const action = {
      type: 'skip_node',
      parameters: { executionId: 'exec-1', nodeId: 'node-1' },
    };
    const result = validateTriggerAction(action as any);

    expect(result.isOk()).toBe(true);
  });

  it('should validate set_variable action', () => {
    const action = {
      type: 'set_variable',
      parameters: { executionId: 'exec-1', variables: { key: 'value' } },
    };
    const result = validateTriggerAction(action as any);

    expect(result.isOk()).toBe(true);
  });

  it('should reject set_variable action with empty variables', () => {
    const action = {
      type: 'set_variable',
      parameters: { executionId: 'exec-1', variables: {} },
    };
    const result = validateTriggerAction(action as any);

    expect(result.isErr()).toBe(true);
  });

  it('should validate send_notification action', () => {
    const action = {
      type: 'send_notification',
      parameters: { message: 'Hello' },
    };
    const result = validateTriggerAction(action as any);

    expect(result.isOk()).toBe(true);
  });

  it('should validate execute_triggered_subworkflow action', () => {
    const action = {
      type: 'execute_triggered_subworkflow',
      parameters: { triggeredWorkflowId: 'wf-1' },
    };
    const result = validateTriggerAction(action as any);

    expect(result.isOk()).toBe(true);
  });

  it('should validate execute_script action', () => {
    const action = {
      type: 'execute_script',
      parameters: { scriptName: 'my_script' },
    };
    const result = validateTriggerAction(action as any);

    expect(result.isOk()).toBe(true);
  });

  it('should reject action with invalid type', () => {
    const action = {
      type: 'invalid_action_type',
      parameters: {},
    };
    const result = validateTriggerAction(action as any);

    expect(result.isErr()).toBe(true);
  });

  it('should reject action with missing parameters', () => {
    const action = {
      type: 'stop_workflow_execution',
      parameters: {},
    };
    const result = validateTriggerAction(action as any);

    expect(result.isErr()).toBe(true);
  });

  it('should validate action with metadata', () => {
    const action = {
      type: 'send_notification',
      parameters: { message: 'Hello' },
      metadata: { source: 'test' },
    };
    const result = validateTriggerAction(action as any);

    expect(result.isOk()).toBe(true);
  });

  it('should validate apply_message_operation action', () => {
    const action = {
      type: 'apply_message_operation',
      parameters: {
        executionId: 'exec-1',
        operationType: 'compress',
      },
    };
    const result = validateTriggerAction(action as any);

    expect(result.isOk()).toBe(true);
  });
});

describe('validateWorkflowTrigger', () => {
  it('should validate a valid workflow trigger', () => {
    const trigger = createValidTrigger();
    const result = validateWorkflowTrigger(trigger as any);

    expect(result.isOk()).toBe(true);
  });

  it('should reject trigger with empty id', () => {
    const trigger = createValidTrigger({ id: '' });
    const result = validateWorkflowTrigger(trigger as any);

    expect(result.isErr()).toBe(true);
  });

  it('should reject trigger with empty name', () => {
    const trigger = createValidTrigger({ name: '' });
    const result = validateWorkflowTrigger(trigger as any);

    expect(result.isErr()).toBe(true);
  });

  it('should reject trigger with invalid condition', () => {
    const trigger = createValidTrigger({
      condition: { eventType: 'INVALID' },
    });
    const result = validateWorkflowTrigger(trigger as any);

    expect(result.isErr()).toBe(true);
  });

  it('should reject trigger with invalid action', () => {
    const trigger = createValidTrigger({
      action: { type: 'invalid_action', parameters: {} },
    });
    const result = validateWorkflowTrigger(trigger as any);

    expect(result.isErr()).toBe(true);
  });

  it('should validate trigger with all optional fields', () => {
    const trigger = createValidTrigger({
      description: 'A test trigger',
      enabled: true,
      maxTriggers: 5,
      metadata: { key: 'value' },
      createCheckpoint: true,
      checkpointDescription: 'Checkpoint desc',
    });
    const result = validateWorkflowTrigger(trigger as any);

    expect(result.isOk()).toBe(true);
  });

  it('should reject negative maxTriggers', () => {
    const trigger = createValidTrigger({ maxTriggers: -1 });
    const result = validateWorkflowTrigger(trigger as any);

    expect(result.isErr()).toBe(true);
  });
});

describe('validateTriggerReference', () => {
  it('should validate a valid trigger reference', () => {
    const reference = {
      templateName: 'my_template',
      triggerId: 'trigger-1',
    };
    const result = validateTriggerReference(reference as any);

    expect(result.isOk()).toBe(true);
  });

  it('should reject reference with empty templateName', () => {
    const reference = { templateName: '', triggerId: 'trigger-1' };
    const result = validateTriggerReference(reference as any);

    expect(result.isErr()).toBe(true);
  });

  it('should reject reference with empty triggerId', () => {
    const reference = { templateName: 'my_template', triggerId: '' };
    const result = validateTriggerReference(reference as any);

    expect(result.isErr()).toBe(true);
  });

  it('should validate reference with configOverride', () => {
    const reference = {
      templateName: 'my_template',
      triggerId: 'trigger-1',
      triggerName: 'My Trigger',
      configOverride: {
        enabled: false,
        maxTriggers: 3,
        condition: { eventType: 'WORKFLOW_EXECUTION_STARTED' },
      },
    };
    const result = validateTriggerReference(reference as any);

    expect(result.isOk()).toBe(true);
  });

  it('should reject reference with invalid configOverride', () => {
    const reference = {
      templateName: 'my_template',
      triggerId: 'trigger-1',
      configOverride: {
        maxTriggers: -5,
      },
    };
    const result = validateTriggerReference(reference as any);

    expect(result.isErr()).toBe(true);
  });
});

describe('validateTriggers', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should validate an array of WorkflowTriggers', () => {
    const triggers = [
      createValidTrigger({ id: 'trigger-1', name: 'Trigger 1' }),
      createValidTrigger({ id: 'trigger-2', name: 'Trigger 2' }),
    ];
    const result = validateTriggers(triggers as any);

    expect(result.isOk()).toBe(true);
    if (result.isOk()) {
      expect(result.value.length).toBe(2);
    }
  });

  it('should validate mixed WorkflowTrigger and TriggerReference', () => {
    const triggers = [
      createValidTrigger({ id: 'trigger-1', name: 'Trigger 1' }),
      { templateName: 'tmpl', triggerId: 'ref-1' },
    ];
    const result = validateTriggers(triggers as any);

    expect(result.isOk()).toBe(true);
    if (result.isOk()) {
      expect(result.value.length).toBe(2);
    }
  });

  it('should return err for non-array input', () => {
    const result = validateTriggers(null as any);
    expect(result.isErr()).toBe(true);
    if (result.isErr()) {
      expect(result.error[0]!.message).toContain('must be an array');
    }
  });

  it('should return err for undefined input', () => {
    const result = validateTriggers(undefined as any);
    expect(result.isErr()).toBe(true);
  });

  it('should return err when any trigger is invalid', () => {
    const triggers = [
      createValidTrigger({ id: 'trigger-1', name: 'Trigger 1' }),
      createValidTrigger({ id: '', name: '' }),
    ];
    const result = validateTriggers(triggers as any);

    expect(result.isErr()).toBe(true);
  });

  it('should detect duplicate trigger IDs', () => {
    const triggers = [
      createValidTrigger({ id: 'trigger-1', name: 'Trigger 1' }),
      createValidTrigger({ id: 'trigger-1', name: 'Trigger 1 Duplicate' }),
    ];
    const result = validateTriggers(triggers as any);

    expect(result.isErr()).toBe(true);
    if (result.isErr()) {
      const hasDuplicateError = result.error.some(e =>
        e.message.includes('must be unique'),
      );
      expect(hasDuplicateError).toBe(true);
    }
  });

  it('should detect duplicate TriggerReference triggerIds', () => {
    const triggers = [
      { templateName: 'tmpl-1', triggerId: 'ref-1' },
      { templateName: 'tmpl-2', triggerId: 'ref-1' },
    ];
    const result = validateTriggers(triggers as any);

    expect(result.isErr()).toBe(true);
  });

  it('should handle empty array', () => {
    const result = validateTriggers([]);
    expect(result.isOk()).toBe(true);
    if (result.isOk()) {
      expect(result.value).toEqual([]);
    }
  });

  it('should reject null/undefined triggers in the array', () => {
    const triggers = [
      createValidTrigger({ id: 'trigger-1', name: 'Trigger 1' }),
      null,
      undefined,
    ];
    const result = validateTriggers(triggers as any);

    expect(result.isErr()).toBe(true);
  });

  it('should report multiple errors at once', () => {
    const triggers = [
      createValidTrigger({ id: 'dup', name: 'First' }),
      createValidTrigger({ id: 'dup', name: 'Second' }),
      createValidTrigger({ id: '', name: '' }),
    ];
    const result = validateTriggers(triggers as any);

    expect(result.isErr()).toBe(true);
    if (result.isErr()) {
      expect(result.error.length).toBeGreaterThan(1);
    }
  });

  it('should validate triggers with expression conditions', () => {
    const triggers = [
      createValidTrigger({
        id: 'trigger-1',
        name: 'Trigger 1',
        condition: {
          eventType: 'WORKFLOW_EXECUTION_COMPLETED',
          condition: { expression: 'data.status == "success"' },
        },
      }),
    ];
    vi.mocked(validateExpression).mockReturnValue(undefined);
    const result = validateTriggers(triggers as any);

    expect(result.isOk()).toBe(true);
  });

  it('should handle TriggerReference with configOverride', () => {
    const triggers = [
      {
        templateName: 'tmpl',
        triggerId: 'ref-1',
        configOverride: {
          condition: { eventType: 'NODE_COMPLETED' },
          enabled: true,
        },
      },
    ];
    const result = validateTriggers(triggers as any);

    expect(result.isOk()).toBe(true);
  });

  it('should detect duplicate IDs across WorkflowTrigger and TriggerReference', () => {
    const triggers = [
      createValidTrigger({ id: 'shared-id', name: 'Trigger 1' }),
      { templateName: 'tmpl', triggerId: 'shared-id' },
    ];
    const result = validateTriggers(triggers as any);

    expect(result.isErr()).toBe(true);
    if (result.isErr()) {
      const hasDuplicateError = result.error.some(e =>
        e.message.includes('must be unique'),
      );
      expect(hasDuplicateError).toBe(true);
    }
  });
});