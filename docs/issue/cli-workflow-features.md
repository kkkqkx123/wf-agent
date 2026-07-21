# CLI Workflow Feature Reimplementation

## Context

After rolling back the temp commit's forced source-code changes, the following workflow features need to be properly reimplemented in `workflow-adapter.ts` and `workflow/index.ts`. The old implementation suffered from type-safety regressions (`as any` casts, `any` types, `[key: string]: any` index signatures) — the new implementation must be architecturally clean and type-safe.

---

## 1. Adapter Methods (`workflow-adapter.ts`)

### 1.1 updateWorkflow

```
workflow update <id> --from-file <file> [-p <params>]
```

**Required behavior:**
- Load TOML/JSON config file via `loadConfigFile` + `parseWorkflow`
- Call `api.update(id, workflow)` to update in registry
- Return the updated `WorkflowTemplate`
- On failure (non-existent id): throw with appropriate error

**Test expectations:**
- Exit code 0 on success
- stdout contains "Workflow updated" and the workflow id
- Exit code != 0 for non-existent workflow

**Type-safety constraints:**
- `api.update(id, workflow)` — `update` expects `Partial<WorkflowTemplate>`, and the parsed workflow IS a `WorkflowTemplate`, so this is a valid call. No `as any` needed.
- The `result.result.isErr()` pattern from the temp commit should be replaced with `isFailure(result)` from the SDK's helper.

---

### 1.2 cloneWorkflow

```
workflow clone <source-id> <target-id> [--name <name>] [--description <desc>]
```

**Required behavior:**
- Fetch source workflow via `api.get(sourceId)`
- Create a new template with target ID, copying all fields from source
- Mark metadata with `clonedFrom` and `clonedAt`
- Register via `api.create(clonedTemplate)`
- Apply name/description overrides if provided
- Return the cloned `WorkflowTemplate`

**Type-safety constraints:**
- `WorkflowMetadata` does NOT have `clonedFrom`/`clonedAt` fields. **Must extend `WorkflowMetadata`** with a new interface rather than using `as any`:

```typescript
interface ClonedWorkflowMetadata extends WorkflowMetadata {
  clonedFrom: string;
  clonedAt: number;
}
```

- `api.update(targetId, { description })` — `description` is already `string | undefined` in `WorkflowTemplate`, so `Partial<WorkflowTemplate>` is satisfied. **No `as any` needed.**

---

### 1.3 rollbackWorkflow

```
workflow rollback <id> --to-version <v> [--confirm]
```

**Required behavior:**
- Fetch current workflow
- Use SDK's versioned update or registry rollback to restore a previous version
- Without `--confirm`: show warning with `--confirm` in the output
- Return the rolled-back `WorkflowTemplate`

**Test expectations:**
- Without `--confirm`: exit code 0, stdout contains `--confirm`
- The rollback operation itself is not yet fully tested (the test only checks the confirmation prompt)

---

### 1.4 listWorkflowVersions

```
workflow show <id> --versions
```

**Required behavior:**
- Fetch version history from the workflow registry
- Return array of `{ version, createdAt, description? }`
- Append version list to the `show` output

**Type-safety constraints:**
- The temp commit used `result.value.map((v: any) => ...)` — this must use proper types from the SDK's version API.

---

### 1.5 deleteWorkflow — Cascade Delete

```
workflow delete <id> [--force] [--cascade]
```

**Required behavior:**
- `--force` alone: skip confirmation prompt
- `--cascade`: find all dependent workflows and delete them first, then delete the target
- Without `--force` and without `--cascade`: refuse with "Use --force to delete"
- When dependents exist and neither `--force` nor `--cascade` is provided: refuse with "Cannot be deleted. Workflow 'X' is referenced by: Y" and suggest `--cascade`
- When `--force` is used without `--cascade` and dependents exist: refuse with "Cannot be deleted. ... Cascade deletion suggestion: Use --cascade"
- Log each cascade deletion with "Cascade Deletion: <id>"

**Test expectations:**
- Delete with dependencies (no cascade): exit code != 0, stderr contains "Cannot be deleted." and "is referenced by"
- Cascade suggestion: stderr contains "--cascade"
- Cascade delete: exit code 0, stdout contains "Workflow is deleted", "Cascade Deletion", both parent and child ids
- Multiple dependents: all deleted
- Delete without force: stderr contains "Use --force to delete"

---

### 1.6 findDependentWorkflows

**Required behavior:**
- Fetch all workflows via `api.getAll()`
- Filter workflows that contain a SUBGRAPH node with `config.subgraphId === id`
- Return array of dependent workflow IDs

