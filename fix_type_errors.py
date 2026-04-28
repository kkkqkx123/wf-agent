#!/usr/bin/env python3
"""
Script to fix type errors in SDK by replacing old naming with new naming.
This handles the refactoring from "thread" to "workflow execution" naming.
"""

import os
import re
from pathlib import Path

# Define the SDK directory
SDK_DIR = Path(r"d:\项目\agent\wf-agent\sdk")

# Define replacement patterns for type imports
TYPE_IMPORT_REPLACEMENTS = [
    # ThreadRegistry -> WorkflowExecutionRegistry
    (r'import type \{ ThreadRegistry \}', r'import type { WorkflowExecutionRegistry }'),
    (r'from ["\']([^"\']*workflow-execution-registry[^"\']*)["\']', 
     r'from "\1"'),  # Keep the path, just change the type name
    
    # GraphRegistry -> WorkflowGraphRegistry
    (r'import type \{ GraphRegistry \}', r'import type { WorkflowGraphRegistry }'),
    
    # ThreadEntity -> WorkflowExecutionEntity
    (r'import type \{ ThreadEntity \}', r'import type { WorkflowExecutionEntity }'),
    (r"ThreadEntity", r"WorkflowExecutionEntity"),
]

# Define simple text replacements
TEXT_REPLACEMENTS = [
    # Type names in code
    (r'\bThreadRegistry\b', r'WorkflowExecutionRegistry'),
    (r'\bGraphRegistry\b', r'WorkflowGraphRegistry'),
    (r'\bThreadEntity\b', r'WorkflowExecutionEntity'),
    
    # Property names in CheckpointDependencies
    (r'\bthreadRegistry:', r'workflowExecutionRegistry:'),
    (r'\bgraphRegistry:', r'workflowGraphRegistry:'),
    
    # Property names in CreateCheckpointOptions
    (r'\bexecutionId:', r'workflowExecutionId:'),
    (r'\bthreadId:', r'workflowExecutionId:'),  # Be careful with this one
    
    # Method names
    (r'\.getWorkflowExecution\(\)', r'.getWorkflowExecutionData()'),
    
    # ThreadStateTransitor -> WorkflowStateTransitor
    (r'\bThreadStateTransitor\b', r'WorkflowStateTransitor'),
    
    # ThreadBuilder -> WorkflowExecutionBuilder (in some contexts)
    # (r'\bThreadBuilder\b', r'WorkflowExecutionBuilder'),  # Be careful, might not always apply
]

# Define file-specific replacements
FILE_SPECIFIC_REPLACEMENTS = {
    # Files to skip (don't modify)
    'skip_files': [
        'analysis_report.md',
        '__pycache__',
        '.py',
        'dist',
        'node_modules',
    ],
    
    # Files where threadId should NOT be replaced
    'keep_thread_id': [
        'conversation-session.ts',
        'workflow-conversation-session.ts',
        'message-history.ts',
    ],
}

def should_skip_file(file_path: Path) -> bool:
    """Check if file should be skipped."""
    for skip_pattern in FILE_SPECIFIC_REPLACEMENTS['skip_files']:
        if skip_pattern in str(file_path):
            return True
    return False

def should_keep_thread_id(file_path: Path) -> bool:
    """Check if threadId should be kept in this file."""
    for keep_pattern in FILE_SPECIFIC_REPLACEMENTS['keep_thread_id']:
        if keep_pattern in str(file_path):
            return True
    return False

def fix_imports(content: str, file_path: Path) -> str:
    """Fix import statements."""
    # Fix ThreadRegistry imports
    content = re.sub(
        r'import type \{ ThreadRegistry \} from ["\']([^"\']*)["\']',
        r'import type { WorkflowExecutionRegistry } from "\1"',
        content
    )
    
    # Fix GraphRegistry imports
    content = re.sub(
        r'import type \{ GraphRegistry \} from ["\']([^"\']*)["\']',
        r'import type { WorkflowGraphRegistry } from "\1"',
        content
    )
    
    # Fix ThreadEntity imports
    content = re.sub(
        r'import type \{ ThreadEntity \} from ["\']([^"\']*)["\']',
        r'import type { WorkflowExecutionEntity } from "\1"',
        content
    )
    
    # Fix ThreadStateTransitor imports
    content = re.sub(
        r'from ["\']([^"\']*thread-state-transitor[^"\']*)["\']',
        r'from "\1"',  # Keep path as is for now, will fix below
        content
    )
    
    return content

def fix_type_names(content: str, file_path: Path) -> str:
    """Fix type names in code."""
    # Replace type names (be careful with word boundaries)
    content = re.sub(r'\bThreadRegistry\b', r'WorkflowExecutionRegistry', content)
    content = re.sub(r'\bGraphRegistry\b', r'WorkflowGraphRegistry', content)
    content = re.sub(r'\bThreadEntity\b', r'WorkflowExecutionEntity', content)
    content = re.sub(r'\bThreadStateTransitor\b', r'WorkflowStateTransitor', content)
    
    return content

def fix_property_names(content: str, file_path: Path) -> str:
    """Fix property names in object literals."""
    # Fix CheckpointDependencies properties
    content = re.sub(r'(\s+)threadRegistry:', r'\1workflowExecutionRegistry:', content)
    content = re.sub(r'(\s+)graphRegistry:', r'\1workflowGraphRegistry:', content)
    
    # Fix CreateCheckpointOptions properties (be careful with threadId)
    if not should_keep_thread_id(file_path):
        # Only replace in checkpoint-related contexts
        content = re.sub(r'(\s+)executionId:', r'\1workflowExecutionId:', content)
    
    return content

def fix_method_names(content: str, file_path: Path) -> str:
    """Fix method names."""
    content = re.sub(r'\.getWorkflowExecution\(\)', r'.getWorkflowExecutionData()', content)
    return content

def fix_file(file_path: Path) -> bool:
    """Fix a single file. Returns True if file was modified."""
    if not file_path.is_file():
        return False
    
    if should_skip_file(file_path):
        return False
    
    if not str(file_path).endswith('.ts'):
        return False
    
    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            original_content = f.read()
        
        content = original_content
        
        # Apply fixes
        content = fix_imports(content, file_path)
        content = fix_type_names(content, file_path)
        content = fix_property_names(content, file_path)
        content = fix_method_names(content, file_path)
        
        # Check if content changed
        if content != original_content:
            with open(file_path, 'w', encoding='utf-8') as f:
                f.write(content)
            print(f"Fixed: {file_path}")
            return True
        
        return False
    
    except Exception as e:
        print(f"Error processing {file_path}: {e}")
        return False

def main():
    """Main function to fix all files."""
    print("Starting type error fixes...")
    print(f"SDK directory: {SDK_DIR}")
    
    # Find all TypeScript files
    ts_files = list(SDK_DIR.rglob("*.ts"))
    print(f"Found {len(ts_files)} TypeScript files")
    
    # Fix each file
    fixed_count = 0
    for ts_file in ts_files:
        if fix_file(ts_file):
            fixed_count += 1
    
    print(f"\nFixed {fixed_count} files")

if __name__ == "__main__":
    main()
