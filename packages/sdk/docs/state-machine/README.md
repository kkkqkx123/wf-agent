# Workflow Execution and Agent Loop State Machine Analysis

This directory contains analysis documents for the execution state machines of WorkflowExecution and AgentLoop instances in the Modular Agent Framework.

## Documents

- **[workflow-execution-state-machine.md](workflow-execution-state-machine.md)** - Workflow execution instance state machine
  - Status definitions and transitions
  - State management classes
  - Validation rules and state transitor

- **[agent-loop-state-machine.md](agent-loop-state-machine.md)** - Agent loop execution instance state machine
  - Status definitions and transitions
  - State management classes
  - Streaming state tracking

## Overview

The framework implements two primary execution state machines:

1. **Workflow Execution State Machine** - Controls workflow execution
2. **Agent Loop State Machine** - Controls agent loop iteration execution

Both follow similar state patterns (CREATED -> RUNNING -> terminal states) but have different characteristics suited to their specific execution models.
