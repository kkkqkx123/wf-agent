# Remaining Test Issues

## Overview

After rolling back the temp commit's forced source-code changes, 8 test files fail with 32 failing tests. The test files themselves are correct (they describe the expected behavior), but the source code they test against has been reverted to the parent's simple CRUD-only state. Below is the detailed breakdown of every failing test and what needs to be implemented.

---

## 1. Workflow Tests

### 1.1 `03-query.test.ts` — 8 tests fail

| Test | Failure Reason | Required Feature |
|------|---------------|------------------|
| 3.1 show details | Expects `"Standalone Workflow (standalone-wf-001)"` format | `formatWorkflow` must output `"Name (id) - Type"` format |
| 3.1 show all nodes | Expects `--json` flag on `workflow show` | `workflow show --json` option |
| 3.4 verbose list | Expects `"ID: standalone-wf-001"`, `"Version: 1.0.0"`, `"Creation time"`, `"Update time"` | `formatWorkflow` with `verbose: true` must output multi-line field format |
| 3.4 verbose metadata | Expects `"ID: standalone-wf-001"`, `"Version: 1.0.0"` | Same as above |
| 3.6 type filter | Expects `--type STANDALONE` to filter | `workflow list --type <type>` option |
| 3.7 status filter | Expects `--status active` to filter | `workflow list --status <status>` option |
| 3.8 triggers | Expects `"trigger-001 (On Node Completed)"` | `workflow show` must display triggers in `"<id> (<name>)"` format |
| 3.9 JSON output | Expects `--json` flag on `workflow show` | `workflow show --json` option |

### 1.2 `04-deletion.test.ts` — 7 tests fail

| Test | Failure Reason | Required Feature |
|------|---------------|------------------|
| 4.2 delete non-existent | Expects exit code != 0, stderr contains "not found" | `deleteWorkflow` must throw on non-existent ID (currently doesn't — the parent's `api.delete()` doesn't throw on missing ID) |
| 4.3 delete with dependencies | Expects stderr "Cannot be deleted." and "is referenced by" | `findDependentWorkflows` + dependency check in delete command |
| 4.3 cascade suggestion | Expects stderr "--cascade" | Same as above |
| 4.4 cascade delete | Expects exit code 0, "Workflow is deleted", "Cascade Deletion", both ids | Cascade delete implementation in adapter + command |
| 4.4 multiple dependents | Expects all dependent workflows deleted | Same as above |
| 4.5 force option | Expects stderr "Use --force to delete" | Delete without `--force` must refuse |
| 4.6 delete with trigger | Expects "Workflow is deleted" | `deleteWorkflow` must log "Workflow is deleted: <id>" |

### 1.3 `05-workflow-advanced.test.ts` — 9 tests fail

| Test | Failure Reason | Required Feature |
|------|---------------|------------------|
| 1. update | `workflow update` command doesn't exist | `updateWorkflow` adapter + `update` command |
| 1. update non-existent | Same as above | Same as above |
| 2. clone | `workflow clone` command doesn't exist | `cloneWorkflow` adapter + `clone` command |
| 2. clone non-existent | Same as above | Same as above |
| 3. rollback | `workflow rollback` command doesn't exist | `rollbackWorkflow` adapter + `rollback` command |
| 4. JSON list | `workflow list --json` doesn't exist | `list --json` option |
| 4. type filter | `workflow list --type` doesn't exist | `list --type` option |
| 4. tag filter | `workflow list --tag` doesn't exist | `list --tag` option |
| 4. enhanced table | `workflow list --table` must show Type + Version columns | Enhanced table format in `list` command |
| 5. JSON show | `workflow show --json` doesn't exist | `show --json` option |
| 5. version history | `workflow show --versions` doesn't exist | `show --versions` + `listWorkflowVersions` adapter |

---

## 2. Plugin Tests

### 2.1 `plugin-management.test.ts` — 2 tests fail

| Test | Failure Reason | Required Feature |
|------|---------------|------------------|
| List plugins | `plugin list` command returns exit code 1 | `plugin list` command must exist and return 0 |
| Table format | Same as above | Same as above |

**Note:** The `plugin-adapter.ts` and `plugin/index.ts` files were added in the temp commit and are present in the working tree. The `plugin list` command exists but may fail because the adapter or SDK plugin API is not fully functional.

---

## 3. Trigger Template Tests

### 3.1 `trigger-template.test.ts` — 5 tests fail

| Test | Failure Reason | Required Feature |
|------|---------------|------------------|
| Register from TOML | `trigger-template register` command returns exit code 1 | Trigger template registration must work |
| List templates | `trigger-template list` output doesn't contain test template | Trigger template list must show registered templates |
| List with verbose | Same as above | Same as above |
| Show by name | `trigger-template show` command returns exit code 1 | Trigger template show command must work |
| Delete with --force | `trigger-template delete` command returns exit code 1 | Trigger template delete command must work |
| Export as JSON | `trigger-template export` command returns exit code 1 | Trigger template export command must work |

**Note:** The `trigger-adapter.ts` and `trigger/index.ts` files were added in the temp commit and are present. The trigger template infrastructure may need fixes.

---

## 4. Other Test Failures

### 4.1 `logger-initialization.test.ts` — 1 test fails

| Test | Failure Reason |
|------|---------------|
| should warn when configuring an already initialized logger | Expected stderr to have content (exit code > 0), got exit code 0 |

The logger initialization warning test expects a warning when the logger is configured twice. This is a logger infrastructure issue, not related to the workflow rollback.

### 4.2 `test-framework.test.ts` — 1 test fails

| Test | Failure Reason |
|------|---------------|
| should extract IDs from output using patterns | `extractWorkflowId` returns null instead of 'agent-12345' |

The `extractWorkflowId` helper in `workflow-test-helpers.ts` uses a Chinese regex pattern `工作流已注册` which doesn't match the current English output format. This is a test helper issue — the pattern needs to be updated to match `"Workflow is registered"`.

### 4.3 `storage/persistence.test.ts` — 1 test fails

| Test | Failure Reason |
|------|---------------|
| should use isolated storage directories | Storage directory isolation not working as expected |

This is a storage infrastructure issue, not related to the workflow rollback.

---

## 5. Summary

| Category | Files | Tests Failing | Root Cause |
|----------|-------|---------------|------------|
| Workflow commands | 3 test files | 24 | Source code rolled back to parent, features missing |
| Plugin commands | 1 test file | 2 | Plugin infrastructure may need fixes |
| Trigger templates | 1 test file | 5 | Trigger template infrastructure may need fixes |
| Logger | 1 test file | 1 | Logger infrastructure issue |
| Test framework | 1 test file | 1 | Chinese regex pattern doesn't match English output |
| Storage | 1 test file | 1 | Storage infrastructure issue |

**Total: 8 test files, 32 failing tests**

The 24 workflow test failures are the primary focus — they will be resolved by properly reimplementing the features documented in `cli-workflow-features.md`. The remaining 8 failures in other subsystems are separate issues.