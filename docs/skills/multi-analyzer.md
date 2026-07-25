---
name: analyzer-usage
description: "Analyzes build tool errors from Cargo, NPM, PNPM, Yarn, Mypy, Pytest, Ruff, Black, Maven, Gradle, Go, golangci-lint, .NET, Ruby (Rubocop/RSpec), and C++ (CMake/GCC/Clang/MSVC/ClangFormat). Invoke when user asks to analyze build errors, check code quality issues, or use the analyzer CLI tool."
---

# Analyzer - Multi-language Build Tool Error Analyzer

This skill provides guidance on using the analyzer binary to analyze errors from various build tools and generate reports.

## Quick Start

```bash
# Basic usage: analyze a build tool command
analyzer <tech-stack> "<command>" [options]

# Run mode: auto-detect tech stack from raw shell command
analyzer run "<raw_shell_command>" [options]

# Rewrite mode: preview the analyzer-equivalent command for a build tool command
analyzer rewrite "<raw_shell_command>"

# Config management
analyzer config show
analyzer config init

# Statistics
analyzer stats
analyzer stats --reset
```

## Subcommands

| Subcommand | Description |
| ---------- | ----------- |
| `<tech-stack> <command>` | Direct analysis mode (default) |
| `run` | Auto-detect tech stack from raw shell command and execute |
| `rewrite` | Preview the analyzer-equivalent command for a build tool command without executing |
| `config` | Show or initialize configuration file |
| `stats` | Show analysis tracking statistics |

### Run Exit Codes

| Code | Meaning |
| ---- | ------- |
| 0 | Success (rewritten and executed successfully) |
| 1 | No matching rule / execution failed |
| 2 | Subcommand not supported |

### Rewrite Exit Codes

| Code | Meaning |
| ---- | ------- |
| 0 | Successfully rewritten (command printed to stdout) |
| 1 | No matching rule / execution failed |

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
analyzer run "npm run lint" --format json --stdout
analyzer run "pytest -v"
analyzer run "go vet ./..." --format raw --stdout
analyzer run "mvn test" --filter-warnings
analyzer run "cargo test" --verbose
```

### Rewrite Mode (Preview Only)

The `rewrite` subcommand previews what the analyzer-equivalent command would be
**without executing it**. It accepts a raw shell command and prints the equivalent
`analyzer <tech-stack> "<command>"` form to stdout.

**Important constraints:**

- The input must be a **build tool command** from a supported tech stack
  (e.g. `cargo check`, `npm run lint`, `mvn test`).
- **Shell builtins** (`cd`, `ls`, `echo`, `cat`, `cp`, `mv`, `rm`, `mkdir`, etc.)
  are NOT supported and will result in exit code 1.
- **General shell commands** (`git`, `curl`, `wget`, `python`, `node`, etc.)
  that are not in the supported tech stacks will fail.
- **Compound commands** are handled: only the first segment before `&&`, `||`,
  `;`, `|`, or `&` is rewritten. A note is printed for any remaining segments.
- **Environment variable prefixes** (`ENV=val cmd`) are stripped automatically.

```bash
# Supported: build tool commands
analyzer rewrite "cargo check --all-targets"
analyzer rewrite "npm run lint"
analyzer rewrite "go vet ./..."
analyzer rewrite "mvn test"
analyzer rewrite "pytest -v --tb=short"
analyzer rewrite "clang -fsyntax-only main.cpp"

# Unsupportable: shell builtins and general commands
# These will all fail with exit code 1:
#   analyzer rewrite "cd src && cargo check"
#   analyzer rewrite "ls -la"
#   analyzer rewrite "echo hello"
#   analyzer rewrite "git status"

# Compound command: only the first segment is rewritten
# Usage: analyzer rewrite "cargo check && cargo test"
# Output: analyzer cargo "check"  (note about compound command printed)
```

## Global Options

| Option | Description |
| ------ | ----------- |
| `-h, --help` | Show help message |
| `-v, --version` | Show version |
| `--filter-warnings` | Filter out all warnings, only show errors |
| `--filter-paths <paths>` | Filter errors by file paths (comma-separated) |
| `--verbose` | Show all issues without truncation |
| `-q, --quiet` | Minimal output (summary only) |
| `-o, --output <file>` | Specify output file path |
| `--stdout` | Output to stdout only, do not write file |
| `--format <format>` | Report format: `markdown`, `json`, `html`, `raw`, `raw-json` (default: markdown) |
| `--no-short-circuit` | Disable success short-circuit (always show full report) |
| `--max-issues <N>` | Limit analysis to the first N issues (default: unlimited) |

## Report Formats

The analyzer supports multiple output formats, selected via `--format`:

| Format | Description | Extension |
| ------ | ----------- | --------- |
| `markdown` | Human-readable Markdown report with issue grouping, severity levels, and statistics | `.md` |
| `json` | Structured JSON report with metadata, summary, and issue details | `.json` |
| `html` | HTML report with styled output, suitable for CI/browser viewing | `.html` |
| `raw` | Pipe-delimited machine-readable format: `LEVEL\|CODE\|FILE:LINE:COL\|MESSAGE` | `.txt` |
| `raw-json` | JSON lines format (one JSON object per line), suitable for streaming | `.jsonl` |

## Cargo Workspace Options

| Option | Description |
| ------ | ----------- |
| `--workspace` | Analyze all workspace members |
| `-p, --package <SPEC>` | Analyze specific package (can be used multiple times) |
| `--exclude <SPEC>` | Exclude specific package from analysis |

## Cargo Target Options

| Option | Description |
| ------ | ----------- |
| `--lib` | Analyze only the library target |
| `--bin <NAME>` | Analyze specific binary target |
| `--bins` | Analyze all binary targets |
| `--test <NAME>` | Analyze specific test target |
| `--tests` | Analyze all test targets |
| `--example <NAME>` | Analyze specific example target |
| `--examples` | Analyze all example targets |
| `--bench <NAME>` | Analyze specific benchmark target |
| `--benches` | Analyze all benchmark targets |
| `--all-targets` | Analyze all targets |

## Cargo Feature Options

| Option | Description |
| ------ | ----------- |
| `--features <FEATURES>` | Space-separated list of features to enable |
| `--all-features` | Enable all available features |
| `--no-default-features` | Do not enable the default feature |

## Cargo Examples

```bash
# Workspace analysis
analyzer cargo check --workspace
analyzer cargo check --package my-crate

