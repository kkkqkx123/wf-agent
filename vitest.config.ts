// Repository root is a Cargo workspace, not a JavaScript project: it holds
// no package.json and no JS test scope. A bare `vitest` run from here falls
// back to recursive default discovery, compiling the whole tree without any
// package's resolver aliases. Run tests where the project config lives:
//
//   JS web app ... cd apps/web-app && npm test   (aggregator: npm test in apps/)
//   Rust ......... cargo test -p <crate>          (full check: cargo clippy)
//
// This file exists only to fail fast with that pointer instead of letting a
// root-level run waste a full-tree compile.
throw new Error(
  'No JS test project at the repository root; run `npm test` inside apps/web-app (JS) or `cargo test -p <crate>` (Rust).',
);
