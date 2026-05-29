/**
 * Transform path aliases (@sdk/, @/) to relative paths in compiled JS output.
 * 
 * The SDK tsconfig defines path aliases like:
 *   "@/*": ["./*"]
 *   "@sdk/core/*": ["./core/*"]
 * 
 * tsc compiles imports with these aliases literally, so they need to be 
 * resolved to relative paths for Node.js runtime.
 */

const fs = require("fs");
const path = require("path");

const DIST_DIR = path.resolve(__dirname, "..", "dist");
const SDK_ROOT = path.resolve(__dirname, "..");

// Regex to match: from "@sdk/..." or from "@/..."
// Handles both: import ... from "@sdk/..." and import("@sdk/...")
const ALIAS_RE = /(from\s+['"])(@sdk\/|@\/)([^'"]+)(['"])/g;
const DYNAMIC_IMPORT_RE = /(import\(['"])(@sdk\/|@\/)([^'"]+)(['"]\))/g;

function transformFile(filePath) {
  let content = fs.readFileSync(filePath, "utf-8");
  const dir = path.dirname(filePath);

  let modified = false;

  // Transform static imports
  content = content.replace(ALIAS_RE, (match, prefix, alias, modulePath, suffix) => {
    // Resolve the alias to an absolute path, then compute relative
    const absoluteTarget = path.resolve(SDK_ROOT, modulePath);
    let relativePath = path.relative(dir, absoluteTarget);
    
    // Ensure it starts with ./ or ../
    if (!relativePath.startsWith(".")) {
      relativePath = "./" + relativePath;
    }
    
    // Normalize path separators (use forward slashes)
    relativePath = relativePath.replace(/\\/g, "/");
    
    modified = true;
    return `${prefix}${relativePath}${suffix}`;
  });

  // Transform dynamic imports
  content = content.replace(DYNAMIC_IMPORT_RE, (match, prefix, alias, modulePath, suffix) => {
    const absoluteTarget = path.resolve(SDK_ROOT, modulePath);
    let relativePath = path.relative(dir, absoluteTarget);
    
    if (!relativePath.startsWith(".")) {
      relativePath = "./" + relativePath;
    }
    
    relativePath = relativePath.replace(/\\/g, "/");
    
    modified = true;
    return `${prefix}${relativePath}${suffix}`;
  });

  if (modified) {
    fs.writeFileSync(filePath, content, "utf-8");
    console.log(`  [transform] ${path.relative(SDK_ROOT, filePath)}`);
  }
}

function walkDir(dir) {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walkDir(fullPath);
    } else if (entry.isFile() && (entry.name.endsWith(".js") || entry.name.endsWith(".mjs") || entry.name.endsWith(".cjs"))) {
      transformFile(fullPath);
    }
  }
}

console.log("Transforming path aliases in dist/...");
walkDir(DIST_DIR);
console.log("Done.");
