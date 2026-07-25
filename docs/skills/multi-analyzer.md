---
name: analyzer-usage
description: "Analyzes build tool errors from Cargo, NPM, PNPM, Yarn, Mypy, Pytest, Ruff, Black, Maven, Gradle, Go, golangci-lint, .NET, Ruby (Rubocop/RSpec), and C++ (CMake/GCC/Clang/MSVC/ClangFormat). Invoke when user asks to analyze build errors, check code quality issues, or use the analyzer CLI tool."
---

# Analyzer - Multi-language Build Tool Error Analyzer

This skill provides guidance on using the analyzer binary to analyze errors from various build tools.

## Quick Start

```bash
# Analyze a single build tool command
analyzer <tech-stack> "<command>" [options]

# Auto-detect tech stack from raw shell command
analyzer run "<raw_shell_command>" [options]

# Preview the equivalent analyzer command without executing
analyzer rewrite "<raw_shell_command>"
```

Results are printed to **stdout** by default (like standard CLI tools).
Use `-o <file>` to write results to a file instead.

## Subcommands

| Subcommand | Description |
| ---------- | ----------- |
| `<tech-stack> <command>` | Direct analysis mode (default) |
| `run` | Auto-detect tech stack from raw shell command and execute |
| `rewrite` | Preview the analyzer-equivalent command without executing |
| `config` | Show or initialize configuration file |
| `stats` | Show analysis tracking statistics |

## Supported Tech Stacks

| Tech Stack | Description | Aliases |
| ---------- | ----------- | ------- |
| Cargo (Rust) | Rust/Cargo build analyzer | `cargo`, `rust` |
| cargo-nextest | Rust nextest test framework | `cargo-nextest`, `nextest` |
| NPM (Node.js) | Node.js package manager analyzer | `npm`, `node` |
| PNPM (Node.js) | PNPM package manager analyzer | `pnpm` |
| Yarn (Node.js) | Yarn package manager analyzer | `yarn` |
| Mypy (Python) | Python type checker analyzer | `mypy` |
| Pytest (Python) | Python test framework analyzer | `pytest`, `py.test` |
| Ruff (Python) | Python linter analyzer | `ruff`, `python-lint` |
| Black (Python) | Python code formatter analyzer | `black` |
| Maven (Java) | Java Maven build analyzer | `maven`, `mvn` |
| Gradle (Java) | Java Gradle build analyzer | `gradle`, `gradlew` |
| Go | Go build and test analyzer | `go`, `golang` |
| golangci-lint | Go linter analyzer | `golangci-lint` |
| .NET | .NET / C# / MSBuild analyzer | `dotnet`, `msbuild`, `csharp` |
| Ruby (Rubocop) | Ruby Rubocop linter analyzer | `rubocop`, `ruby`, `rails` |
| Ruby (RSpec) | Ruby RSpec test analyzer | `rspec` |
| C++ (CMake) | C++ CMake build analyzer | `cmake`, `cmake-build` |
| C++ (GCC) | C++ GCC compiler analyzer | `gcc`, `g++` |
| C++ (Clang) | C++ Clang compiler analyzer | `clang`, `clang++` |
| C++ (MSVC) | C++ MSVC compiler analyzer | `msvc`, `cl` |
| C++ (ClangFormat) | C++ ClangFormat code formatter analyzer | `clang-format` |

## Common Usage Examples

### Direct Analysis Mode

