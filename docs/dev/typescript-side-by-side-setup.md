# TypeScript Side-by-Side Setup

## Background

TypeScript 7.0 shipped a Go-native compiler that is 8-12x faster than TypeScript 6.x, but **does not ship a stable programmatic API**. Tools like `typescript-eslint` depend on TypeScript's internal JS API (e.g. `ts.Extension.Cjs`) to parse and lint TypeScript code, and those APIs were removed in TS7.

As a result, `typescript-eslint` (peer dependency range `>=4.8.4 <6.1.0`) **cannot run with TypeScript 7** directly.

## Current Solution: npm Alias Side-by-Side

We use [Microsoft's official recommendation](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/#running-side-by-side-with-typescript-6.0) from the TypeScript 7.0 announcement:

```json
{
  "devDependencies": {
    "@typescript/native": "npm:typescript@^7.0.2",
    "typescript": "npm:@typescript/typescript6@^6.0.2"
  }
}
```

| Package name | Resolves to | Used by |
|---|---|---|
| `typescript` | `@typescript/typescript6@6.0.2` (TS6 API) | `typescript-eslint` for linting |
| `@typescript/native` | `typescript@7.0.2` (TS7) | `tsc` for build & type checking |

This ensures:
- **`tsc`** uses TS7 — builds and type checks benefit from 8-12x speedup
- **`typescript-eslint`** uses TS6 API — lint continues to work without crashes

## Cleanup Plan

When **both** conditions are met, this side-by-side setup should be removed:

1. **TypeScript >= 7.1** ships a stable programmatic API (expected ~October 2026)
2. **typescript-eslint** adds TS7 support (tracked in [typescript-eslint#10940](https://github.com/typescript-eslint/typescript-eslint/issues/10940))

### Steps to Clean Up

1. Edit `package.json` — replace the two aliased entries with a single dependency:

```diff
-    "@typescript/native": "npm:typescript@^7.0.2",
-    "typescript": "npm:@typescript/typescript6@^6.0.2",
+    "typescript": "^7.1.0",
```

2. Run `pnpm install --no-frozen-lockfile` to update the lockfile.

3. Run `pnpm lint` to confirm lint passes.

4. Remove this documentation file.