# Target-specific analysis
analyzer cargo check --lib
analyzer cargo check --bin my-app
analyzer cargo check --tests --all-features
analyzer cargo clippy --workspace --all-targets
analyzer cargo check --package foo --features "feat1 feat2"

# With options
analyzer cargo "test" --filter-warnings
analyzer cargo "check" --format json --stdout
```

## C++ Build Options

| Option | Description |
| ------ | ----------- |
| `--source-dir <DIR>` | Source directory for CMake/GCC/Clang builds |
| `--build-dir <DIR>` | Build directory for CMake builds |
| `--cmake-generator <GEN>` | CMake generator (e.g. "Ninja", "Unix Makefiles") |
| `--target <NAME>` | Build target name |
| `--target-files <FILES>` | Comma-separated target source files |
| `-I, --include-path <DIR>` | Add include search path (repeatable) |
| `-D, --define <MACRO>` | Add preprocessor define (repeatable) |
| `--cpp-std <STANDARD>` | C++ standard (e.g. c++17, c++20) |

## Test Analysis

The analyzer detects test subcommands (commands containing "test") and
automatically runs test-specific analysis. When a test command is detected:

1. The test framework is resolved from the configuration (if declared)
2. Test output is parsed for pass/fail/ignore status
3. A combined report is generated showing compile issues + test results

```bash
# Run test analysis with framework detection
analyzer cargo "test" --verbose

# Run with custom output format
analyzer pytest "-v" --format json --stdout

# Test analysis with config-defined framework
# (.analyzer.toml: [tech_stacks.pnpm] test_framework = "vitest")
analyzer pnpm "test" --verbose
```

## Configuration

### Global Configuration

Create `~/.config/analyzer/config.toml` to customize global behavior:

```toml
version = "1.0"

[report]
format = "markdown"
verbosity = "normal"
success_short_circuit = true

[filter]
strip_ansi = true
strip_tui_frames = true
max_lines = 0
max_line_length = 0
noise_patterns = []
keep_patterns = []

[tee]
enabled = true
mode = "failures"
max_files = 20
max_file_size = 1048576
```

### Project-Level Configuration

Create `.analyzer.toml` in your project root to override settings:

```toml
version = "1.0"

[report]
format = "json"
verbosity = "verbose"
success_short_circuit = false

[filter]
strip_ansi = false
noise_patterns = ["warning: unused import"]

[commands.typecheck]
exec = "npm run typecheck"
description = "Run TypeScript type checker"
tech_stacks = ["npm", "pnpm", "yarn"]
enabled = true

[tech_stacks.npm]
test_framework = "jest"

[tech_stacks.pnpm]
test_framework = "vitest"

[tech_stacks.npm.scripts]
test = "jest"
lint = "eslint"
```

### Configuration Sections

| Section | Description |
| ------- | ----------- |
| `[report]` | Report format, verbosity, and short-circuit behavior |
| `[filter]` | Output filtering: ANSI stripping, TUI frame stripping, line limits, noise/keep patterns |
| `[commands.<name>]` | Command aliases: exec command, description, restricted tech stacks, enabled flag |
| `[tech_stacks.<name>]` | Tech stack settings: test framework, script-to-framework mapping |
| `[tee]` | Tee output settings: enable/disable, mode (failures/always/never), file limits |

### Command Aliases

Define custom command aliases in `.analyzer.toml`:

```toml
[commands.lint]
exec = "eslint src/"
description = "Run ESLint on source"
tech_stacks = ["npm", "pnpm", "yarn"]
enabled = true

[commands.ci]
exec = "cargo check --workspace --all-targets"
description = "CI check"
tech_stacks = ["cargo"]
```

Then use them directly:

```bash
analyzer npm "lint"
analyzer cargo "ci"
```

### Script Resolution

Map npm/pnpm/yarn script names to actual test frameworks:

```toml
[tech_stacks.pnpm]
test_framework = "vitest"

[tech_stacks.pnpm.scripts]
test = "vitest run"
lint = "eslint src/"
```

## Command Discovery Engine

The analyzer includes a built-in discovery engine that maps raw shell commands
to tech stacks. This powers the `run` and `rewrite` subcommands.

```bash
# Auto-detect and analyze
analyzer run "cargo check --all-targets"

# Preview what would be analyzed
analyzer rewrite "npm run lint"
```

The discovery engine supports:
- Compound command splitting (&&, ||, ;, |, &) — only the first segment is analyzed
- Configuration-based command aliases
- Pattern matching against a built-in rules table for all supported tech stacks