```bash
# Rust/Cargo
analyzer cargo "check"
analyzer cargo "clippy --all-targets"
analyzer cargo "test"
analyzer cargo "build --release"

# Python/Mypy
analyzer mypy "--show-column-numbers ."
analyzer mypy "--strict ."

# Python/Pytest
analyzer pytest "-v"
analyzer pytest "-v --tb=short"

# Python/Ruff
analyzer ruff "check ."
analyzer ruff "format --check ."

# Python/Black
analyzer black "--check ."

# Node.js/NPM
analyzer npm "run lint"
analyzer npm "run typecheck"
analyzer npm "audit"

# Node.js/PNPM
analyzer pnpm "lint"
analyzer pnpm "typecheck"

# Node.js/Yarn
analyzer yarn "run lint"
analyzer yarn "run typecheck"

# Java/Maven
analyzer maven "compile -q"
analyzer maven "test"

# Java/Gradle
analyzer gradle "compileJava --quiet"
analyzer gradle "test"

# Go
analyzer go "build ./..."
analyzer go "vet ./..."
analyzer go "test -v ./..."

# golangci-lint
analyzer golangci-lint "run ./..."

# .NET
analyzer dotnet "build"
analyzer dotnet "test"

# Ruby/Rubocop
analyzer rubocop "."

# Ruby/RSpec
analyzer rspec "spec/"

# C++/CMake
analyzer cmake "--build build"

# C++/GCC
analyzer gcc "-fsyntax-only main.cpp"

# C++/Clang
analyzer clang "-fsyntax-only main.cpp"

# C++/MSVC
analyzer msvc "/Zs main.cpp"

# C++/ClangFormat
analyzer clang-format "--dry-run --Werror main.cpp"
```

### Run Mode (Auto-Detect)

```bash
analyzer run "cargo check --all-targets"
analyzer run "npm run lint" --format json
analyzer run "pytest -v"
analyzer run "go vet ./..." --format raw
analyzer run "mvn test" --filter-warnings
analyzer run "cargo test" --verbose
```

### Rewrite Mode (Preview Only)

The `rewrite` subcommand prints the equivalent `analyzer <tech-stack> "<command>"` form to stdout **without executing it**.

```bash
analyzer rewrite "cargo check --all-targets"
analyzer rewrite "npm run lint"
analyzer rewrite "go vet ./..."
```

**Constraints:**
- Only build tool commands from supported tech stacks are supported
- Shell builtins (`cd`, `ls`, `echo`, etc.) and general commands (`git`, `curl`, etc.) are **not** supported (exit code 1)
- Compound commands (`&&`, `||`, `;`, `|`, `&`) — only the first segment is rewritten
- Environment variable prefixes (`ENV=val cmd`) are stripped automatically

## Common Options

| Option | Description |
| ------ | ----------- |
| `-h, --help` | Show help message |
| `--version` | Show version |
| `--filter-warnings` | Filter out all warnings, only show errors |
| `--filter-paths <paths>` | Filter errors by file paths (comma-separated) |
| `--verbose` | Show detailed progress information on stderr |
| `-q, --quiet` | Suppress all informational messages (stderr) |
| `-o, --output, --file, -f <file>` | Write report to file instead of stdout |
| `--format <format>` | Report format: `markdown`, `json`, `html`, `raw`, `raw-json` (default: markdown) |
| `--no-short-circuit` | Disable success short-circuit (always show full report) |
| `--max-issues <N>` | Limit analysis to the first N issues |

## Report Formats

| Format | Description | Extension |
| ------ | ----------- | --------- |
| `markdown` | Human-readable Markdown report | `.md` |
| `json` | Structured JSON report | `.json` |
| `html` | HTML report for CI/browser viewing | `.html` |
| `raw` | Pipe-delimited: `LEVEL\|CODE\|FILE:LINE:COL\|MESSAGE` | `.txt` |
| `raw-json` | JSON lines (one object per line), streaming-friendly | `.jsonl` |

## Configuration Quick Start

Create `.analyzer.toml` in your project root for per-project settings:

```toml
version = "1.0"

[report]
format = "json"
verbosity = "verbose"

[filter]
noise_patterns = ["warning: unused import"]

[commands.lint]
exec = "eslint src/"
description = "Run ESLint on source"

[tech_stacks.pnpm]
test_framework = "vitest"
```

> **For detailed reference** on all options (Cargo workspace/target/feature flags, C++ build options, test analysis, full configuration sections, command aliases, script resolution, and the command discovery engine), see the [Analyzer Reference](references/analyzer-reference.md) document.
