#!/usr/bin/env python3
"""
Script to fix remaining type errors - Phase 2
"""

import os
import re
from pathlib import Path

SDK_DIR = Path(r"d:\项目\agent\wf-agent\sdk")

def fix_file(file_path: Path, replacements: list) -> bool:
    """Fix a single file with given replacements."""
    if not file_path.is_file():
        return False
    
    if 'dist' in str(file_path) or 'node_modules' in str(file_path):
        return False
    
    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            content = f.read()
        
        original = content
        
        for pattern, replacement in replacements:
            content = re.sub(pattern, replacement, content)
        
        if content != original:
            with open(file_path, 'w', encoding='utf-8') as f:
                f.write(content)
            return True
        
        return False
    except Exception as e:
        print(f"Error: {file_path}: {e}")
        return False

# Fix 1: Replace .getExecution() with .getWorkflowExecutionData()
def fix_get_execution():
    """Fix getExecution method calls."""
    file_path = SDK_DIR / "api/workflow/resources/executions/workflow-execution-registry-api.ts"
    if fix_file(file_path, [
        (r'\.getExecution\(\)', r'.getWorkflowExecutionData()'),
    ]):
        print(f"Fixed: {file_path}")

# Fix 2: Fix event builders - replace workflowExecutionId with threadId in event objects
def fix_event_builders():
    """Fix event builder files."""
    file_path = SDK_DIR / "core/utils/event/builders/workflow-execution-events.ts"
    if fix_file(file_path, [
        (r'workflowExecutionId:', r'threadId:'),
    ]):
        print(f"Fixed: {file_path}")

# Fix 3: Fix task-serializer.ts
def fix_task_serializer():
    """Fix task serializer."""
    file_path = SDK_DIR / "core/serialization/entities/task-serializer.ts"
    if fix_file(file_path, [
        (r'\.threadId', r'.id'),  # Use id instead of threadId
        (r'threadId:', r'id:'),
    ]):
        print(f"Fixed: {file_path}")

# Fix 4: Fix execution-builder.ts
def fix_execution_builder():
    """Fix execution builder."""
    file_path = SDK_DIR / "api/workflow/builders/execution-builder.ts"
    if fix_file(file_path, [
        (r'\.threadId', r'.id'),
    ]):
        print(f"Fixed: {file_path}")

# Fix 5: Fix cancel-workflow-command.ts
def fix_cancel_workflow_command():
    """Fix cancel workflow command."""
    file_path = SDK_DIR / "api/workflow/operations/execution/cancel-workflow-command.ts"
    if fix_file(file_path, [
        (r'\.executionId', r'.workflowExecutionId'),
    ]):
        print(f"Fixed: {file_path}")

# Fix 6: Fix pause-workflow-command.ts
def fix_pause_workflow_command():
    """Fix pause workflow command."""
    file_path = SDK_DIR / "api/workflow/operations/execution/pause-workflow-command.ts"
    if fix_file(file_path, [
        (r'\.executionId', r'.workflowExecutionId'),
    ]):
        print(f"Fixed: {file_path}")

# Fix 7: Fix resume-workflow-command.ts
def fix_resume_workflow_command():
    """Fix resume workflow command."""
    file_path = SDK_DIR / "api/workflow/operations/execution/resume-workflow-command.ts"
    if fix_file(file_path, [
        (r'\.executionId', r'.workflowExecutionId'),
    ]):
        print(f"Fixed: {file_path}")

# Fix 8: Fix workflow-execution-registry-api.ts result object
def fix_registry_api_result():
    """Fix registry API result object."""
    file_path = SDK_DIR / "api/workflow/resources/executions/workflow-execution-registry-api.ts"
    if fix_file(file_path, [
        (r'workflowExecutionId:', r'threadId:'),  # In result objects
    ]):
        print(f"Fixed: {file_path}")

# Fix 9: Fix module paths for workflow tools
def fix_workflow_tool_paths():
    """Fix workflow tool import paths."""
    files = [
        SDK_DIR / "resources/predefined/tools/builtin/workflow/cancel-workflow/handler.ts",
        SDK_DIR / "resources/predefined/tools/builtin/workflow/execute-workflow/handler.ts",
        SDK_DIR / "resources/predefined/tools/builtin/workflow/query-workflow-status/handler.ts",
    ]
    
    for file_path in files:
        if file_path.exists():
            if fix_file(file_path, [
                (r'graph/execution/types/workflow-tool\.types\.js', 
                 r'workflow/execution/types/workflow-tool.types.js'),
                (r'graph/execution/types/triggered-subworkflow\.types\.js', 
                 r'workflow/execution/types/triggered-subworkflow.types.js'),
                (r'graph/execution/handlers/triggered-subworkflow-handler\.js', 
                 r'workflow/execution/handlers/triggered-subworkflow-handler.js'),
            ]):
                print(f"Fixed: {file_path}")

def main():
    print("Phase 2: Fixing remaining type errors...")
    
    fix_get_execution()
    fix_event_builders()
    fix_task_serializer()
    fix_execution_builder()
    fix_cancel_workflow_command()
    fix_pause_workflow_command()
    fix_resume_workflow_command()
    fix_registry_api_result()
    fix_workflow_tool_paths()
    
    print("Phase 2 complete!")

if __name__ == "__main__":
    main()
