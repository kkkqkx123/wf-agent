# Validation System

## 1. Overview

The validation system provides multi-layer validation for workflow graphs, nodes, and execution configurations.

## 2. Validation Layers

### Graph Validation

`GraphValidator` validates the overall graph structure:

```
GraphValidator.validate(graph)
├── Node validation
│   ├── Required nodes exist (START, END)
│   ├── No duplicate node IDs
│   └── Node type is recognized
├── Edge validation
│   ├── Source and target nodes exist
│   ├── No duplicate edges
│   └── Edge types are valid
├── Structure validation
│   ├── Single start node
│   ├── Reachable from start to end
│   ├── No cycles (DAG enforcement)
│   ├── FORK/JOIN matching
│   └── LOOP_START/LOOP_END matching
└── Subgraph validation
    ├── Subgraph references exist
    ├── No circular subgraph dependencies
    └── Embed graph constraints
```

### Node Validation

`NodeValidator` validates individual node configurations:

```
NodeValidator.validate(node)
├── Common validation
│   ├── Node ID is present
│   ├── Node type is specified
│   └── Required config fields exist
├── Type-specific validation
│   ├── SCRIPT: Script exists, parameters valid
│   ├── LLM: LLM profile exists, prompt valid
│   ├── FORK: Branches configured, targets valid
│   ├── JOIN: Strategy specified, branch count valid
│   ├── SUBGRAPH: Sub-workflow reference exists
│   ├── LOOP: Loop condition valid
│   └── ROUTE: Conditions properly configured
└── Config schema validation
```

### Workflow Validator

`WorkflowValidator` provides top-level workflow validation:

```
WorkflowValidator.validate(workflowTemplate)
├── Workflow metadata validation
├── Graph structure validation
├── Node configuration validation
├── Edge connectivity validation
├── Trigger configuration validation
├── Variable definition validation
├── Hook configuration validation
└── Returns WorkflowValidationResult
```

### Script Config Validator

`CodeConfigValidator` validates script configurations:

- Script content validation
- Parameter type validation
- Sandbox configuration validation
- Execution timeout validation

## 3. Protocol Consistency Validator

`ProtocolConsistencyValidator` ensures consistent naming and type conventions:

- Naming convention checks (camelCase, PascalCase)
- Type consistency across boundaries
- Interface compliance verification
- Cross-module type compatibility

## 4. Node Template Validation

`NodeTemplateValidator` validates node template definitions:

- Template structure validation
- Schema compatibility
- Default value validation
- Required field checks

## 5. Configuration Validation

`WorkflowConfigValidation` provides configuration-level validation:

- Environment variable validation
- Config format validation (TOML, JSON)
- Field type and range validation
- Default value application

## 6. Validation Result

All validation operations return structured results:

```typescript
interface ValidationResult {
  valid: boolean;
  errors: ValidationError[];
  warnings: ValidationWarning[];
  metadata?: Record<string, unknown>;
}

interface ValidationError {
  code: string;
  message: string;
  path?: string;
  field?: string;
  severity: "ERROR" | "WARNING";
}
```

## 7. Integration with Preprocessing

Validation is integrated into the graph preprocessing pipeline:

```
WorkflowGraphBuilder.buildAndValidate()
├── Build graph structure
├── Run GraphValidator.validate()
├── Run NodeValidator.validate() for each node
├── Run PreprocessValidation
└── Return { graph, validationResult }
```