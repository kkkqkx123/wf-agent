/**
 * Transform @sdk/* TypeScript path aliases to relative paths in compiled JS output.
 * 
 * tsconfig.json paths:
 *   @sdk/core/*     -> ./core/*
 *   @sdk/services/* -> ./services/*
 *   @sdk/utils/*    -> ./utils/*
 *   @sdk/workflow/* -> ./workflow/*
 *   @sdk/agent/*    -> ./agent/*
 *   @sdk/api/*      -> ./api/*
 *   @/*             -> ./*
 */
const fs = require("fs");
const path = require("path");

const DIST_DIR = path.resolve(__dirname, "../dist");

// Map of alias prefix to the relative directory from SDK root
const ALIAS_MAP = {
  "@sdk/core/":     "core",
  "@sdk/services/": "services",
  "@sdk/utils/":    "utils",
  "@sdk/workflow/": "workflow",
  "@sdk/agent/":    "agent",
  "@sdk/api/":      "api",
};

function collectDistFiles(dir) {
  const results = [];
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...collectDistFiles(fullPath));
    } else if (entry.isFile() && (entry.name.endsWith(".js") || entry.name.endsWith(".d.ts"))) {
      results.push(fullPath);
    }
  }
  return results;
}

/**
 * Compute the relative path from `fromFile` (absolute path of a .js file in dist/)
 * to the target that `@sdk/<module>/<rest>` points to.
 * 
 * Example:
 *   fromFile:  D:/.../sdk/dist/resources/predefined/registration.js
 *   alias:     @sdk/utils/contextual-logger.js
 *   aliasDir:  core -> D:/.../sdk/dist/utils
 *   relative:  ../../utils/contextual-logger.js
 */
function resolveAlias(filePath, importSpecifier) {
  // Remove .js extension from the import specifier for matching
  const specifier = importSpecifier.replace(/\.js$/, "");

  for (const [aliasPrefix, aliasDir] of Object.entries(ALIAS_MAP)) {
    if (specifier.startsWith(aliasPrefix)) {
      const rest = specifier.slice(aliasPrefix.length); // e.g. "contextual-logger"
      // The alias resolves to <dist>/<aliasDir>/<rest>
      // e.g. <dist>/utils/contextual-logger
      const distRoot = DIST_DIR;
      const targetDir = path.resolve(distRoot, aliasDir);

      // from absolute file path, compute relative path
      const fileDir = path.dirname(filePath);
      let relativePath = path.relative(fileDir, path.join(targetDir, rest));

      // Normalize to forward slashes and ensure starts with ./
      relativePath = relativePath.replace(/\\/g, "/");
      if (!relativePath.startsWith(".")) {
        relativePath = "./" + relativePath;
      }

      return relativePath + ".js";
    }
  }

  // Handle @/* alias
  // @/utils/foo -> ./utils/foo
  if (specifier.startsWith("@/") || specifier === "@") {
    const rest = specifier === "@" ? "" : specifier.slice(2);
    const distRoot = DIST_DIR;
    const targetDir = distRoot;
    const fileDir = path.dirname(filePath);
    const targetPath = rest ? path.join(targetDir, rest) : targetDir;
    let relativePath = path.relative(fileDir, targetPath);
    relativePath = relativePath.replace(/\\/g, "/");
    if (!relativePath.startsWith(".")) {
      relativePath = "./" + relativePath;
    }
    return relativePath + ".js";
  }

  return null; // not an alias import
}

function transformFile(filePath) {
  let content = fs.readFileSync(filePath, "utf-8");
  let modified = false;

  // Match import/export statements with string specifiers
  // e.g. import ... from "@sdk/utils/foo.js"
  //      export ... from "@sdk/core/bar.js"
  const importRegex = /(from\s+["'])(@(?:sdk\/[^"']+|@?[^"']*))(["'])/g;

  content = content.replace(importRegex, (match, prefix, specifier, suffix) => {
    const resolved = resolveAlias(filePath, specifier);
    if (resolved) {
      modified = true;
      return `${prefix}${resolved}${suffix}`;
    }
    return match;
  });

  // Handle dynamic import() calls with alias strings
  const dynamicImportRegex = /(import\(["'])(@(?:sdk\/[^"']+|@?[^"']*))(["'])\)/g;
  content = content.replace(dynamicImportRegex, (match, prefix, specifier, suffix) => {
    const resolved = resolveAlias(filePath, specifier);
    if (resolved) {
      modified = true;
      return `${prefix}${resolved}${suffix})`;
    }
    return match;
  });

  if (modified) {
    fs.writeFileSync(filePath, content, "utf-8");
    console.log(`  ✓ Transformed: ${path.relative(DIST_DIR, filePath)}`);
  }

  return modified;
}

function main() {
  console.log("Transforming @sdk/* aliases to relative paths in dist/...");
  const files = collectDistFiles(DIST_DIR);
  console.log(`  Found ${files.length} files`);

  let count = 0;
  for (const file of files) {
    if (transformFile(file)) {
      count++;
    }
  }

  console.log(`  Done. Transformed ${count} file(s).`);
}

main();
