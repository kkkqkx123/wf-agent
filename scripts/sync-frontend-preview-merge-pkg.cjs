#!/usr/bin/env node
// sync-frontend-preview-merge-pkg.cjs
// Merges web-app's package.json into the preview's package.json for the
// sync-frontend-preview.sh script. Replaces an earlier jq-based merge so the
// script does not require jq to be installed (node is required anyway).
//
// Merge strategy:
//   - name / version / description -> preview identity (passed as argv)
//   - scripts, devDependencies, dependencies, engines, type -> from source
//   - keywords / author / license -> preview's own value, else source
//
// Usage: node sync-frontend-preview-merge-pkg.cjs <src-pkg> <dst-pkg> <name> <version> <description>
"use strict";

const fs = require("node:fs");

const [srcPath, dstPath, nameArg, versionArg, descriptionArg] = process.argv.slice(2);

if (!srcPath || !dstPath) {
	console.error("Usage: node sync-frontend-preview-merge-pkg.cjs <src-pkg> <dst-pkg> <name> <version> <description>");
	process.exit(1);
}

const readJson = (path) => JSON.parse(fs.readFileSync(path, "utf8"));
const src = readJson(srcPath);
const dst = fs.existsSync(dstPath) ? readJson(dstPath) : {};

const pick = (a, b) => (a !== undefined ? a : b);
const pickStr = (a, b) => {
	const v = a || b;
	return v === undefined || v === "" ? undefined : v;
};

const merged = {
	// Preview identity wins (fall back to existing files, then source)
	name: pickStr(nameArg, pickStr(dst.name, src.name)),
	version: pickStr(versionArg, pickStr(dst.version, src.version)),
	description: pickStr(descriptionArg, pickStr(dst.description, src.description)),

	// Structural fields follow source
	type: pick(src.type, dst.type),
	scripts: pick(src.scripts, dst.scripts),
	engines: pick(src.engines, dst.engines),

	// Dependency blocks follow source so new deps propagate
	devDependencies: pick(src.devDependencies, {}),
	dependencies: pick(src.dependencies, {}),

	// Optional metadata - preview keeps its own if present, else source
	keywords: pick(dst.keywords, src.keywords),
	author: pick(dst.author, src.author),
	license: pick(dst.license, src.license),
};

// Drop undefined keys so the output stays clean.
for (const key of Object.keys(merged)) {
	if (merged[key] === undefined) delete merged[key];
}

fs.writeFileSync(`${dstPath}.tmp`, `${JSON.stringify(merged, null, "\t")}\n`);
