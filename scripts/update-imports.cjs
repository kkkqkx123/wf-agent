/**
 * Script to update import paths from graph to workflow
 */
const fs = require('fs');
const path = require('path');

const TARGET_DIR = path.join(__dirname, '..', 'sdk', 'workflow');

// Mapping of old imports to new imports
const importMappings = [
  // Path mappings
  { from: /from\s+["']\.\.\/..\/graph-builder\//g, to: 'from "../../builder/' },
  { from: /from\s+["']\.\.\/..\/stores\/thread-registry/g, to: 'from "../../stores/workflow-execution-registry' },
  { from: /from\s+["']\.\.\/..\/stores\/graph-registry/g, to: 'from "../../stores/workflow-graph-registry' },
  { from: /from\s+["']\.\.\/..\/entities\/thread-entity/g, to: 'from "../../entities/workflow-execution-entity' },
  { from: /from\s+["']\.\.\/..\/state-managers\/thread-state-coordinator/g, to: 'from "../../state-managers/workflow-state-coordinator' },
  { from: /from\s+["']\.\.\/..\/checkpoint\//g, to: 'from "../../checkpoint/' },
  { from: /from\s+["']\.\.\/..\/message\/graph-conversation-session/g, to: 'from "../../message/workflow-conversation-session' },
  
  // Relative path fixes for execution subdirectory
  { from: /from\s+["']\.\.\/thread-pool/g, to: 'from "../workflow-execution-pool' },
  { from: /from\s+["']\.\.\/thread-execution-context/g, to: 'from "../workflow-execution-context' },
  { from: /from\s+["']\.\.\/interruption-detector/g, to: 'from "../interruption-detector' },
  
  // Class name mappings (only in specific contexts to avoid over-replacement)
  { from: /type\s+ThreadRegistry\b/g, to: 'type WorkflowExecutionRegistry' },
  { from: /:\s*ThreadRegistry\b/g, to: ': WorkflowExecutionRegistry' },
  { from: /type\s+GraphRegistry\b/g, to: 'type WorkflowGraphRegistry' },
  { from: /:\s*GraphRegistry\b/g, to: ': WorkflowGraphRegistry' },
  { from: /type\s+ThreadStateCoordinator\b/g, to: 'type WorkflowStateCoordinator' },
  { from: /:\s*ThreadStateCoordinator\b/g, to: ': WorkflowStateCoordinator' },
];

function processFile(filePath) {
  let content = fs.readFileSync(filePath, 'utf-8');
  let modified = false;
  
  for (const mapping of importMappings) {
    if (mapping.from.test(content)) {
      content = content.replace(mapping.from, mapping.to);
      modified = true;
    }
  }
  
  if (modified) {
    fs.writeFileSync(filePath, content, 'utf-8');
    console.log(`Updated: ${filePath}`);
  }
}

function walkDir(dir) {
  const files = fs.readdirSync(dir);
  
  for (const file of files) {
    const filePath = path.join(dir, file);
    const stat = fs.statSync(filePath);
    
    if (stat.isDirectory()) {
      walkDir(filePath);
    } else if (file.endsWith('.ts')) {
      processFile(filePath);
    }
  }
}

console.log('Starting import path updates...');
walkDir(TARGET_DIR);
console.log('Done!');
