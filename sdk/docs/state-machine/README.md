# Thread and Agent Loop State Machine Analysis

This directory contains analysis documents for the execution state machines of Thread and Agent Loop instances in the Modular Agent Framework.

## Documents

- **[thread-state-machine.md](thread-state-machine.md)** - Thread execution instance state machine
  - Status definitions and transitions
  - State management classes
  - Validation rules and state transitor

- **[agent-loop-state-machine.md](agent-loop-state-machine.md)** - Agent loop execution instance state machine
  - Status definitions and transitions
  - State management classes
  - Streaming state tracking

## Overview

The framework implements two primary execution state machines:

1. **Thread State Machine** - Controls workflow thread execution
2. **Agent Loop State Machine** - Controls agent loop iteration execution

Both follow similar state patterns (CREATED -> RUNNING -> terminal states) but have different characteristics suited to their specific execution models.