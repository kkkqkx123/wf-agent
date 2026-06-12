# Skill Tool

The `skill` tool enables dynamic skill execution within workflows. It provides:

- Schema definition for skill configuration
- Handler for executing skill logic
- Type definitions for type safety

This tool is designed for interaction scenarios requiring flexible, user-defined skill invocation.

## Usage

Import and register the tool in your agent configuration:

```ts
import {
  createSkillHandler,
  skillSchema,
  SKILL_TOOL_DESCRIPTION,
} from "@wf-agent/sdk/resources/predefined/tools/builtin/skill";
```
