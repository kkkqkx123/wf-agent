---
name: analyzer-usage
description: "Analyzes build tool errors from Cargo, NPM, PNPM, Yarn, Mypy, Pytest, Maven, Gradle, Go, and C++ (CMake/GCC/Clang/MSVC). Invoke when user asks to analyze build errors, check code quality issues, or use the analyzer CLI tool."
---

# Analyzer - Multi-language Build Tool Error Analyzer

This skill provides guidance on using the analyzer binary to analyze errors from various build tools and generate reports.

## Quick Start

```bash
analyzer <tech-stack> "<full command>"
```

## Supported Tech Stacks

| Tech Stack      | Description                                      |
| --------------- | ------------------------------------------------ |
| Cargo (Rust)    | Rust/Cargo build analyzer                        |
| Mypy (Python)   | Python type checker analyzer                     |
| Pytest (Python) | Python test framework analyzer                   |
| NPM (Node.js)   | Node.js package manager analyzer (npm/pnpm/yarn) |
| Maven (Java)    | Java Maven build analyzer                        |
| Gradle (Java)   | Java Gradle build analyzer                       |
| Go              | Go build and test analyzer                       |
| C++ (CMake)     | C++ CMake build analyzer                         |
| C++ (GCC)       | C++ GCC compiler analyzer                        |
| C++ (Clang)     | C++ Clang compiler analyzer                      |
| C++ (MSVC)      | C++ MSVC compiler analyzer                       |

## Common Usage Examples

```bash
# Rust/Cargo
analyzer cargo "check"
analyzer cargo "clippy --all-targets"
analyzer cargo "test"

# Python/Mypy
analyzer mypy "--show-column-numbers ."
analyzer mypy "--strict ."

# Python/Pytest
analyzer pytest "-v"
analyzer pytest "-v --tb=short"

# Node.js/NPM
analyzer npm "run lint"
analyzer npm "run typecheck"
analyzer npm "audit"

# Node.js/PNPM
analyzer pnpm "run lint"
analyzer pnpm "run typecheck"

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

# C++/CMake
analyzer cmake "--build build"

# C++/GCC
analyzer gcc "-fsyntax-only main.cpp"

# C++/Clang
analyzer clang "-fsyntax-only main.cpp"

# C++/MSVC
analyzer msvc "/Zs main.cpp"
```

## Global Options

| Option                   | Description                                            |
| ------------------------ | ------------------------------------------------------ |
| `-h, --help`             | Show help message                                      |
| `-v, --version`          | Show version                                           |
| `--filter-warnings`      | Filter out all warnings, only show errors              |
| `--filter-paths <paths>` | Filter errors by file paths (comma-separated)          |
| `--verbose`              | Show all issues without truncation                     |
| `-o, --output <file>`    | Specify output file path (default: analysis_report.md) |

## Configuration

Create `.analyzer.toml` in your project root to customize behavior:

```toml
version = "1.0"

[global]
default_format = "markdown"
filter_warnings = false

[commands.typecheck]
exec = "npm run typecheck"
description = "Run TypeScript type checker"
tech_stacks = ["npm", "pnpm", "yarn"]

[tech_stack.npm]
test_framework = "jest"
```

## References

- [Cargo Workspace & Target Options](references/cargo-options.md) - Detailed Cargo-specific options
- [Configuration Guide](references/configuration-guide.md) - How to configure the analyzer
- [Report Formats](references/report-formats.md) - Available output formats and their structure