**Type-safety constraints:**
- `api.getAll()` returns `WorkflowTemplate[]` — **use `WorkflowTemplate` and `StaticNode` types**, not `any`.
- `StaticNode` has `type` and `config` fields — access them through the proper type.

```typescript
// Correct approach — no `any`
return allWorkflows
  .filter((wf: WorkflowTemplate) =>
    wf.nodes?.some(
      (node: StaticNode) =>
        node.type === "SUBGRAPH" &&
        node.config &&
        (node.config as Record<string, unknown>).subgraphId === id,
    ),
  )
  .map((wf: WorkflowTemplate) => wf.id);
```

---

## 2. CLI Commands (`workflow/index.ts`)

### 2.1 Enhanced `workflow list`

**Options to add:**
- `--type <type>` — filter by workflow type (e.g., STANDALONE, DEPENDENT, TRIGGERED_SUBWORKFLOW)
- `--status <status>` — filter by status (e.g., active)
- `--tag <tag>` — filter by tag
- `--json` — output as JSON array

**Table format enhancement:**
- Add `Type` and `Version` columns to the table output

**Type-safety constraints:**
- Use a proper options interface extending `CommandOptions`, not `any`
- The `--type`/`--status` filters can be applied client-side after fetching from the adapter

**Test expectations:**
- `--type STANDALONE`: exit code 0, contains standalone-wf-001, does NOT contain triggered-wf-001
- `--status active`: exit code 0, contains standalone-wf-001
- `--json`: exit code 0, valid JSON array
- `--table`: exit code 0, enhanced table with Type and Version columns

---

### 2.2 Enhanced `workflow show`

**Options to add:**
- `--json` — output as JSON (using `getFormatter().json()`)
- `--versions` — show version history

**Output format:**
- Default: `formatWorkflow(workflow)` which outputs "Name (id) - Type"
- `--json`: JSON output of the full workflow
- `--versions`: append version history after the main output

**Test expectations:**
- `--json`: exit code 0, contains workflow id, JSON parseable
- `--versions`: exit code 0, contains workflow id

---

### 2.3 Delete Command — Cascade Logic

**Options:**
- `-f, --force` — skip confirmation prompt
- `--cascade` — cascade delete dependent workflows

**Logic flow:**
```
if dependents exist:
  if !force && !cascade → error: "Cannot be deleted. ... is referenced by ... Use --cascade or --force"
  if force && !cascade → error: "Cannot be deleted. ... Cascade deletion suggestion: Use --cascade"
  // cascade=true → proceed with cascade delete
if !force && !cascade → error: "Use --force to delete"
// proceed with delete
```

---

## 3. Output Format (`cli-formatters.ts`)

### 3.1 formatWorkflow

**Default format:** `"Name (id) - Type"`

**Verbose format:** Multi-line with fields:
```
ID: <id>
Name: <name>
Type: <type>
Version: <version>
Description: <description>  (if present)
Status: <status>
Creation time: <createdAt>
Update time: <updatedAt>  (if present)
Number of nodes: <n>  (if nodes present)
Number of sides: <n>  (if edges present)
Number of triggers: <n>  (if triggers present)
```

**Type-safety constraints:**
- `WorkflowSummary` must remain a **proper union type**, not `[key: string]: any`
- Use `in` operator or type guards to check for field existence

### 3.2 Trigger display in `workflow show`

**Format:** `"<trigger-id> (<trigger-name>)"`

Example: `"trigger-001 (On Node Completed)"`

---

## 4. Error Handling (`index.ts`)

### 4.1 Exit Code

Fix the `process.exitCode ||` to use `??`:

```typescript
const exitCode = process.exitCode ?? (commandError ? 1 : 0);
```

---

## 5. Implementation Order

1. **`formatWorkflow`** — fix output format in `cli-formatters.ts` (both default and verbose)
2. **`findDependentWorkflows`** — implement in `workflow-adapter.ts` with proper types
3. **`deleteWorkflow` cascade** — implement cascade logic in adapter
4. **Delete command** — implement cascade/force logic in `workflow/index.ts`
5. **`cloneWorkflow`** — implement in adapter with proper `ClonedWorkflowMetadata` interface
6. **Clone command** — add to `workflow/index.ts`
7. **`updateWorkflow`** — implement in adapter
8. **Update command** — add to `workflow/index.ts`
9. **`rollbackWorkflow`** — implement in adapter
10. **Rollback command** — add to `workflow/index.ts`
11. **Enhanced `list`** — add `--type`, `--status`, `--tag`, `--json`, enhanced table
12. **Enhanced `show`** — add `--json`, `--versions`
13. **`listWorkflowVersions`** — implement in adapter
14. **Error handling** — fix `index.ts` exit code